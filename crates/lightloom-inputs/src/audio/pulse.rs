// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux: the monitor of an output sink over the native PulseAudio protocol
//! (`pulseaudio` crate, pure Rust). PulseAudio serves it, and so does
//! PipeWire through `pipewire-pulse` on current desktops, and a Flatpak with
//! `--socket=pulseaudio`.
//!
//! * Only **sinks** are listed and chosen. What is recorded is the sink's own
//!   monitor source (`SinkInfo.monitor_source_index`), and before the stream
//!   is created the source is checked to be the monitor of that sink
//!   (`SourceInfo.monitor_of_sink_index`). A microphone or any other source
//!   is never opened.
//! * The stream asks for f32 in the sink's rate and channel map, with 10 ms
//!   fragments. A reader thread converts each data packet and pushes whole
//!   frames into the same lock-free ring as the Windows loopback.
//! * Following the default output: the connection subscribes to server
//!   events; when the server changes (a new default sink) the source reports
//!   itself gone and [`super::AudioInput`] reopens it 300 ms later on the new
//!   default. A named sink keeps its stream. A killed stream, a closed socket
//!   or a protocol error also ends the source.

use super::SampleSource;
use pulseaudio::protocol;
use std::ffi::CString;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Read/write timeout of the request/reply phase (a hung server must not
/// hang the audio thread, which [`super::AudioInput::stop`] joins).
const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// `PA_NATIVE_COOKIE_LENGTH`.
const COOKIE_LEN: usize = 256;
/// How often the reader thread wakes without data to check for a stop.
const POLL: Duration = Duration::from_millis(250);

/// The server socket: `PULSE_SERVER` (`unix:/path` or a bare path), else
/// `$PULSE_RUNTIME_PATH/native`, else `$XDG_RUNTIME_DIR/pulse/native`; the
/// first that exists.
fn socket_path(env: impl Fn(&str) -> Option<String>, exists: impl Fn(&PathBuf) -> bool) -> Option<PathBuf> {
    let server = env("PULSE_SERVER").and_then(|s| {
        let s = s.trim();
        // Only local sockets: `tcp:host` and lists of servers are not supported.
        let path = s.strip_prefix("unix:").unwrap_or(s);
        path.starts_with('/').then(|| PathBuf::from(path))
    });
    server
        .into_iter()
        .chain(env("PULSE_RUNTIME_PATH").map(|p| PathBuf::from(p).join("native")))
        .chain(env("XDG_RUNTIME_DIR").map(|p| PathBuf::from(p).join("pulse").join("native")))
        .find(exists)
}

/// A connected, authenticated client (blocking, with timeouts).
struct Connection {
    sock: BufReader<UnixStream>,
    version: u16,
    seq: u32,
}

impl Connection {
    fn open(name: &str) -> Result<Self, String> {
        let path = socket_path(
            |k| std::env::var(k).ok().filter(|v| !v.is_empty()),
            |p| p.exists(),
        )
        .ok_or(
            "no PulseAudio or pipewire-pulse server socket (PULSE_SERVER, XDG_RUNTIME_DIR/pulse/native)",
        )?;
        let sock =
            UnixStream::connect(&path).map_err(|e| format!("connecting to {}: {e}", path.display()))?;
        sock.set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|e| e.to_string())?;
        sock.set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|e| e.to_string())?;
        // PulseAudio rejects an auth without exactly 256 cookie bytes (zeros
        // when there is no cookie file, as libpulse sends).
        let cookie = pulseaudio::cookie_path_from_env()
            .and_then(|p| std::fs::read(p).ok())
            .filter(|c| c.len() == COOKIE_LEN)
            .unwrap_or_else(|| vec![0; COOKIE_LEN]);
        let mut c = Self {
            sock: BufReader::new(sock),
            version: protocol::MAX_VERSION,
            seq: 0,
        };
        let auth: protocol::AuthReply = c.request(protocol::Command::Auth(protocol::AuthParams {
            version: protocol::MAX_VERSION,
            supports_shm: false,
            supports_memfd: false,
            cookie,
        }))?;
        c.version = protocol::MAX_VERSION.min(auth.version);
        let mut props = protocol::Props::new();
        props.set(
            protocol::Prop::ApplicationName,
            CString::new(name).unwrap_or_default(),
        );
        props.set(
            protocol::Prop::ApplicationProcessId,
            CString::new(std::process::id().to_string()).unwrap_or_default(),
        );
        let _: protocol::SetClientNameReply = c.request(protocol::Command::SetClientName(props))?;
        Ok(c)
    }

    fn send(&mut self, cmd: &protocol::Command) -> Result<u32, String> {
        self.seq += 1;
        protocol::write_command_message(self.sock.get_mut(), self.seq, cmd, self.version)
            .map_err(|e| format!("PulseAudio request: {e}"))?;
        Ok(self.seq)
    }

    /// The whole message (descriptor and payload) answering request `seq`.
    /// Data packets and server events that arrive first are skipped.
    fn reply_to(&mut self, seq: u32) -> Result<Vec<u8>, String> {
        let err = |e: std::io::Error| format!("PulseAudio reply: {e}");
        loop {
            let mut msg = vec![0; protocol::DESCRIPTOR_SIZE];
            self.sock.read_exact(&mut msg).map_err(err)?;
            let word = |i: usize| u32::from_be_bytes([msg[i], msg[i + 1], msg[i + 2], msg[i + 3]]);
            let (length, channel) = (word(0) as usize, word(4));
            msg.resize(protocol::DESCRIPTOR_SIZE + length, 0);
            self.sock
                .read_exact(&mut msg[protocol::DESCRIPTOR_SIZE..])
                .map_err(err)?;
            if channel != u32::MAX {
                continue;
            }
            let payload = &mut Cursor::new(&msg[protocol::DESCRIPTOR_SIZE..]);
            if let Ok((got, protocol::Command::Reply | protocol::Command::Error(_))) =
                protocol::Command::read_tag_prefixed(payload, self.version)
            {
                if got == seq {
                    return Ok(msg);
                }
            }
        }
    }

    fn request<R: protocol::CommandReply>(&mut self, cmd: protocol::Command) -> Result<R, String> {
        let seq = self.send(&cmd)?;
        let msg = self.reply_to(seq)?;
        protocol::read_reply_message::<R>(&mut Cursor::new(&msg[..]), self.version)
            .map(|(_, reply)| reply)
            .map_err(|e| format!("PulseAudio reply: {e}"))
    }

    fn ack(&mut self, cmd: protocol::Command) -> Result<(), String> {
        let seq = self.send(&cmd)?;
        let msg = self.reply_to(seq)?;
        protocol::read_ack_message(&mut Cursor::new(&msg[..]))
            .map(drop)
            .map_err(|e| format!("PulseAudio reply: {e}"))
    }

    fn sinks(&mut self) -> Result<Vec<Sink>, String> {
        let list: protocol::SinkInfoList = self.request(protocol::Command::GetSinkInfoList)?;
        Ok(list.iter().map(Sink::from).collect())
    }

    fn default_sink(&mut self) -> Result<Option<String>, String> {
        let info: protocol::ServerInfo = self.request(protocol::Command::GetServerInfo)?;
        Ok(info.default_sink_name.map(|n| n.to_string_lossy().into_owned()))
    }
}

/// What this module needs of a sink.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Sink {
    index: u32,
    /// Server name (`alsa_output.pci-0000_0b_00.4.analog-stereo`).
    name: String,
    /// Human name (`Starship/Matisse HD Audio Controller Analog Stereo`).
    description: Option<String>,
    /// Its monitor source.
    monitor: Option<u32>,
    rate: u32,
    channels: u8,
    map: protocol::ChannelMap,
}

impl From<&protocol::SinkInfo> for Sink {
    fn from(s: &protocol::SinkInfo) -> Self {
        Self {
            index: s.index,
            name: s.name.to_string_lossy().into_owned(),
            description: s.description.as_ref().map(|d| d.to_string_lossy().into_owned()),
            monitor: s.monitor_source_index,
            rate: s.sample_spec.sample_rate,
            channels: s.channel_map.num_channels(),
            map: s.channel_map,
        }
    }
}

impl Sink {
    /// The name shown in the device picker.
    fn label(&self) -> &str {
        self.description
            .as_deref()
            .filter(|d| !d.is_empty())
            .unwrap_or(&self.name)
    }
}

/// The sink `wanted` names (its description or its server name), or the
/// default sink, or the first one.
fn pick<'a>(sinks: &'a [Sink], wanted: Option<&str>, default: Option<&str>) -> Result<&'a Sink, String> {
    match wanted.map(str::trim).filter(|w| !w.is_empty()) {
        Some(w) => sinks
            .iter()
            .find(|s| s.label() == w)
            .or_else(|| sinks.iter().find(|s| s.name == w))
            .ok_or_else(|| format!("no output device named {w:?}")),
        None => default
            .and_then(|d| sinks.iter().find(|s| s.name == d))
            .or(sinks.first())
            .ok_or_else(|| "no output device".to_string()),
    }
}

/// Names of the output devices (sinks), as the picker shows them.
pub fn output_devices() -> Vec<String> {
    match Connection::open("Lightloom").and_then(|mut c| c.sinks()) {
        Ok(sinks) => sinks.iter().map(|s| s.label().to_string()).collect(),
        Err(e) => {
            tracing::debug!("audio: cannot list the output devices: {e}");
            Vec::new()
        }
    }
}

/// Converts f32 LE bytes to samples, carrying a partial frame over to the
/// next packet so channels stay aligned.
#[derive(Debug, Default)]
struct Decoder {
    carry: Vec<u8>,
    samples: Vec<f32>,
}

impl Decoder {
    /// Whole frames of `bytes` (after any carried-over part) into `self.samples`.
    fn feed(&mut self, bytes: &[u8], channels: usize) -> &[f32] {
        let frame = 4 * channels.max(1);
        self.samples.clear();
        let mut rest = bytes;
        if !self.carry.is_empty() {
            let take = (frame - self.carry.len()).min(rest.len());
            self.carry.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.carry.len() < frame {
                return &self.samples;
            }
            let carried = std::mem::take(&mut self.carry);
            self.push(&carried);
        }
        let whole = rest.len() / frame * frame;
        self.push(&rest[..whole]);
        self.carry.extend_from_slice(&rest[whole..]);
        &self.samples
    }

    fn push(&mut self, bytes: &[u8]) {
        self.samples
            .extend(bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)));
    }
}

/// What ended the reader thread.
type Ended = Arc<Mutex<Option<String>>>;

/// A record stream on a sink's monitor feeding a lock-free ring.
pub struct MonitorSource {
    rx: rtrb::Consumer<f32>,
    fs: u32,
    channels: u16,
    stop: Arc<AtomicBool>,
    ended: Ended,
    /// A clone of the socket, shut down to wake the reader.
    sock: UnixStream,
    reader: Option<JoinHandle<()>>,
}

impl MonitorSource {
    /// Record the monitor of `device` (a sink's description or name; `None`/empty: the default sink).
    pub fn open(device: Option<&str>) -> Result<Self, String> {
        let mut c = Connection::open("Lightloom")?;
        let follow_default = device.is_none_or(|d| d.trim().is_empty());
        let sinks = c.sinks()?;
        let default = c.default_sink()?;
        let sink = pick(&sinks, device, default.as_deref())?.clone();
        let monitor = sink
            .monitor
            .ok_or_else(|| format!("{:?} has no monitor source", sink.label()))?;
        // Never a microphone: the source must be this sink's monitor.
        let source: protocol::SourceInfo =
            c.request(protocol::Command::GetSourceInfo(protocol::GetSourceInfo {
                index: Some(monitor),
                name: None,
            }))?;
        if source.monitor_of_sink_index != Some(sink.index) {
            return Err(format!(
                "source {monitor} is not the monitor of {:?}",
                sink.label()
            ));
        }
        if follow_default {
            c.ack(protocol::Command::Subscribe(protocol::SubscriptionMask::SERVER))?;
        }
        let channels = sink.channels.clamp(1, 32);
        let fs = sink.rate.max(1);
        let frame_bytes = 4 * u32::from(channels);
        let mut props = protocol::Props::new();
        props.set(protocol::Prop::MediaName, c"Lightloom audio effects");
        let params = protocol::RecordStreamParams {
            sample_spec: protocol::SampleSpec {
                format: protocol::SampleFormat::Float32Le,
                channels,
                sample_rate: fs,
            },
            channel_map: sink.map,
            source_index: Some(monitor),
            buffer_attr: protocol::stream::BufferAttr {
                // 10 ms packets: the analysis wakes at up to 60 Hz.
                fragment_size: (fs / 100).max(1) * frame_bytes,
                ..Default::default()
            },
            flags: protocol::stream::StreamFlags {
                adjust_latency: true,
                // A named sink stays put; the default one is reopened on change.
                no_move: !follow_default,
                ..Default::default()
            },
            props,
            ..Default::default()
        };
        let reply: protocol::CreateRecordStreamReply =
            c.request(protocol::Command::CreateRecordStream(params))?;
        let fs = reply.sample_spec.sample_rate;
        let channels = u16::from(reply.sample_spec.channels.max(1));
        if reply.sample_spec.format != protocol::SampleFormat::Float32Le {
            return Err(format!(
                "the server sends {:?}, not f32",
                reply.sample_spec.format
            ));
        }
        tracing::info!(
            "audio: monitor of {:?} ({}, {fs} Hz, {channels} ch)",
            sink.label(),
            sink.name
        );
        let sock = c.sock.get_ref().try_clone().map_err(|e| e.to_string())?;
        c.sock
            .get_ref()
            .set_read_timeout(Some(POLL))
            .map_err(|e| e.to_string())?;
        // Half a second of headroom; the analysis drains it every tick.
        let (tx, rx) = rtrb::RingBuffer::<f32>::new((fs as usize * usize::from(channels) / 2).max(4096));
        let stop = Arc::new(AtomicBool::new(false));
        let ended: Ended = Arc::default();
        let reader = Reader {
            sock: c.sock,
            version: c.version,
            stream: reply.channel,
            channels: usize::from(channels),
            follow_default,
            tx,
            stop: Arc::clone(&stop),
        };
        let done = Arc::clone(&ended);
        let reader = thread::Builder::new()
            .name("lightloom-pulse".into())
            .spawn(move || {
                let why = reader.run();
                *done.lock().unwrap_or_else(|e| e.into_inner()) = Some(why);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            rx,
            fs,
            channels,
            stop,
            ended,
            sock,
            reader: Some(reader),
        })
    }
}

impl Drop for MonitorSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.sock.shutdown(std::net::Shutdown::Both);
        if let Some(t) = self.reader.take() {
            let _ = t.join();
        }
    }
}

impl SampleSource for MonitorSource {
    fn sample_rate(&self) -> u32 {
        self.fs
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn read(&mut self, out: &mut Vec<f32>) -> Result<(), String> {
        let n = self.rx.slots() / usize::from(self.channels) * usize::from(self.channels);
        if let Ok(chunk) = self.rx.read_chunk(n) {
            let (a, b) = chunk.as_slices();
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            chunk.commit_all();
        }
        match self.ended.lock().unwrap_or_else(|e| e.into_inner()).take() {
            Some(why) => Err(why),
            None => Ok(()),
        }
    }
}

/// The socket after the stream was created: data packets and server events.
struct Reader {
    sock: BufReader<UnixStream>,
    version: u16,
    stream: u32,
    channels: usize,
    follow_default: bool,
    tx: rtrb::Producer<f32>,
    stop: Arc<AtomicBool>,
}

impl Reader {
    /// Runs until the stream ends; returns why.
    fn run(mut self) -> String {
        let mut decoder = Decoder::default();
        let mut payload = Vec::new();
        while !self.stop.load(Ordering::Acquire) {
            // Wait for the next packet without consuming anything, so a
            // timeout here is harmless.
            match self.sock.fill_buf() {
                Ok([]) => return "the server closed the connection".into(),
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return format!("reading from the server: {e}"),
            }
            let _ = self.sock.get_ref().set_read_timeout(Some(IO_TIMEOUT));
            let packet = protocol::read_descriptor(&mut self.sock)
                .map_err(|e| e.to_string())
                .and_then(|d| {
                    payload.resize(d.length as usize, 0);
                    self.sock.read_exact(&mut payload).map_err(|e| e.to_string())?;
                    Ok(d.channel)
                });
            let _ = self.sock.get_ref().set_read_timeout(Some(POLL));
            let channel = match packet {
                Ok(c) => c,
                Err(e) => return format!("reading from the server: {e}"),
            };
            if channel == self.stream {
                let samples = decoder.feed(&payload, self.channels);
                // A packet that does not fit whole is dropped (the analysis is behind).
                if self.tx.slots() >= samples.len() {
                    let _ = self.tx.push_partial_slice(samples);
                }
            } else if channel == u32::MAX {
                if let Some(why) = self.event(&payload) {
                    return why;
                }
            }
        }
        "stopped".into()
    }

    /// A command from the server: `Some(reason)` when the stream is over.
    fn event(&self, payload: &[u8]) -> Option<String> {
        let (_, cmd) = protocol::Command::read_tag_prefixed(&mut Cursor::new(payload), self.version).ok()?;
        match cmd {
            protocol::Command::RecordStreamKilled(c) if c == self.stream => {
                Some("the server ended the stream".into())
            }
            protocol::Command::SubscribeEvent(e)
                if self.follow_default && e.event_facility == protocol::SubscriptionEventFacility::Server =>
            {
                Some("the server settings changed (default output?)".into())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink(index: u32, name: &str, description: Option<&str>) -> Sink {
        Sink {
            index,
            name: name.into(),
            description: description.map(Into::into),
            monitor: Some(index + 100),
            rate: 48_000,
            channels: 2,
            map: protocol::ChannelMap::default(),
        }
    }

    #[test]
    fn picks_a_sink_by_description_name_or_default() {
        let sinks = [
            sink(1, "alsa_output.hdmi", Some("HDMI Audio")),
            sink(2, "alsa_output.analog", Some("Speakers")),
            sink(3, "bluez_output.xx", None),
        ];
        assert_eq!(pick(&sinks, Some("Speakers"), None).unwrap().index, 2);
        assert_eq!(pick(&sinks, Some(" alsa_output.hdmi "), None).unwrap().index, 1);
        assert_eq!(pick(&sinks, Some("bluez_output.xx"), None).unwrap().index, 3);
        assert_eq!(pick(&sinks, None, Some("alsa_output.analog")).unwrap().index, 2);
        assert_eq!(pick(&sinks, Some(""), Some("gone")).unwrap().index, 1);
        assert!(pick(&sinks, Some("Microphone"), None).is_err());
        assert!(pick(&[], None, None).is_err());
        assert_eq!(sinks[2].label(), "bluez_output.xx");
    }

    #[test]
    fn decoder_keeps_frames_whole_across_packets() {
        let bytes: Vec<u8> = [0.5f32, -0.5, 0.25, -0.25, 1.0, -1.0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let mut d = Decoder::default();
        // 1.5 frames, then the rest: no sample is split, channels stay aligned.
        assert_eq!(d.feed(&bytes[..12], 2), &[0.5, -0.5]);
        assert_eq!(d.feed(&bytes[12..13], 2), &[] as &[f32]);
        assert_eq!(d.feed(&bytes[13..], 2), &[0.25, -0.25, 1.0, -1.0]);
        assert!(d.carry.is_empty());
    }

    #[test]
    fn socket_path_follows_the_pulse_environment() {
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |k: &str| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        let any = |_: &PathBuf| true;
        assert_eq!(
            socket_path(env(&[("PULSE_SERVER", "unix:/mnt/wslg/PulseServer")]), any),
            Some(PathBuf::from("/mnt/wslg/PulseServer"))
        );
        assert_eq!(
            socket_path(
                env(&[
                    ("PULSE_SERVER", "tcp:host"),
                    ("XDG_RUNTIME_DIR", "/run/user/1000")
                ]),
                any
            ),
            Some(PathBuf::from("/run/user/1000/pulse/native"))
        );
        let only_xdg = |p: &PathBuf| p.starts_with("/run/user");
        assert_eq!(
            socket_path(
                env(&[
                    ("PULSE_RUNTIME_PATH", "/gone"),
                    ("XDG_RUNTIME_DIR", "/run/user/1000")
                ]),
                only_xdg
            ),
            Some(PathBuf::from("/run/user/1000/pulse/native"))
        );
        assert_eq!(socket_path(env(&[]), any), None);
    }

    /// Manual check against a running server (never run by `cargo test`):
    /// lists the sinks and records the default sink's monitor for 1 s.
    /// Prints only levels, never samples.
    /// `cargo test -p lightloom-inputs pulse -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a PulseAudio or pipewire-pulse server"]
    fn record_the_default_monitor() {
        println!("sinks: {:?}", output_devices());
        let mut s = MonitorSource::open(None).expect("open");
        let mut buf = Vec::new();
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(100));
            s.read(&mut buf).expect("read");
        }
        let rms = (buf.iter().map(|v| v * v).sum::<f32>() / buf.len().max(1) as f32).sqrt();
        println!(
            "{} samples at {} Hz x {}, rms {rms}",
            buf.len(),
            s.sample_rate(),
            s.channels()
        );
        assert!(buf.len() > s.sample_rate() as usize / 2);
    }
}
