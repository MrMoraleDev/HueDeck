// SPDX-License-Identifier: GPL-3.0-or-later
//! Audio loopback capture feeding `engine.audio.*`.

use crate::error::HalResult;

/// Audio loopback backend (WASAPI loopback, PipeWire/PulseAudio monitor).
pub trait AudioBackend: Send {
    /// Start capturing the default render device.
    fn start(&mut self) -> HalResult<()>;
    /// Sample rate in Hz of the stream (valid after `start`).
    fn sample_rate(&self) -> u32;
    /// Interleaved channel count.
    fn channels(&self) -> u16;
    /// Copy up to `buf.len()` interleaved f32 samples. Returns samples copied (`0` when none pending).
    fn read_samples(&mut self, buf: &mut [f32]) -> HalResult<usize>;
    /// Stop capturing.
    fn stop(&mut self);
}
