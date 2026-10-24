// SPDX-License-Identifier: GPL-3.0-or-later
//! `engine.vision.<name>`: pixel-statistics meters over a region of the
//! captured display (`<meta meter …>` of the active effect, docs/screen.md).
//!
//! * **area**: the fraction `0..=1` of the region's pixels whose HSL falls in
//!   the meter's `h`/`s`/`l` windows.
//! * **linear**: a bar. The region is cut into slices across its long axis;
//!   a slice is lit when at least half its pixels match. The value is the
//!   fraction `0..=1` of lit slices (a 70 % health bar reads 0.7, a notch in
//!   the bar does not cut it to 0).
//! * **colormean**: `[h, s, l]` of the region's mean colour.
//! * OCR meters are out of scope and read 0.
//!
//! Regions are sampled on the full-resolution frame, at most 256 points along
//! and 32 across each region.

use super::sample::{rgb_to_hsl, Frame};

/// What a meter computes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeterKind {
    /// Fill fraction of a bar.
    Linear,
    /// Fraction of matching pixels.
    Area,
    /// Mean colour `[h, s, l]`.
    ColorMean,
    /// Not evaluated (OCR): always 0.
    Unsupported,
}

/// An inclusive window; `lo > hi` wraps (hue `340-20` is red).
pub type Window = (f32, f32);

/// Colour windows; a missing window accepts everything.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HslWindow {
    /// Hue `0..=360`.
    pub h: Option<Window>,
    /// Saturation `0..=100`.
    pub s: Option<Window>,
    /// Lightness `0..=100`.
    pub l: Option<Window>,
}

fn inside(v: f32, w: Option<Window>) -> bool {
    match w {
        None => true,
        Some((lo, hi)) if lo <= hi => v >= lo && v <= hi,
        Some((lo, hi)) => v >= lo || v <= hi,
    }
}

impl HslWindow {
    /// Whether `[r, g, b]` falls in every window. Values are compared after
    /// rounding to whole degrees and percent, as the windows are written.
    pub fn matches(&self, [r, g, b]: [u8; 3]) -> bool {
        let (h, s, l) = rgb_to_hsl(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
        inside(h.round() % 360.0, self.h) && inside(s.round(), self.s) && inside(l.round(), self.l)
    }
}

/// A rectangle in display fractions (`0..=1`); no height: a one-pixel band.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: Option<f64>,
}

/// A region used instead of the base one on matching displays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectOverride {
    /// Exact display size `(w, h)` this applies to.
    pub size: Option<(u32, u32)>,
    /// Display aspect `(w, h)` this applies to (when no size matches).
    pub aspect: Option<(u32, u32)>,
    /// The region.
    pub rect: Rect,
}

/// One meter of the active effect.
#[derive(Clone, Debug, PartialEq)]
pub struct Meter {
    /// `engine.vision` key.
    pub name: String,
    /// What it computes.
    pub kind: MeterKind,
    /// Region on any display without an override.
    pub rect: Rect,
    /// Matching colours.
    pub color: HslWindow,
    /// Per-display regions (`<resolution>` children).
    pub overrides: Vec<RectOverride>,
}

/// A value for `engine.vision.<name>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VisionValue {
    /// `0..=1` (linear, area) or 0 (unsupported).
    Number(f64),
    /// `[h, s, l]` (colormean).
    Triple([f64; 3]),
}

/// Most samples along a region's long axis / across it.
const ALONG: u32 = 256;
const ACROSS: u32 = 32;

/// Up to `n` evenly spaced pixel indices in `[lo, hi)`.
fn points(lo: u32, hi: u32, n: u32) -> impl Iterator<Item = u32> + Clone {
    let span = hi - lo;
    let n = span.min(n).max(1);
    (0..n).map(move |i| lo + (2 * i + 1) * span / (2 * n))
}

impl Meter {
    /// The region for a `w` x `h` display: an exact size override, else an
    /// aspect override, else the base region.
    pub fn rect_for(&self, w: u32, h: u32) -> Rect {
        let by_size = self.overrides.iter().find(|o| o.size == Some((w, h)));
        let by_aspect = || {
            self.overrides.iter().find(|o| {
                o.aspect.is_some_and(|(aw, ah)| {
                    ah > 0 && (f64::from(w) / f64::from(h) - f64::from(aw) / f64::from(ah)).abs() < 0.01
                })
            })
        };
        by_size.or_else(by_aspect).map_or(self.rect, |o| o.rect)
    }

    /// Pixel bounds `[x0, x1) x [y0, y1)` of the region on a `w` x `h` display
    /// (at least one pixel each way).
    fn bounds(&self, w: u32, h: u32) -> (u32, u32, u32, u32) {
        let r = self.rect_for(w, h);
        // A hair of tolerance so 0.55 * 200 (110.00000000000001) is 110.
        let lo = |v: f64, n: u32| (v.clamp(0.0, 1.0) * f64::from(n) + 1e-6).floor() as u32;
        let hi = |v: f64, n: u32| (v.clamp(0.0, 1.0) * f64::from(n) - 1e-6).ceil() as u32;
        let x0 = lo(r.x, w).min(w - 1);
        let y0 = lo(r.y, h).min(h - 1);
        let x1 = hi(r.x + r.width, w);
        let y1 = r.height.map_or(0, |rh| hi(r.y + rh, h));
        (x0, x1.clamp(x0 + 1, w), y0, y1.clamp(y0 + 1, h))
    }

    /// The meter's value on `frame`.
    pub fn evaluate(&self, frame: &Frame<'_>) -> VisionValue {
        let (w, h) = frame.display_size();
        let (x0, x1, y0, y1) = self.bounds(w, h);
        let horizontal = x1 - x0 >= y1 - y0;
        let (along, across) = if horizontal {
            (points(x0, x1, ALONG), points(y0, y1, ACROSS))
        } else {
            (points(y0, y1, ALONG), points(x0, x1, ACROSS))
        };
        let at = |a: u32, c: u32| {
            if horizontal {
                frame.rgb(a, c)
            } else {
                frame.rgb(c, a)
            }
        };
        match self.kind {
            MeterKind::Unsupported => VisionValue::Number(0.0),
            MeterKind::Area => {
                let (mut hit, mut n) = (0u32, 0u32);
                for a in along {
                    for c in across.clone() {
                        hit += u32::from(self.color.matches(at(a, c)));
                        n += 1;
                    }
                }
                VisionValue::Number(f64::from(hit) / f64::from(n))
            }
            MeterKind::Linear => {
                let (mut lit, mut slices) = (0u32, 0u32);
                for a in along {
                    let (mut hit, mut n) = (0u32, 0u32);
                    for c in across.clone() {
                        hit += u32::from(self.color.matches(at(a, c)));
                        n += 1;
                    }
                    lit += u32::from(2 * hit >= n);
                    slices += 1;
                }
                VisionValue::Number(f64::from(lit) / f64::from(slices))
            }
            MeterKind::ColorMean => {
                let (mut sum, mut n) = ([0u64; 3], 0u64);
                for a in along {
                    for c in across.clone() {
                        for (s, v) in sum.iter_mut().zip(at(a, c)) {
                            *s += u64::from(v);
                        }
                        n += 1;
                    }
                }
                let [r, g, b] = sum.map(|s| s as f32 / n as f32 / 255.0);
                let (h, s, l) = rgb_to_hsl(r, g, b);
                VisionValue::Triple([f64::from(h.round()), f64::from(s.round()), f64::from(l.round())])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::sample::tests::surface;
    use super::super::sample::Rotation;
    use super::*;

    const W: u32 = 400;
    const H: u32 = 200;

    fn meter(kind: MeterKind, rect: Rect, color: HslWindow) -> Meter {
        Meter {
            name: "m".into(),
            kind,
            rect,
            color,
            overrides: Vec::new(),
        }
    }

    fn red() -> HslWindow {
        HslWindow {
            h: Some((340.0, 20.0)),
            s: Some((60.0, 100.0)),
            l: Some((30.0, 70.0)),
        }
    }

    /// Black screen with a red bar at y 100..110 filled over x 40..(40 + fill).
    fn bar(fill: u32) -> Vec<u8> {
        surface(W, H, |x, y| {
            if (100..110).contains(&y) && (40..40 + fill).contains(&x) {
                [230, 20, 30]
            } else {
                [0, 0, 0]
            }
        })
    }

    fn eval(m: &Meter, data: &[u8]) -> VisionValue {
        m.evaluate(&Frame::new(data, W, H, (W * 4) as usize, Rotation::Identity).unwrap())
    }

    const BAR: Rect = Rect {
        x: 0.1,
        y: 0.5,
        width: 0.5,
        height: Some(0.05),
    };

    #[test]
    fn a_region_of_the_matching_colour_reads_one() {
        let data = bar(200);
        assert_eq!(eval(&meter(MeterKind::Area, BAR, red()), &data), VisionValue::Number(1.0));
        assert_eq!(eval(&meter(MeterKind::Linear, BAR, red()), &data), VisionValue::Number(1.0));
        let off = HslWindow {
            h: Some((100.0, 140.0)),
            ..red()
        };
        assert_eq!(eval(&meter(MeterKind::Area, BAR, off), &data), VisionValue::Number(0.0));
    }

    #[test]
    fn linear_reads_the_fill_fraction() {
        let VisionValue::Number(v) = eval(&meter(MeterKind::Linear, BAR, red()), &bar(140)) else {
            panic!()
        };
        assert!((v - 0.7).abs() < 0.01, "{v}");
        // A two-pixel notch does not empty the bar.
        let mut notched = bar(200);
        for y in 100..110 {
            for x in 90..92 {
                let i = ((y * W + x) * 4) as usize;
                notched[i..i + 3].copy_from_slice(&[0, 0, 0]);
            }
        }
        let VisionValue::Number(v) = eval(&meter(MeterKind::Linear, BAR, red()), &notched) else {
            panic!()
        };
        assert!(v > 0.98, "{v}");
        // A vertical bar fills along y.
        let vbar = Rect {
            x: 0.1,
            y: 0.5,
            width: 0.025,
            height: Some(0.5),
        };
        let data = surface(W, H, |x, y| {
            if (40..50).contains(&x) && y >= 150 {
                [230, 20, 30]
            } else {
                [0; 3]
            }
        });
        let VisionValue::Number(v) = eval(&meter(MeterKind::Linear, vbar, red()), &data) else {
            panic!()
        };
        assert!((v - 0.5).abs() < 0.02, "{v}");
    }

    #[test]
    fn area_counts_matching_pixels_and_hue_wraps() {
        // Left half of the region pure red (hue 0), right half hue 350.
        let data = surface(W, H, |x, _| if x < 200 { [255, 0, 0] } else { [255, 0, 43] });
        let all = Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: Some(1.0),
        };
        assert_eq!(eval(&meter(MeterKind::Area, all, red()), &data), VisionValue::Number(1.0));
        let only_350 = HslWindow {
            h: Some((345.0, 355.0)),
            ..HslWindow::default()
        };
        assert_eq!(eval(&meter(MeterKind::Area, all, only_350), &data), VisionValue::Number(0.5));
    }

    #[test]
    fn colormean_and_unsupported() {
        let data = surface(W, H, |_, _| [64, 128, 191]);
        assert_eq!(
            eval(&meter(MeterKind::ColorMean, BAR, HslWindow::default()), &data),
            VisionValue::Triple([210.0, 50.0, 50.0])
        );
        assert_eq!(
            eval(&meter(MeterKind::Unsupported, BAR, red()), &data),
            VisionValue::Number(0.0)
        );
    }

    #[test]
    fn a_band_without_height_is_one_pixel_row() {
        let line = Rect {
            height: None,
            y: 0.525,
            ..BAR
        };
        let VisionValue::Number(v) = eval(&meter(MeterKind::Linear, line, red()), &bar(100)) else {
            panic!()
        };
        assert!((v - 0.5).abs() < 0.01, "{v}");
    }

    #[test]
    fn overrides_pick_by_size_then_aspect() {
        let alt = |x| Rect { x, ..BAR };
        let mut m = meter(MeterKind::Area, BAR, red());
        m.overrides = vec![
            RectOverride {
                size: None,
                aspect: Some((16, 9)),
                rect: alt(0.3),
            },
            RectOverride {
                size: Some((1920, 1080)),
                aspect: None,
                rect: alt(0.2),
            },
        ];
        assert_eq!(m.rect_for(1920, 1080).x, 0.2);
        assert_eq!(m.rect_for(2560, 1440).x, 0.3);
        assert_eq!(m.rect_for(1920, 1200).x, 0.1);
    }
}
