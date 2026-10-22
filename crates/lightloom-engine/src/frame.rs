// SPDX-License-Identifier: GPL-3.0-or-later
//! A rendered canvas frame.

use crate::color::Rgb;

/// Bytes per pixel (BGRA8, the CEF off-screen-render format).
pub const BYTES_PER_PIXEL: usize = 4;

/// One canvas frame, BGRA8 row-major without padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Frame {
    /// All-black frame.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0u8; width as usize * height as usize * BYTES_PER_PIXEL],
        }
    }

    /// Frame filled with one colour (alpha 255).
    pub fn solid(width: u32, height: u32, color: Rgb) -> Self {
        let mut f = Self::new(width, height);
        for px in f.pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            px.copy_from_slice(&[color.b, color.g, color.r, 0xFF]);
        }
        f
    }

    /// Wrap an existing BGRA buffer. Returns `None` when the length does not match.
    pub fn from_bgra(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        (pixels.len() == width as usize * height as usize * BYTES_PER_PIXEL).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Raw BGRA bytes.
    pub fn as_bgra(&self) -> &[u8] {
        &self.pixels
    }

    /// Mutable raw BGRA bytes.
    pub fn as_bgra_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Number of pixels.
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Colour at a linear pixel offset (`y * width + x`). Black when out of range.
    #[inline]
    pub fn pixel_at_offset(&self, offset: usize) -> Rgb {
        let at = offset * BYTES_PER_PIXEL;
        match self.pixels.get(at..at + 3) {
            Some(bgr) => Rgb {
                r: bgr[2],
                g: bgr[1],
                b: bgr[0],
            },
            None => Rgb::BLACK,
        }
    }

    /// Colour at `(x, y)`. Black when out of range.
    pub fn pixel(&self, x: u32, y: u32) -> Rgb {
        if x >= self.width || y >= self.height {
            return Rgb::BLACK;
        }
        self.pixel_at_offset(y as usize * self.width as usize + x as usize)
    }

    /// Set the colour at `(x, y)` (alpha 255). Ignored when out of range.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgb) {
        if x >= self.width || y >= self.height {
            return;
        }
        let at = (y as usize * self.width as usize + x as usize) * BYTES_PER_PIXEL;
        self.pixels[at..at + 4].copy_from_slice(&[color.b, color.g, color.r, 0xFF]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_and_pixel_access() {
        let f = Frame::solid(4, 2, Rgb::new(1, 2, 3));
        assert_eq!(f.as_bgra().len(), 32);
        assert_eq!(&f.as_bgra()[..4], &[3, 2, 1, 255]);
        assert_eq!(f.pixel(3, 1), Rgb::new(1, 2, 3));
        assert_eq!(f.pixel(4, 0), Rgb::BLACK);
        let mut f = Frame::new(4, 2);
        f.set_pixel(2, 1, Rgb::WHITE);
        assert_eq!(f.pixel_at_offset(6), Rgb::WHITE);
        assert_eq!(f.pixel_at_offset(99), Rgb::BLACK);
        assert!(Frame::from_bgra(2, 2, vec![0; 15]).is_none());
    }
}
