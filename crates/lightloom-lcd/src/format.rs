// SPDX-License-Identifier: GPL-3.0-or-later
//! `LCD.getFrame({format, quality})` formats and their encoders.
//!
//! SignalRGB's validator names eight formats (exe string "Valid formats are: JPEG, PNG,
//! BMP, RGBA, RGB, RGB565, NZXT::RGB565, NZXT::Q565"), matched case-insensitively (the
//! corpus passes `"jpeg"`). Layouts, as implemented here:
//!
//! | format | bytes |
//! |---|---|
//! | `JPEG` | baseline JPEG file, `quality` 1-100 (default 75, Qt's default) |
//! | `PNG` | PNG file, 8-bit RGB |
//! | `BMP` | BMP file: 54-byte header (`BITMAPINFOHEADER`), 24-bit BGR, rows bottom-up, each padded to 4 bytes |
//! | `RGBA` | R, G, B, A (A = 255) per pixel, rows top-down, no padding |
//! | `RGB` | R, G, B per pixel, rows top-down, **each row padded with zeros to a multiple of 4 bytes** |
//! | `RGB565` | one little-endian `u16` per pixel, `r5 << 11 \| g6 << 5 \| b5`, rows top-down, each padded to a multiple of 4 bytes |
//! | `NZXT::RGB565`, `NZXT::Q565` | not implemented: [`LcdError::UnsupportedFormat`] |
//!
//! The `RGB` row padding is documented by the corpus (CoolerMaster Atmos II: "LCD.getFrame
//! pads each row up to a 4-byte boundary"; hid-usb.md Gap resolution). It is the memory
//! layout of a Qt `QImage` (scanlines 32-bit aligned), so the other raw formats follow the
//! same `QImage` layouts: `Format_RGBA8888` and `Format_RGB16` (native-endian `u16`, little
//! endian on every platform SignalRGB runs on). For the widths seen in the corpus the
//! RGB565 padding never applies (even widths); it only matters for odd ones (17x19).
//!
//! The two NZXT formats are named in the validator only. "NZXT::RGB565" is RGB565 in some
//! NZXT-specific byte or pixel order and "NZXT::Q565" a quantised/compressed variant; no
//! public document gives either layout, so these return a typed error rather than bytes
//! in a guessed layout that a Kraken would show as garbage.

use crate::LcdError;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use lightloom_engine::Frame;
use serde::Serialize;

/// JPEG quality when the plugin gives none (Qt's `QImageWriter` default).
pub const DEFAULT_JPEG_QUALITY: u8 = 75;

/// A `getFrame` format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum LcdFormat {
    /// JPEG file.
    Jpeg,
    /// PNG file.
    Png,
    /// BMP file.
    Bmp,
    /// Raw RGBA.
    Rgba,
    /// Raw RGB, rows padded to 4 bytes.
    Rgb,
    /// Raw RGB565 little endian, rows padded to 4 bytes.
    Rgb565,
    /// NZXT's RGB565 variant (layout unknown).
    NzxtRgb565,
    /// NZXT's Q565 (layout unknown).
    NzxtQ565,
}

impl LcdFormat {
    /// All formats, in the validator's order.
    pub const ALL: [LcdFormat; 8] = [
        LcdFormat::Jpeg,
        LcdFormat::Png,
        LcdFormat::Bmp,
        LcdFormat::Rgba,
        LcdFormat::Rgb,
        LcdFormat::Rgb565,
        LcdFormat::NzxtRgb565,
        LcdFormat::NzxtQ565,
    ];

    /// The canonical name (as the validator spells it).
    pub fn name(self) -> &'static str {
        match self {
            LcdFormat::Jpeg => "JPEG",
            LcdFormat::Png => "PNG",
            LcdFormat::Bmp => "BMP",
            LcdFormat::Rgba => "RGBA",
            LcdFormat::Rgb => "RGB",
            LcdFormat::Rgb565 => "RGB565",
            LcdFormat::NzxtRgb565 => "NZXT::RGB565",
            LcdFormat::NzxtQ565 => "NZXT::Q565",
        }
    }

    /// Parse a format name, case-insensitively (`"jpeg"`, `"Jpeg"`, `"nzxt::q565"`).
    /// `"JPG"` is accepted as JPEG.
    pub fn parse(name: &str) -> Result<Self, LcdError> {
        let n = name.trim();
        if n.eq_ignore_ascii_case("jpg") {
            return Ok(LcdFormat::Jpeg);
        }
        Self::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(n))
            .ok_or_else(|| LcdError::InvalidFormat(name.chars().take(64).collect()))
    }

    /// Why this format cannot be produced, `None` when it can.
    pub fn unsupported_reason(self) -> Option<&'static str> {
        match self {
            LcdFormat::NzxtRgb565 => Some(
                "its byte and pixel order are not publicly documented (NZXT Kraken LCDs); \
                 use a documented format or contribute a capture-backed layout",
            ),
            LcdFormat::NzxtQ565 => Some(
                "its quantised encoding is not publicly documented (NZXT Kraken LCDs); \
                 use a documented format or contribute a capture-backed layout",
            ),
            _ => None,
        }
    }
}

impl std::fmt::Display for LcdFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Bytes of one row of `width` pixels at `bytes_per_pixel`, padded to a multiple of 4.
pub fn padded_stride(width: u32, bytes_per_pixel: usize) -> usize {
    (width as usize * bytes_per_pixel).div_ceil(4) * 4
}

/// The frame as tightly packed RGB8 rows (premultiplied BGRA over black = its BGR).
fn rgb8(frame: &Frame) -> Vec<u8> {
    let mut out = Vec::with_capacity(frame.pixel_count() * 3);
    for px in frame.as_bgra().as_chunks::<4>().0 {
        out.extend_from_slice(&[px[2], px[1], px[0]]);
    }
    out
}

/// Raw rows of `bytes_per_pixel`, each padded to 4 bytes; `put` writes one pixel.
fn raw_rows(frame: &Frame, bytes_per_pixel: usize, put: impl Fn(&[u8; 4], &mut [u8])) -> Vec<u8> {
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let stride = padded_stride(frame.width(), bytes_per_pixel);
    let mut out = vec![0u8; stride * h];
    let pixels = frame.as_bgra().as_chunks::<4>().0;
    for y in 0..h {
        let row = &mut out[y * stride..y * stride + w * bytes_per_pixel];
        for (x, dst) in row.chunks_exact_mut(bytes_per_pixel).enumerate() {
            put(&pixels[y * w + x], dst);
        }
    }
    out
}

/// `r5 g6 b5` of a BGRA pixel.
pub fn rgb565(bgra: &[u8; 4]) -> u16 {
    let (r, g, b) = (u16::from(bgra[2]), u16::from(bgra[1]), u16::from(bgra[0]));
    ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)
}

/// A 24-bit bottom-up BMP file of the frame.
fn bmp(frame: &Frame) -> Vec<u8> {
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let stride = padded_stride(frame.width(), 3);
    let image = stride * h;
    let mut out = Vec::with_capacity(54 + image);
    let u32le = |v: usize| (v as u32).to_le_bytes();
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&u32le(54 + image));
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&u32le(54));
    // BITMAPINFOHEADER: positive height = bottom-up rows
    out.extend_from_slice(&u32le(40));
    out.extend_from_slice(&u32le(w));
    out.extend_from_slice(&u32le(h));
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&u32le(0)); // BI_RGB
    out.extend_from_slice(&u32le(image));
    out.extend_from_slice(&u32le(2835)); // 72 dpi
    out.extend_from_slice(&u32le(2835));
    out.extend_from_slice(&u32le(0));
    out.extend_from_slice(&u32le(0));
    let pixels = frame.as_bgra().as_chunks::<4>().0;
    for y in (0..h).rev() {
        let start = out.len();
        for px in &pixels[y * w..(y + 1) * w] {
            out.extend_from_slice(&[px[0], px[1], px[2]]);
        }
        out.resize(start + stride, 0);
    }
    out
}

/// Encode `frame` as `format`. `quality` applies to JPEG only (clamped to 1-100,
/// default [`DEFAULT_JPEG_QUALITY`]).
pub fn encode(frame: &Frame, format: LcdFormat, quality: Option<u8>) -> Result<Vec<u8>, LcdError> {
    if let Some(reason) = format.unsupported_reason() {
        return Err(LcdError::UnsupportedFormat {
            format: format.name(),
            reason,
        });
    }
    let (w, h) = (frame.width(), frame.height());
    let mut out = Vec::new();
    match format {
        LcdFormat::Jpeg => {
            let q = quality.unwrap_or(DEFAULT_JPEG_QUALITY).clamp(1, 100);
            JpegEncoder::new_with_quality(&mut out, q)
                .write_image(&rgb8(frame), w, h, ExtendedColorType::Rgb8)
                .map_err(|e| LcdError::Encode(e.to_string()))?;
        }
        LcdFormat::Png => {
            PngEncoder::new(&mut out)
                .write_image(&rgb8(frame), w, h, ExtendedColorType::Rgb8)
                .map_err(|e| LcdError::Encode(e.to_string()))?;
        }
        LcdFormat::Bmp => out = bmp(frame),
        LcdFormat::Rgba => {
            out = raw_rows(frame, 4, |px, dst| {
                dst.copy_from_slice(&[px[2], px[1], px[0], 0xFF])
            });
        }
        LcdFormat::Rgb => out = raw_rows(frame, 3, |px, dst| dst.copy_from_slice(&[px[2], px[1], px[0]])),
        LcdFormat::Rgb565 => {
            out = raw_rows(frame, 2, |px, dst| dst.copy_from_slice(&rgb565(px).to_le_bytes()))
        }
        LcdFormat::NzxtRgb565 | LcdFormat::NzxtQ565 => unreachable!("refused above"),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_engine::Rgb;

    /// 3x2: red, green, blue / white, black, (10, 20, 30).
    fn sample() -> Frame {
        let mut f = Frame::new(3, 2);
        f.set_pixel(0, 0, Rgb::new(255, 0, 0));
        f.set_pixel(1, 0, Rgb::new(0, 255, 0));
        f.set_pixel(2, 0, Rgb::new(0, 0, 255));
        f.set_pixel(0, 1, Rgb::new(255, 255, 255));
        f.set_pixel(1, 1, Rgb::new(0, 0, 0));
        f.set_pixel(2, 1, Rgb::new(10, 20, 30));
        f
    }

    #[test]
    fn names_parse_case_insensitively() {
        for f in LcdFormat::ALL {
            assert_eq!(LcdFormat::parse(f.name()).unwrap(), f);
            assert_eq!(LcdFormat::parse(&f.name().to_lowercase()).unwrap(), f);
        }
        assert_eq!(LcdFormat::parse("jpeg").unwrap(), LcdFormat::Jpeg);
        assert_eq!(LcdFormat::parse(" Jpg ").unwrap(), LcdFormat::Jpeg);
        let e = LcdFormat::parse("GIF").unwrap_err();
        assert_eq!(e.code(), "INVALID_FORMAT");
        assert!(e.to_string().contains("Valid formats are: JPEG, PNG, BMP"));
    }

    #[test]
    fn rgb_rows_are_padded_to_four_bytes() {
        let out = encode(&sample(), LcdFormat::Rgb, None).unwrap();
        // 3 px * 3 bytes = 9 -> 12 per row
        assert_eq!(padded_stride(3, 3), 12);
        assert_eq!(out.len(), 24);
        assert_eq!(&out[..12], &[255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]);
        assert_eq!(&out[12..], &[255, 255, 255, 0, 0, 0, 10, 20, 30, 0, 0, 0]);
        // 320 px rows are already aligned: width*height*3 exactly (Ryujin III expects 230400).
        let wide = encode(&Frame::new(320, 240), LcdFormat::Rgb, None).unwrap();
        assert_eq!(wide.len(), 230_400);
    }

    #[test]
    fn rgba_is_opaque_and_unpadded() {
        let out = encode(&sample(), LcdFormat::Rgba, None).unwrap();
        assert_eq!(out.len(), 3 * 2 * 4);
        assert_eq!(&out[..8], &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(&out[20..], &[10, 20, 30, 255]);
        // A fresh (all-zero, alpha 0) frame still comes out opaque black.
        assert!(encode(&Frame::new(2, 1), LcdFormat::Rgba, None)
            .unwrap()
            .chunks(4)
            .all(|p| p == [0, 0, 0, 255]));
    }

    #[test]
    fn rgb565_is_little_endian_and_row_padded() {
        assert_eq!(rgb565(&[0, 0, 255, 255]), 0xF800);
        assert_eq!(rgb565(&[0, 255, 0, 255]), 0x07E0);
        assert_eq!(rgb565(&[255, 0, 0, 255]), 0x001F);
        assert_eq!(rgb565(&[255, 255, 255, 255]), 0xFFFF);
        // (10, 20, 30): r 1, g 5, b 3 -> 0x08A3
        assert_eq!(rgb565(&[30, 20, 10, 255]), 0x08A3);
        let out = encode(&sample(), LcdFormat::Rgb565, None).unwrap();
        // 3 px * 2 bytes = 6 -> 8 per row
        assert_eq!(out.len(), 16);
        assert_eq!(&out[..8], &[0x00, 0xF8, 0xE0, 0x07, 0x1F, 0x00, 0, 0]);
        assert_eq!(&out[8..], &[0xFF, 0xFF, 0x00, 0x00, 0xA3, 0x08, 0, 0]);
        assert_eq!(
            encode(&Frame::new(240, 240), LcdFormat::Rgb565, None)
                .unwrap()
                .len(),
            240 * 240 * 2
        );
    }

    #[test]
    fn bmp_is_a_bottom_up_24_bit_file() {
        let out = encode(&sample(), LcdFormat::Bmp, None).unwrap();
        assert_eq!(&out[..2], b"BM");
        assert_eq!(
            u32::from_le_bytes(out[2..6].try_into().unwrap()) as usize,
            out.len()
        );
        assert_eq!(u32::from_le_bytes(out[10..14].try_into().unwrap()), 54);
        assert_eq!(i32::from_le_bytes(out[18..22].try_into().unwrap()), 3);
        assert_eq!(i32::from_le_bytes(out[22..26].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(out[28..30].try_into().unwrap()), 24);
        assert_eq!(out.len(), 54 + 12 * 2);
        // first stored row is the bottom one, BGR
        assert_eq!(&out[54..66], &[255, 255, 255, 0, 0, 0, 30, 20, 10, 0, 0, 0]);
        assert_eq!(&out[66..78], &[0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn png_and_jpeg_round_trip() {
        let f = Frame::solid(16, 8, Rgb::new(200, 40, 90));
        let png = encode(&f, LcdFormat::Png, None).unwrap();
        let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .unwrap()
            .to_rgb8();
        assert_eq!(img.dimensions(), (16, 8));
        assert!(img.pixels().all(|p| p.0 == [200, 40, 90]));

        let hi = encode(&f, LcdFormat::Jpeg, Some(95)).unwrap();
        assert_eq!(&hi[..2], &[0xFF, 0xD8]);
        let img = image::load_from_memory_with_format(&hi, image::ImageFormat::Jpeg)
            .unwrap()
            .to_rgb8();
        assert_eq!(img.dimensions(), (16, 8));
        let p = img.get_pixel(8, 4).0;
        assert!(
            p.iter().zip([200u8, 40, 90]).all(|(a, b)| a.abs_diff(b) < 12),
            "{p:?}"
        );
        // quality is honoured (a noisy frame shrinks at low quality) and clamped
        let mut noisy = Frame::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                noisy.set_pixel(
                    x,
                    y,
                    Rgb::new(
                        (x * 37 % 256) as u8,
                        (y * 91 % 256) as u8,
                        ((x ^ y) * 13 % 256) as u8,
                    ),
                );
            }
        }
        let small = encode(&noisy, LcdFormat::Jpeg, Some(10)).unwrap();
        let large = encode(&noisy, LcdFormat::Jpeg, Some(100)).unwrap();
        assert!(small.len() < large.len());
        assert!(encode(&noisy, LcdFormat::Jpeg, Some(0)).is_ok());
    }

    #[test]
    fn nzxt_formats_fail_typed() {
        for f in [LcdFormat::NzxtRgb565, LcdFormat::NzxtQ565] {
            let e = encode(&sample(), f, None).unwrap_err();
            assert_eq!(e.code(), "UNSUPPORTED_FORMAT");
            assert!(e.to_string().contains(f.name()));
        }
    }
}
