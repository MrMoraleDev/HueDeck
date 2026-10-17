// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Colour type and brightness math.

use serde::{Deserialize, Serialize};

/// 8-bit RGB colour, what `device.color(x, y)` hands to plugins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// Black.
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
    /// White.
    pub const WHITE: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    /// The SignalRGB default pulse/forced colour `#009EDB`.
    pub const SIGNAL_BLUE: Rgb = Rgb {
        r: 0x00,
        g: 0x9E,
        b: 0xDB,
    };

    /// Build a colour.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse `#RRGGBB` (case-insensitive, `#` optional).
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().strip_prefix('#').unwrap_or(s.trim());
        if s.len() != 6 || !s.is_ascii() {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }

    /// `#rrggbb`, lowercase.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Multiply every channel by `factor` (clamped to `0..=1`), rounding to nearest.
    pub fn scaled(self, factor: f32) -> Self {
        let f = factor.clamp(0.0, 1.0);
        let s = |c: u8| (c as f32 * f).round().clamp(0.0, 255.0) as u8;
        Self {
            r: s(self.r),
            g: s(self.g),
            b: s(self.b),
        }
    }

    /// HSV → RGB with `h` in degrees (any value, wrapped), `s`/`v` in `0..=1`.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(360.0) / 60.0;
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let to8 = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Self {
            r: to8(r),
            g: to8(g),
            b: to8(b),
        }
    }
}

/// Effective brightness factor `0..=1` from the three sliders
/// (global, layout, device), each `0..=100`. Values above 100 are clamped.
pub fn effective_brightness(global: u8, layout: u8, device: u8) -> f32 {
    let pct = |v: u8| (v.min(100) as f32) / 100.0;
    pct(global) * pct(layout) * pct(device)
}

/// Apply a brightness factor to a colour.
pub fn apply_brightness(color: Rgb, factor: f32) -> Rgb {
    color.scaled(factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Rgb::from_hex("#009EDB"), Some(Rgb::SIGNAL_BLUE));
        assert_eq!(Rgb::from_hex("009edb"), Some(Rgb::SIGNAL_BLUE));
        assert_eq!(Rgb::SIGNAL_BLUE.to_hex(), "#009edb");
        assert_eq!(Rgb::from_hex("#12345"), None);
        assert_eq!(Rgb::from_hex("#gg0000"), None);
    }

    #[test]
    fn brightness_is_a_product_of_three_sliders() {
        assert_eq!(effective_brightness(100, 100, 100), 1.0);
        assert!((effective_brightness(50, 50, 100) - 0.25).abs() < 1e-6);
        assert!((effective_brightness(50, 50, 50) - 0.125).abs() < 1e-6);
        assert_eq!(effective_brightness(0, 100, 100), 0.0);
        assert_eq!(effective_brightness(200, 100, 100), 1.0);
        assert_eq!(
            apply_brightness(Rgb::new(200, 100, 50), 0.5),
            Rgb::new(100, 50, 25)
        );
        assert_eq!(apply_brightness(Rgb::WHITE, 2.0), Rgb::WHITE);
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(Rgb::from_hsv(0.0, 1.0, 1.0), Rgb::new(255, 0, 0));
        assert_eq!(Rgb::from_hsv(120.0, 1.0, 1.0), Rgb::new(0, 255, 0));
        assert_eq!(Rgb::from_hsv(240.0, 1.0, 1.0), Rgb::new(0, 0, 255));
        assert_eq!(Rgb::from_hsv(360.0, 1.0, 1.0), Rgb::new(255, 0, 0));
        assert_eq!(Rgb::from_hsv(-120.0, 1.0, 1.0), Rgb::new(0, 0, 255));
        assert_eq!(Rgb::from_hsv(0.0, 0.0, 1.0), Rgb::WHITE);
    }
}
