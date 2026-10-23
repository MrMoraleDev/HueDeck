// SPDX-License-Identifier: GPL-3.0-or-later
//! `engine.audio`: loopback capture of what the PC plays, analyzed once per
//! tick into an [`AudioSnapshot`] handed to a sink (the daemon forwards it to
//! the renderer). See docs/audio.md.
//!
//! * [`analyzer`]: the DSP (FFT, 200 bands, level, width, density).
//! * [`AudioInput`]: a thread that drains a [`SampleSource`] at a fixed rate,
//!   runs the analyzer and calls the sink. A source that fails is reopened
//!   (300 ms after a stream error, every 2 s while opening fails); a source
//!   that delivers nothing (WASAPI loopback sends no packets while nothing
//!   plays) is read as silence after two ticks.
//! * On Windows, [`loopback`] opens the default (or a named) **output**
//!   device through cpal's WASAPI loopback. Microphones and other capture
//!   endpoints are never opened.

pub mod analyzer;
#[cfg(windows)]
mod wasapi;

use crate::AudioSnapshot;
pub use analyzer::Analyzer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How the 200 `freq` bands are spaced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BandScale {
    /// 20 Hz-20 kHz, log-spaced (66.7 bands per decade), with a 3 dB/octave
    /// tilt: every part of the spectrum moves with music.
    #[default]
    Log,
    /// 0-fs/2 in equal steps (120 Hz at 48 kHz): most music energy lands in
    /// the first few bands.
    Linear,
}

impl BandScale {
    /// `"log"` / `"linear"` (case-insensitive); anything else is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "log" | "logarithmic" => Some(Self::Log),
            "linear" => Some(Self::Linear),
            _ => None,
        }
    }
}

/// Interleaved f32 samples from somewhere (a loopback stream, a test signal).
pub trait SampleSource: Send {
    /// Frames per second.
    fn sample_rate(&self) -> u32;
    /// Interleaved channels.
    fn channels(&self) -> u16;
    /// Append every sample captured since the last call to `out`. `Err`: the
    /// stream is gone and the source must be reopened.
    fn read(&mut self, out: &mut Vec<f32>) -> Result<(), String>;
}

/// Opens a source (called again after it fails).
pub type OpenSource = Box<dyn FnMut() -> Result<Box<dyn SampleSource>, String> + Send>;
/// Receives one snapshot per tick.
pub type Sink = Box<dyn FnMut(&AudioSnapshot) + Send>;

/// Wait after a stream error before reopening.
const REOPEN_AFTER_ERROR: Duration = Duration::from_millis(300);
/// Wait between failed opens.
const RETRY_OPEN: Duration = Duration::from_secs(2);

/// Names of the output devices loopback can capture (Windows; empty elsewhere).
pub fn output_devices() -> Vec<String> {
    #[cfg(windows)]
    {
        wasapi::output_devices()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// An [`OpenSource`] for loopback of `device` (an output device name as
/// [`output_devices`] lists it; `None` or empty: the default output).
pub fn loopback(device: Option<String>) -> OpenSource {
    #[cfg(windows)]
    {
        Box::new(move || {
            wasapi::LoopbackSource::open(device.as_deref()).map(|s| Box::new(s) as Box<dyn SampleSource>)
        })
    }
    #[cfg(not(windows))]
    {
        let _ = device;
        Box::new(|| Err("audio capture is not available on this platform yet".into()))
    }
}

/// A running audio input (see the module docs); stops when dropped.
pub struct AudioInput {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for AudioInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioInput")
            .field("running", &self.thread.is_some())
            .finish()
    }
}

impl AudioInput {
    /// Start analyzing `open`'s samples `rate_hz` times per second (clamped
    /// to 10..=120) and calling `sink` with each snapshot.
    pub fn start(rate_hz: u32, scale: BandScale, open: OpenSource, sink: Sink) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let period = Duration::from_secs_f64(1.0 / f64::from(rate_hz.clamp(10, 120)));
        let thread = thread::Builder::new()
            .name("lightloom-audio".into())
            .spawn(move || run(&flag, period, scale, open, sink))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    /// Stop and join the thread (the source closes with it).
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for AudioInput {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(stop: &AtomicBool, period: Duration, scale: BandScale, mut open: OpenSource, mut sink: Sink) {
    let mut source: Option<Box<dyn SampleSource>> = None;
    let mut analyzer: Option<(Analyzer, u32, u16)> = None;
    let mut open_at = Instant::now();
    let mut open_failed = false;
    let mut buf = Vec::new();
    let mut last_data = Instant::now();
    let mut last_tick = Instant::now();
    let mut next = Instant::now();
    let mut idle_sent = false;
    while !stop.load(Ordering::Acquire) {
        next += period;
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        } else {
            next = now;
        }
        let now = Instant::now();
        if source.is_none() && now >= open_at {
            match open() {
                Ok(s) => {
                    let (fs, ch) = (s.sample_rate(), s.channels());
                    tracing::info!("audio: capturing at {fs} Hz, {ch} channels");
                    if !matches!(&analyzer, Some((_, f, c)) if *f == fs && *c == ch) {
                        analyzer = Some((Analyzer::new(fs, ch, scale), fs, ch));
                    }
                    source = Some(s);
                    open_failed = false;
                    last_data = now;
                }
                Err(e) => {
                    if !open_failed {
                        tracing::warn!("audio: cannot open the loopback capture: {e}");
                    }
                    open_failed = true;
                    open_at = now + RETRY_OPEN;
                }
            }
        }
        buf.clear();
        if let Some(s) = source.as_mut() {
            if let Err(e) = s.read(&mut buf) {
                tracing::info!("audio: capture stopped ({e}); reopening");
                source = None;
                open_at = now + REOPEN_AFTER_ERROR;
            }
        }
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        let Some((a, fs, ch)) = analyzer.as_mut() else {
            if !idle_sent {
                sink(&AudioSnapshot::default());
                idle_sent = true;
            }
            continue;
        };
        if !buf.is_empty() {
            last_data = now;
        } else if source.is_none() || now.duration_since(last_data) > period * 2 {
            // Loopback delivers no packets while nothing plays: that is silence.
            let frames = (*fs as f32 * dt).round() as usize;
            buf.resize(frames * usize::from(*ch), 0.0);
        }
        sink(&a.process(&buf, dt));
    }
}

#[cfg(test)]
mod tests {
    use super::analyzer::tests::{sine, FS};
    use super::*;
    use std::sync::mpsc;

    /// A source producing a 1 kHz sine in real time (by elapsed time).
    struct FakeSine {
        started: Instant,
        sent: usize,
        fail_after: Option<usize>,
    }

    impl SampleSource for FakeSine {
        fn sample_rate(&self) -> u32 {
            FS
        }
        fn channels(&self) -> u16 {
            2
        }
        fn read(&mut self, out: &mut Vec<f32>) -> Result<(), String> {
            if self.fail_after.is_some_and(|n| self.sent >= n) {
                return Err("gone".into());
            }
            let due = (self.started.elapsed().as_secs_f64() * f64::from(FS)) as usize;
            out.extend(sine(1000.0, 0.03, due - self.sent, self.sent));
            self.sent = due;
            Ok(())
        }
    }

    fn collect(open: OpenSource, ticks: usize) -> Vec<AudioSnapshot> {
        let (tx, rx) = mpsc::channel();
        let mut input = AudioInput::start(
            60,
            BandScale::Log,
            open,
            Box::new(move |s| {
                let _ = tx.send(s.clone());
            }),
        )
        .unwrap();
        let snaps: Vec<_> = (0..ticks)
            .map(|_| {
                rx.recv_timeout(Duration::from_secs(5))
                    .expect("a snapshot per tick")
            })
            .collect();
        input.stop();
        snaps
    }

    fn peak_band(s: &AudioSnapshot) -> usize {
        (0..s.freq.len()).max_by_key(|&i| s.freq[i]).unwrap()
    }

    #[test]
    fn pushes_snapshots_of_a_fake_source() {
        let open: OpenSource = Box::new(|| {
            Ok(Box::new(FakeSine {
                started: Instant::now(),
                sent: 0,
                fail_after: None,
            }) as Box<dyn SampleSource>)
        });
        let snaps = collect(open, 30);
        let last = snaps.last().unwrap();
        let (lo, hi, _) = analyzer::band_edges(BandScale::Log, peak_band(last), FS as f32);
        assert!(lo - 25.0 <= 1000.0 && 1000.0 <= hi + 25.0, "peak {lo}..{hi}");
        assert!(last.level > -40.0 && last.level < -30.0, "level {}", last.level);
    }

    #[test]
    fn reopens_a_source_that_fails() {
        let opens = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&opens);
        let open: OpenSource = Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeSine {
                started: Instant::now(),
                sent: 0,
                fail_after: Some(4800),
            }) as Box<dyn SampleSource>)
        });
        // ~0.1 s of audio, error, 0.3 s wait, reopen: within 60 ticks (1 s).
        let snaps = collect(open, 60);
        assert_eq!(snaps.len(), 60, "silence keeps flowing while reopening");
        assert!(opens.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn a_source_that_cannot_open_yields_one_idle_snapshot() {
        let (tx, rx) = mpsc::channel();
        let input = AudioInput::start(
            60,
            BandScale::Log,
            Box::new(|| Err("no device".into())),
            Box::new(move |s| {
                let _ = tx.send(s.clone());
            }),
        )
        .unwrap();
        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first, AudioSnapshot::default());
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        drop(input);
    }

    #[test]
    fn a_quiet_source_reads_as_silence() {
        struct Quiet;
        impl SampleSource for Quiet {
            fn sample_rate(&self) -> u32 {
                FS
            }
            fn channels(&self) -> u16 {
                2
            }
            fn read(&mut self, _: &mut Vec<f32>) -> Result<(), String> {
                Ok(())
            }
        }
        let snaps = collect(Box::new(|| Ok(Box::new(Quiet) as Box<dyn SampleSource>)), 10);
        let last = snaps.last().unwrap();
        assert_eq!(last.level, analyzer::SILENT_DB);
        assert!(last.freq.iter().all(|&v| v == 0));
    }

    #[test]
    fn band_scale_parses() {
        assert_eq!(BandScale::parse("LOG"), Some(BandScale::Log));
        assert_eq!(BandScale::parse("linear"), Some(BandScale::Linear));
        assert_eq!(BandScale::parse("mel"), None);
    }
}
