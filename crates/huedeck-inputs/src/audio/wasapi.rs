// SPDX-License-Identifier: GPL-3.0-or-later
//! WASAPI loopback through cpal: a stream built as *input* on a *render*
//! (output) device makes cpal set `AUDCLNT_STREAMFLAGS_LOOPBACK` (cpal 0.18.2
//! `host/wasapi/device.rs`), so we capture what the PC plays, in the device's
//! shared-mode mix format. Only output devices are ever enumerated or opened:
//! no microphone is captured.

use super::SampleSource;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Data, ErrorKind, InputCallbackInfo, SampleFormat};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Names of the output (render) devices.
pub fn output_devices() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|it| it.map(|d| d.to_string()).collect())
        .unwrap_or_default()
}

/// A loopback stream feeding a lock-free ring.
pub struct LoopbackSource {
    _stream: cpal::Stream,
    rx: rtrb::Consumer<f32>,
    fs: u32,
    channels: u16,
    /// Set by the error callback when the stream must be rebuilt.
    dead: Arc<AtomicBool>,
    xruns: Arc<AtomicU32>,
    xruns_seen: u32,
}

impl LoopbackSource {
    /// Open loopback on `device` (an output device name; `None`/empty: the default output).
    pub fn open(device: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();
        let dev = match device.filter(|d| !d.is_empty()) {
            Some(name) => host
                .output_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.to_string() == name)
                .ok_or_else(|| format!("no output device named {name:?}"))?,
            None => host
                .default_output_device()
                .ok_or_else(|| "no default output device".to_string())?,
        };
        let name = dev.to_string();
        // On WASAPI this is GetMixFormat: capture streams only take the native format.
        let supported = dev.default_output_config().map_err(|e| e.to_string())?;
        let (config, format) = (supported.config(), supported.sample_format());
        let (fs, channels) = (config.sample_rate, config.channels);
        // Half a second of headroom; the reader drains it every tick.
        let (mut tx, rx) = rtrb::RingBuffer::<f32>::new((fs as usize * usize::from(channels) / 2).max(4096));
        let dead = Arc::new(AtomicBool::new(false));
        let xruns = Arc::new(AtomicU32::new(0));
        let (dead_cb, xruns_cb) = (Arc::clone(&dead), Arc::clone(&xruns));
        // Real-time thread: no allocation, no locks. A buffer that does not fit
        // whole is dropped, so frames never split and channels stay aligned.
        let data = move |d: &Data, _: &InputCallbackInfo| match d.sample_format() {
            _ if tx.slots() < d.len() => {}
            SampleFormat::F32 => {
                if let Some(s) = d.as_slice::<f32>() {
                    let _ = tx.push_partial_slice(s);
                }
            }
            SampleFormat::I16 => {
                for &s in d.as_slice::<i16>().unwrap_or_default() {
                    let _ = tx.push(f32::from(s) / 32_768.0);
                }
            }
            SampleFormat::I32 => {
                for &s in d.as_slice::<i32>().unwrap_or_default() {
                    let _ = tx.push(s as f32 / 2_147_483_648.0);
                }
            }
            _ => {}
        };
        let error = move |e: cpal::Error| match e.kind() {
            ErrorKind::Xrun => {
                xruns_cb.fetch_add(1, Ordering::Relaxed);
            }
            ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
            _ => dead_cb.store(true, Ordering::Release),
        };
        let stream = dev
            .build_input_stream_raw(config, format, data, error, Some(Duration::from_secs(2)))
            .map_err(|e| format!("{name}: {e}"))?;
        // Streams start paused (cpal 0.18).
        stream.play().map_err(|e| format!("{name}: {e}"))?;
        tracing::info!("audio: loopback of {name:?} ({format:?})");
        Ok(Self {
            _stream: stream,
            rx,
            fs,
            channels,
            dead,
            xruns,
            xruns_seen: 0,
        })
    }
}

impl SampleSource for LoopbackSource {
    fn sample_rate(&self) -> u32 {
        self.fs
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn read(&mut self, out: &mut Vec<f32>) -> Result<(), String> {
        if self.dead.load(Ordering::Acquire) {
            return Err("the loopback stream was invalidated".into());
        }
        let n = self.rx.slots() / usize::from(self.channels) * usize::from(self.channels);
        if let Ok(chunk) = self.rx.read_chunk(n) {
            let (a, b) = chunk.as_slices();
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            chunk.commit_all();
        }
        let xruns = self.xruns.load(Ordering::Relaxed);
        if xruns != self.xruns_seen {
            tracing::debug!("audio: {} capture discontinuities", xruns - self.xruns_seen);
            self.xruns_seen = xruns;
        }
        Ok(())
    }
}
