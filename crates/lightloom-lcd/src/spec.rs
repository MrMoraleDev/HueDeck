// SPDX-License-Identifier: GPL-3.0-or-later
//! `LCD.initialize({width, height, circular?, circularPreview?})`.
//!
//! SignalRGB's validation messages (effects-engine.md §4): "Settings must be an object",
//! "Width must be provided and must be a number", "Height must be provided and must be a
//! number", "Circular must be a boolean", "Invalid dimensions". The type checks happen on
//! the JS side (`js/host.js`); [`LcdSpec::new`] checks the numbers.

use crate::LcdError;
use serde::Serialize;

/// Largest width or height accepted. The corpus goes up to 1920 (480x1920 Lian Li
/// Universal Screen, 1920x462 bars).
pub const MAX_SIDE: u32 = 2048;
/// Largest pixel count accepted: the face renderer keeps three frames of this in shared
/// memory (3 x 2 Mpx x 4 bytes = 24 MiB).
pub const MAX_PIXELS: u64 = 2048 * 1024;

/// What a plugin asked `LCD.initialize` for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LcdSpec {
    /// Face width in pixels.
    pub width: u32,
    /// Face height in pixels.
    pub height: u32,
    /// The screen is round (`circular`, or `circularPreview` for the dashboard only).
    /// Frames stay rectangular; faces draw inside the inscribed circle.
    pub circular: bool,
}

impl LcdSpec {
    /// Validate `initialize` numbers: whole, `1..=`[`MAX_SIDE`] each, at most
    /// [`MAX_PIXELS`] together.
    pub fn new(width: f64, height: f64, circular: bool) -> Result<Self, LcdError> {
        let side = |v: f64| -> Option<u32> {
            (v.is_finite() && v.fract() == 0.0 && v >= 1.0 && v <= f64::from(MAX_SIDE)).then_some(v as u32)
        };
        match (side(width), side(height)) {
            (Some(w), Some(h)) if u64::from(w) * u64::from(h) <= MAX_PIXELS => Ok(Self {
                width: w,
                height: h,
                circular,
            }),
            _ => Err(LcdError::InvalidSettings(format!(
                "Invalid dimensions: {width} x {height} (each 1-{MAX_SIDE}, at most {MAX_PIXELS} pixels)"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_sizes_are_accepted() {
        for (w, h) in [
            (480, 480),
            (240, 240),
            (640, 640),
            (320, 480),
            (128, 40),
            (1920, 462),
            (480, 1920),
            (17, 19),
        ] {
            let s = LcdSpec::new(f64::from(w), f64::from(h), false).unwrap();
            assert_eq!((s.width, s.height), (w, h));
        }
        assert!(LcdSpec::new(480.0, 480.0, true).unwrap().circular);
    }

    #[test]
    fn bad_dimensions_are_refused() {
        for (w, h) in [
            (0.0, 10.0),
            (10.0, -1.0),
            (10.5, 10.0),
            (f64::NAN, 10.0),
            (f64::INFINITY, 10.0),
            (4096.0, 10.0),
            (2048.0, 2048.0),
        ] {
            let e = LcdSpec::new(w, h, false).unwrap_err();
            assert_eq!(e.code(), "INVALID_SETTINGS", "{w}x{h}");
            assert!(e.to_string().starts_with("Invalid dimensions"));
        }
    }
}
