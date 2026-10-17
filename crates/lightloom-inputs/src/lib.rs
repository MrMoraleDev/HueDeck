// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Input sources for lightscripts: audio spectrum (`engine.audio`), screen
//! zones (`engine.zone`), vision meters (`engine.vision`) and sensors.
//!
//! Stub for M0: only the snapshot shapes pushed to the renderer are defined.
//! The cpal + rustfft audio pipeline, xcap screen capture and the sensor
//! facade arrive with M6.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// `engine.audio.freq` bin count.
pub const AUDIO_FREQ_BINS: usize = 200;
/// `engine.zone` columns.
pub const ZONE_COLS: usize = 28;
/// `engine.zone` rows.
pub const ZONE_ROWS: usize = 20;
/// `engine.zone` cell count.
pub const ZONE_CELLS: usize = ZONE_COLS * ZONE_ROWS;
/// `engine.zone.imagedata` width.
pub const ZONE_IMAGE_WIDTH: usize = 160;
/// `engine.zone.imagedata` height.
pub const ZONE_IMAGE_HEIGHT: usize = 100;
/// `engine.zone.imagedata` byte length (RGBA).
pub const ZONE_IMAGE_BYTES: usize = ZONE_IMAGE_WIDTH * ZONE_IMAGE_HEIGHT * 4;

/// One `engine.audio` update.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioSnapshot {
    /// 200 spectrum bins, `0..=1`.
    pub freq: Vec<f32>,
    /// Level in dB.
    pub level: f32,
    /// Raw linear level.
    pub rawlevel: f32,
    /// Stereo width `0..=1`.
    pub width: f32,
    /// Spectral density `0..=1`.
    pub density: f32,
}

impl Default for AudioSnapshot {
    fn default() -> Self {
        Self { freq: vec![0.0; AUDIO_FREQ_BINS], level: -100.0, rawlevel: 0.0, width: 0.0, density: 0.0 }
    }
}

/// One `engine.zone` update (28x20 HSL grid + 160x100 RGBA thumbnail).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneSnapshot {
    /// Hue per cell, `0..=360`.
    pub hue: Vec<i16>,
    /// Saturation per cell, `0..=100`.
    pub saturation: Vec<i8>,
    /// Lightness per cell, `0..=100`.
    pub lightness: Vec<i8>,
    /// RGBA thumbnail.
    pub imagedata: Vec<u8>,
}

impl Default for ZoneSnapshot {
    fn default() -> Self {
        Self {
            hue: vec![0; ZONE_CELLS],
            saturation: vec![0; ZONE_CELLS],
            lightness: vec![0; ZONE_CELLS],
            imagedata: vec![0; ZONE_IMAGE_BYTES],
        }
    }
}

/// A source that produces snapshots at its own rate.
pub trait InputSource: Send {
    /// Name for logs/UI.
    fn name(&self) -> &str;
    /// Start producing.
    fn start(&mut self) -> Result<(), String>;
    /// Stop producing.
    fn stop(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shapes_match_the_engine_contract() {
        assert_eq!(AudioSnapshot::default().freq.len(), 200);
        let z = ZoneSnapshot::default();
        assert_eq!(z.hue.len(), 560);
        assert_eq!(z.imagedata.len(), 64_000);
    }
}
