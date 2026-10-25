// SPDX-License-Identifier: GPL-3.0-or-later
//! `LCD.getImageBuffer(x, y, w, h, {outputWidth, outputHeight, format})` and
//! `LCD.getSlicedFrame({columns|cols, rows, format, quality})`.
//!
//! SignalRGB's messages (components-channels.md Gap resolution): "LCD.getImageBuffer(): X
//! position [%1] is out of bounds ... LCD canvas bounds: [0 - %2)", "Output size [%1, %2]
//! ... 1-1024", "Invalid grid dimensions: columns=%1, rows=%2. Both must be positive.",
//! "LCD.getSlicedFrame(): Invalid grid configuration. Canvas size [%1x%2] divided by grid
//! [%3x%4] results in invalid cell size." Neither member has a corpus call site, so the
//! details below are Lightloom's reading of those messages:
//!
//! * a crop starts inside the canvas and is clipped to it; it is scaled (bilinear) to the
//!   output size, which defaults to the clipped region's size and must be 1-1024 per side;
//! * a grid cell is `width / columns` x `height / rows` (integer division, at least one
//!   pixel); cells come back row by row, left to right; leftover pixels at the right and
//!   bottom edges belong to no cell.

use crate::LcdError;
use image::imageops::{self, FilterType};
use image::RgbaImage;
use lightloom_engine::Frame;

/// BGRA8.
const BYTES_PER_PIXEL: usize = 4;

/// Largest `getImageBuffer` output side.
pub const MAX_OUTPUT_SIDE: u32 = 1024;

/// A `getImageBuffer` region and output size (`None`: the region's size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRequest {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Region width.
    pub width: f64,
    /// Region height.
    pub height: f64,
    /// Output width.
    pub output_width: Option<f64>,
    /// Output height.
    pub output_height: Option<f64>,
}

fn whole(v: f64) -> Option<i64> {
    (v.is_finite() && v.abs() < 1e9).then(|| v.floor() as i64)
}

fn bounds(what: &str, v: f64, limit: u32) -> Result<u32, LcdError> {
    match whole(v) {
        Some(p) if p >= 0 && p < i64::from(limit) => Ok(p as u32),
        _ => Err(LcdError::OutOfBounds(format!(
            "LCD.getImageBuffer(): {what} position [{v}] is out of bounds. LCD canvas bounds: [0 - {limit})"
        ))),
    }
}

/// Copy the `w`x`h` region at (`x`, `y`) (inside `frame`).
fn crop(frame: &Frame, x: u32, y: u32, w: u32, h: u32) -> Frame {
    let mut out = Vec::with_capacity(w as usize * h as usize * BYTES_PER_PIXEL);
    let row = frame.width() as usize * BYTES_PER_PIXEL;
    for yy in y..y + h {
        let start = yy as usize * row + x as usize * BYTES_PER_PIXEL;
        out.extend_from_slice(&frame.as_bgra()[start..start + w as usize * BYTES_PER_PIXEL]);
    }
    Frame::from_bgra(w, h, out).unwrap_or_else(|| Frame::new(w, h))
}

/// `frame` scaled to `w`x`h` (bilinear).
fn scale(frame: Frame, w: u32, h: u32) -> Frame {
    if (frame.width(), frame.height()) == (w, h) {
        return frame;
    }
    let (fw, fh) = (frame.width(), frame.height());
    let Some(img) = RgbaImage::from_raw(fw, fh, frame.as_bgra().to_vec()) else {
        return Frame::new(w, h);
    };
    // The channel order does not matter to the filter: BGRA in, BGRA out.
    let scaled = imageops::resize(&img, w, h, FilterType::Triangle);
    Frame::from_bgra(w, h, scaled.into_raw()).unwrap_or_else(|| Frame::new(w, h))
}

/// The region of `req` cut out of `frame` and scaled to the output size.
pub fn crop_scale(frame: &Frame, req: &ImageRequest) -> Result<Frame, LcdError> {
    let (fw, fh) = (frame.width(), frame.height());
    let x = bounds("X", req.x, fw)?;
    let y = bounds("Y", req.y, fh)?;
    let size = |what: &str, v: f64, room: u32| -> Result<u32, LcdError> {
        match whole(v) {
            Some(s) if s >= 1 => Ok((s.min(i64::from(room))) as u32),
            _ => Err(LcdError::OutOfBounds(format!(
                "LCD.getImageBuffer(): {what} [{v}] must be at least 1"
            ))),
        }
    };
    let w = size("width", req.width, fw - x)?;
    let h = size("height", req.height, fh - y)?;
    let out = |v: Option<f64>, region: u32| -> Option<u32> {
        match v {
            None => Some(region.min(MAX_OUTPUT_SIDE)),
            Some(v) => whole(v)
                .filter(|s| (1..=i64::from(MAX_OUTPUT_SIDE)).contains(s))
                .map(|s| s as u32),
        }
    };
    let (Some(ow), Some(oh)) = (out(req.output_width, w), out(req.output_height, h)) else {
        return Err(LcdError::OutOfBounds(format!(
            "LCD.getImageBuffer(): Output size [{}, {}] is invalid. Each side must be 1-{MAX_OUTPUT_SIDE}",
            req.output_width.unwrap_or(f64::from(w)),
            req.output_height.unwrap_or(f64::from(h))
        )));
    };
    Ok(scale(crop(frame, x, y, w, h), ow, oh))
}

/// `frame` cut into `columns` x `rows` cells, row by row.
pub fn slice(frame: &Frame, columns: f64, rows: f64) -> Result<Vec<Frame>, LcdError> {
    let positive = |v: f64| whole(v).filter(|&n| n >= 1 && v.fract() == 0.0);
    let (Some(c), Some(r)) = (positive(columns), positive(rows)) else {
        return Err(LcdError::InvalidGrid(format!(
            "Invalid grid dimensions: columns={columns}, rows={rows}. Both must be positive."
        )));
    };
    let (fw, fh) = (i64::from(frame.width()), i64::from(frame.height()));
    let (cw, ch) = (fw / c, fh / r);
    if cw < 1 || ch < 1 {
        return Err(LcdError::InvalidGrid(format!(
            "LCD.getSlicedFrame(): Invalid grid configuration. Canvas size [{fw}x{fh}] divided by grid [{c}x{r}] results in invalid cell size."
        )));
    }
    let (c, r, cw, ch) = (c as u32, r as u32, cw as u32, ch as u32);
    Ok((0..r)
        .flat_map(|row| (0..c).map(move |col| (col, row)))
        .map(|(col, row)| crop(frame, col * cw, row * ch, cw, ch))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_engine::Rgb;

    /// 8x4, pixel (x, y) = (x*10, y*10, 7).
    fn ramp() -> Frame {
        let mut f = Frame::new(8, 4);
        for y in 0..4 {
            for x in 0..8 {
                f.set_pixel(x, y, Rgb::new((x * 10) as u8, (y * 10) as u8, 7));
            }
        }
        f
    }

    fn req(x: f64, y: f64, w: f64, h: f64) -> ImageRequest {
        ImageRequest {
            x,
            y,
            width: w,
            height: h,
            output_width: None,
            output_height: None,
        }
    }

    #[test]
    fn crop_copies_the_region_and_clips_it() {
        let f = ramp();
        let c = crop_scale(&f, &req(2.0, 1.0, 3.0, 2.0)).unwrap();
        assert_eq!((c.width(), c.height()), (3, 2));
        assert_eq!(c.pixel(0, 0), Rgb::new(20, 10, 7));
        assert_eq!(c.pixel(2, 1), Rgb::new(40, 20, 7));
        let clipped = crop_scale(&f, &req(6.0, 3.0, 10.0, 10.0)).unwrap();
        assert_eq!((clipped.width(), clipped.height()), (2, 1));
        assert_eq!(clipped.pixel(1, 0), Rgb::new(70, 30, 7));
    }

    #[test]
    fn crop_scales_to_the_output_size() {
        let f = Frame::solid(8, 4, Rgb::new(9, 99, 199));
        let mut r = req(0.0, 0.0, 8.0, 4.0);
        r.output_width = Some(16.0);
        r.output_height = Some(2.0);
        let s = crop_scale(&f, &r).unwrap();
        assert_eq!((s.width(), s.height()), (16, 2));
        assert!(s.as_bgra().chunks(4).all(|p| p == [199, 99, 9, 255]));
    }

    #[test]
    fn crop_bounds_are_checked() {
        let f = ramp();
        for (r, needle) in [
            (
                req(8.0, 0.0, 1.0, 1.0),
                "X position [8] is out of bounds. LCD canvas bounds: [0 - 8)",
            ),
            (req(-1.0, 0.0, 1.0, 1.0), "X position"),
            (req(0.0, 4.0, 1.0, 1.0), "Y position [4]"),
            (req(0.0, 0.0, 0.0, 1.0), "width [0]"),
            (req(0.0, 0.0, 1.0, f64::NAN), "height"),
        ] {
            let e = crop_scale(&f, &r).unwrap_err();
            assert_eq!(e.code(), "OUT_OF_BOUNDS");
            assert!(e.to_string().contains(needle), "{e}");
        }
        let mut big = req(0.0, 0.0, 1.0, 1.0);
        big.output_width = Some(1025.0);
        assert!(crop_scale(&f, &big)
            .unwrap_err()
            .to_string()
            .contains("Output size [1025, 1]"));
    }

    #[test]
    fn slices_come_row_by_row() {
        let cells = slice(&ramp(), 4.0, 2.0).unwrap();
        assert_eq!(cells.len(), 8);
        assert!(cells.iter().all(|c| (c.width(), c.height()) == (2, 2)));
        assert_eq!(cells[0].pixel(0, 0), Rgb::new(0, 0, 7));
        assert_eq!(cells[3].pixel(1, 1), Rgb::new(70, 10, 7));
        assert_eq!(cells[4].pixel(0, 0), Rgb::new(0, 20, 7));
        // 8 / 3 = 2 px per column; the last two columns belong to no cell
        let odd = slice(&ramp(), 3.0, 1.0).unwrap();
        assert_eq!(odd.len(), 3);
        assert_eq!(odd[2].pixel(1, 0), Rgb::new(50, 0, 7));
    }

    #[test]
    fn bad_grids_are_refused() {
        for (c, r) in [(0.0, 1.0), (1.0, -2.0), (1.5, 1.0), (f64::NAN, 1.0)] {
            let e = slice(&ramp(), c, r).unwrap_err();
            assert_eq!(e.code(), "INVALID_GRID");
            assert!(e.to_string().starts_with("Invalid grid dimensions"), "{e}");
        }
        let e = slice(&ramp(), 9.0, 1.0).unwrap_err();
        assert!(
            e.to_string().contains("Canvas size [8x4] divided by grid [9x1]"),
            "{e}"
        );
    }
}
