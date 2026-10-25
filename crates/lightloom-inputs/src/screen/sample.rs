// SPDX-License-Identifier: GPL-3.0-or-later
//! CPU side of screen capture: a BGRA desktop image is reduced to the
//! 160x100 RGBA `engine.zone.imagedata` thumbnail, and the thumbnail to the
//! 28x20 HSL grid (`engine.zone.hue/saturation/lightness`).
//!
//! The thumbnail is a stratified box filter: each output pixel averages up to
//! 4x4 evenly spaced source pixels of the area it covers (16 reads per output
//! pixel, 256,000 per frame whatever the display size). Each zone cell is the
//! area-weighted mean of the thumbnail pixels it covers (5.714 x 5 px).

use crate::{ZoneSnapshot, ZONE_COLS, ZONE_IMAGE_HEIGHT, ZONE_IMAGE_WIDTH, ZONE_ROWS};

/// How the desktop image is rotated before it reaches the display
/// (`DXGI_MODE_ROTATION`): the captured surface is always un-rotated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    /// Landscape as captured.
    #[default]
    Identity,
    /// Shown rotated 90 degrees clockwise.
    Rotate90,
    /// Shown upside down.
    Rotate180,
    /// Shown rotated 270 degrees clockwise.
    Rotate270,
}

/// One captured desktop image, BGRA8, borrowed from the capture backend.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    bgra: &'a [u8],
    width: u32,
    height: u32,
    stride: usize,
    rotation: Rotation,
}

impl<'a> Frame<'a> {
    /// A `width` x `height` surface whose rows start every `stride` bytes;
    /// `None` when `bgra` is too short or a dimension is 0.
    pub fn new(bgra: &'a [u8], width: u32, height: u32, stride: usize, rotation: Rotation) -> Option<Self> {
        let row = width as usize * 4;
        if width == 0 || height == 0 || stride < row || bgra.len() < stride * (height as usize - 1) + row {
            return None;
        }
        Some(Self {
            bgra,
            width,
            height,
            stride,
            rotation,
        })
    }

    /// Width and height as the display shows them (swapped when rotated a quarter turn).
    pub fn display_size(&self) -> (u32, u32) {
        match self.rotation {
            Rotation::Rotate90 | Rotation::Rotate270 => (self.height, self.width),
            Rotation::Identity | Rotation::Rotate180 => (self.width, self.height),
        }
    }

    /// Byte offset contributed by display column `dx` (rotation folded in).
    fn col_offset(&self, dx: u32) -> usize {
        let (w, h) = (self.width as usize, self.height as usize);
        let dx = dx as usize;
        match self.rotation {
            Rotation::Identity => dx * 4,
            Rotation::Rotate90 => (h - 1 - dx) * self.stride,
            Rotation::Rotate180 => (w - 1 - dx) * 4,
            Rotation::Rotate270 => dx * self.stride,
        }
    }

    /// Byte offset contributed by display row `dy` (rotation folded in).
    fn row_offset(&self, dy: u32) -> usize {
        let (w, h) = (self.width as usize, self.height as usize);
        let dy = dy as usize;
        match self.rotation {
            Rotation::Identity => dy * self.stride,
            Rotation::Rotate90 => dy * 4,
            Rotation::Rotate180 => (h - 1 - dy) * self.stride,
            Rotation::Rotate270 => (w - 1 - dy) * 4,
        }
    }

    /// `[r, g, b]` at display coordinates (`dx < width`, `dy < height` of
    /// [`Frame::display_size`]).
    pub fn rgb(&self, dx: u32, dy: u32) -> [u8; 3] {
        let i = self.col_offset(dx) + self.row_offset(dy);
        [self.bgra[i + 2], self.bgra[i + 1], self.bgra[i]]
    }
}

/// Most samples per axis per thumbnail pixel.
const SAMPLES: u32 = 4;

/// Up to [`SAMPLES`] evenly spaced coordinates inside `[lo, hi)` (at least one).
fn strata(lo: u32, hi: u32, limit: u32) -> impl Iterator<Item = u32> {
    let span = hi.saturating_sub(lo).max(1);
    let n = span.min(SAMPLES);
    (0..n).map(move |i| (lo + (2 * i + 1) * span / (2 * n)).min(limit - 1))
}

/// Downscale `frame` (stretched, aspect not kept) into the 160x100 RGBA
/// thumbnail `out` (`ZONE_IMAGE_BYTES` long).
pub fn thumbnail(frame: &Frame<'_>, out: &mut [u8]) {
    let (w, h) = frame.display_size();
    let (tw, th) = (ZONE_IMAGE_WIDTH as u32, ZONE_IMAGE_HEIGHT as u32);
    let cols: Vec<Vec<usize>> = (0..tw)
        .map(|x| {
            strata(x * w / tw, (x + 1) * w / tw, w)
                .map(|dx| frame.col_offset(dx))
                .collect()
        })
        .collect();
    let mut px = out.as_chunks_mut::<4>().0.iter_mut();
    for y in 0..th {
        let rows: Vec<usize> = strata(y * h / th, (y + 1) * h / th, h)
            .map(|dy| frame.row_offset(dy))
            .collect();
        for col in &cols {
            let (mut b, mut g, mut r) = (0u32, 0u32, 0u32);
            for &ro in &rows {
                for &co in col {
                    let i = ro + co;
                    b += u32::from(frame.bgra[i]);
                    g += u32::from(frame.bgra[i + 1]);
                    r += u32::from(frame.bgra[i + 2]);
                }
            }
            let n = (rows.len() * col.len()) as u32;
            let o = px.next().expect("a 160x100 RGBA buffer");
            o[0] = ((r + n / 2) / n) as u8;
            o[1] = ((g + n / 2) / n) as u8;
            o[2] = ((b + n / 2) / n) as u8;
            o[3] = 255;
        }
    }
}

/// CSS HSL of an sRGB colour (`0..=1` channels): hue `0..360`, saturation and
/// lightness `0..=100`.
pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= f32::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, (s * 100.0).min(100.0), l * 100.0)
}

/// Weights of the thumbnail pixels a zone column (or row) covers: `(pixel, overlap)`.
fn spans(cells: usize, pixels: usize) -> Vec<Vec<(usize, f32)>> {
    let step = pixels as f32 / cells as f32;
    (0..cells)
        .map(|c| {
            let (lo, hi) = (c as f32 * step, (c + 1) as f32 * step);
            (lo.floor() as usize..(hi.ceil() as usize).min(pixels))
                .filter_map(|p| {
                    let w = hi.min(p as f32 + 1.0) - lo.max(p as f32);
                    (w > 1e-4).then_some((p, w))
                })
                .collect()
        })
        .collect()
}

/// Fill `zone.hue/saturation/lightness` from the thumbnail in `zone.imagedata`.
pub fn zones(zone: &mut ZoneSnapshot) {
    let cols = spans(ZONE_COLS, ZONE_IMAGE_WIDTH);
    let rows = spans(ZONE_ROWS, ZONE_IMAGE_HEIGHT);
    for (zy, row) in rows.iter().enumerate() {
        for (zx, col) in cols.iter().enumerate() {
            let (mut sum, mut wsum) = ([0f32; 3], 0f32);
            for &(y, wy) in row {
                for &(x, wx) in col {
                    let i = (y * ZONE_IMAGE_WIDTH + x) * 4;
                    let w = wx * wy;
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += f32::from(zone.imagedata[i + c]) * w;
                    }
                    wsum += w;
                }
            }
            let [r, g, b] = sum.map(|s| s / wsum / 255.0);
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let cell = zy * ZONE_COLS + zx;
            zone.hue[cell] = (h.round() as i16).rem_euclid(360);
            zone.saturation[cell] = s.round() as i8;
            zone.lightness[cell] = l.round() as i8;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ZONE_IMAGE_BYTES;

    /// A tightly packed BGRA surface painted by `paint(x, y) -> [r, g, b]`.
    pub(crate) fn surface(w: u32, h: u32, paint: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let [r, g, b] = paint(x, y);
                v.extend_from_slice(&[b, g, r, 255]);
            }
        }
        v
    }

    fn zone_of(frame: &Frame<'_>) -> ZoneSnapshot {
        let mut z = ZoneSnapshot::default();
        thumbnail(frame, &mut z.imagedata);
        zones(&mut z);
        z
    }

    #[test]
    fn hsl_matches_css() {
        let hsl = |r: u8, g: u8, b: u8| {
            let (h, s, l) = rgb_to_hsl(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
            (h.round() as i32, s.round() as i32, l.round() as i32)
        };
        assert_eq!(hsl(255, 0, 0), (0, 100, 50));
        assert_eq!(hsl(0, 255, 0), (120, 100, 50));
        assert_eq!(hsl(0, 0, 255), (240, 100, 50));
        assert_eq!(hsl(255, 0, 255), (300, 100, 50));
        assert_eq!(hsl(255, 255, 255), (0, 0, 100));
        assert_eq!(hsl(128, 128, 128), (0, 0, 50));
        // rgb(64, 128, 191) = hsl(210, 50%, 50%)
        assert_eq!(hsl(64, 128, 191), (210, 50, 50));
    }

    #[test]
    fn rejects_short_buffers() {
        assert!(Frame::new(&[0; 15], 2, 2, 8, Rotation::Identity).is_none());
        assert!(Frame::new(&[0; 16], 2, 2, 8, Rotation::Identity).is_some());
        // Padded rows: the last row needs only width * 4 bytes.
        assert!(Frame::new(&[0; 24], 2, 2, 16, Rotation::Identity).is_some());
        assert!(Frame::new(&[], 0, 0, 0, Rotation::Identity).is_none());
    }

    #[test]
    fn vertical_grey_gradient_gives_rising_lightness() {
        let (w, h) = (320, 200);
        let data = surface(w, h, |_, y| [(y * 255 / (h - 1)) as u8; 3]);
        let z = zone_of(&Frame::new(&data, w, h, (w * 4) as usize, Rotation::Identity).unwrap());
        for r in 0..ZONE_ROWS {
            // Zone row r covers source rows 10r..10r+9.
            let mean: f32 = (10 * r..10 * r + 10)
                .map(|y| (y as u32 * 255 / (h - 1)) as f32)
                .sum::<f32>()
                / 10.0;
            let want = (mean / 255.0 * 100.0).round() as i8;
            for c in 0..ZONE_COLS {
                let i = r * ZONE_COLS + c;
                assert!(
                    (z.lightness[i] - want).abs() <= 1,
                    "row {r}: {} vs {want}",
                    z.lightness[i]
                );
                assert_eq!((z.hue[i], z.saturation[i]), (0, 0));
            }
        }
        assert_eq!(z.imagedata.len(), ZONE_IMAGE_BYTES);
        // Thumbnail row 0 averages source rows 0 and 1 (values 0 and 1), rounded half up.
        assert_eq!(&z.imagedata[..4], &[1, 1, 1, 255]);
    }

    #[test]
    fn red_and_blue_halves_give_their_hues() {
        let (w, h) = (2560, 1440);
        let data = surface(w, h, |x, _| if x < w / 2 { [255, 0, 0] } else { [0, 0, 255] });
        let z = zone_of(&Frame::new(&data, w, h, (w * 4) as usize, Rotation::Identity).unwrap());
        for r in 0..ZONE_ROWS {
            for c in (0..13).chain(15..ZONE_COLS) {
                let i = r * ZONE_COLS + c;
                let want = if c < 14 { 0 } else { 240 };
                assert_eq!(
                    (z.hue[i], z.saturation[i], z.lightness[i]),
                    (want, 100, 50),
                    "cell {r},{c}"
                );
            }
        }
    }

    #[test]
    fn hue_gradient_columns_follow_the_hue() {
        // Each zone column of a 280-px-wide surface spans 10 px: paint each
        // column block with a fully saturated hue of 12 degrees per column.
        let (w, h) = (280, 40);
        let hue_rgb = |deg: f32| {
            let f = |n: f32| {
                let k = (n + deg / 30.0) % 12.0;
                let v = 0.5 - 0.5 * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
                (v * 255.0).round() as u8
            };
            [f(0.0), f(8.0), f(4.0)]
        };
        let data = surface(w, h, |x, _| hue_rgb((x / 10) as f32 * 12.0));
        let z = zone_of(&Frame::new(&data, w, h, (w * 4) as usize, Rotation::Identity).unwrap());
        for c in 0..ZONE_COLS {
            let got = z.hue[5 * ZONE_COLS + c];
            let want = c as i16 * 12;
            // Edge thumbnail pixels blend into the neighbour: within 3 degrees.
            assert!((got - want).abs() <= 3, "column {c}: hue {got}, want {want}");
        }
    }

    #[test]
    fn rotation_maps_display_coordinates() {
        // Surface 4x2: the top-left pixel is red, everything else black.
        let data = surface(4, 2, |x, y| if (x, y) == (0, 0) { [255, 0, 0] } else { [0; 3] });
        let at = |rot| {
            let f = Frame::new(&data, 4, 2, 16, rot).unwrap();
            let (w, h) = f.display_size();
            let red: Vec<_> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| f.rgb(x, y)[0] == 255)
                .collect();
            ((w, h), red)
        };
        assert_eq!(at(Rotation::Identity), ((4, 2), vec![(0, 0)]));
        // Turned a quarter clockwise, the surface's top-left lands top-right.
        assert_eq!(at(Rotation::Rotate90), ((2, 4), vec![(1, 0)]));
        assert_eq!(at(Rotation::Rotate180), ((4, 2), vec![(3, 1)]));
        assert_eq!(at(Rotation::Rotate270), ((2, 4), vec![(0, 3)]));
    }

    #[test]
    fn small_displays_still_fill_the_thumbnail() {
        let data = surface(8, 8, |_, _| [10, 20, 30]);
        let mut out = vec![0; ZONE_IMAGE_BYTES];
        thumbnail(
            &Frame::new(&data, 8, 8, 32, Rotation::Identity).unwrap(),
            &mut out,
        );
        assert!(out.as_chunks::<4>().0.iter().all(|p| p == &[10, 20, 30, 255]));
    }
}
