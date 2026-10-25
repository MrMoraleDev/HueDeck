// SPDX-License-Identifier: GPL-3.0-or-later
//! Copying a captured image into the packed BGRA layout `Frame` reads.

#![cfg_attr(not(any(feature = "screencast", feature = "x11")), allow(dead_code))]

/// Byte order of a 32-bit pixel in memory (the fourth byte is ignored).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// `BGRx` / `BGRA`: what `Frame` reads, copied as is.
    Bgrx,
    /// `RGBx` / `RGBA`: red and blue swapped while copying.
    Rgbx,
}

/// Copy a `width` x `height` image whose rows start every `stride` bytes of
/// `src` into `out`, packed (`width * 4` bytes per row) and in BGRx order.
/// `false` (and `out` untouched) when `src` is too short or `stride` too small.
pub fn to_bgra(
    src: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    order: Order,
    out: &mut Vec<u8>,
) -> bool {
    let row = width * 4;
    if width == 0 || height == 0 || stride < row || src.len() < stride * (height - 1) + row {
        return false;
    }
    out.resize(row * height, 0);
    for (dst, line) in out.chunks_exact_mut(row).zip(src.chunks(stride)) {
        let line = &line[..row];
        match order {
            Order::Bgrx => dst.copy_from_slice(line),
            Order::Rgbx => {
                for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(line.as_chunks::<4>().0) {
                    *d = [s[2], s[1], s[0], s[3]];
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_rows_and_swaps_rgb() {
        // 2x2, stride 12 (4 bytes of padding per row).
        let src = [
            1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, //
            9, 10, 11, 12, 13, 14, 15, 16,
        ];
        let mut out = Vec::new();
        assert!(to_bgra(&src, 2, 2, 12, Order::Bgrx, &mut out));
        assert_eq!(out, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        assert!(to_bgra(&src, 2, 2, 12, Order::Rgbx, &mut out));
        assert_eq!(out, [3, 2, 1, 4, 7, 6, 5, 8, 11, 10, 9, 12, 15, 14, 13, 16]);
        assert!(!to_bgra(&src[..19], 2, 2, 12, Order::Bgrx, &mut out));
        assert!(!to_bgra(&src, 2, 2, 4, Order::Bgrx, &mut out));
        assert!(!to_bgra(&src, 0, 2, 12, Order::Bgrx, &mut out));
    }
}
