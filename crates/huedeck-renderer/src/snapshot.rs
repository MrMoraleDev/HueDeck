// SPDX-License-Identifier: GPL-3.0-or-later
//! Frame snapshots: content hash, "is it black?" metrics and PNG export of a BGRA frame.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// A copy of one painted frame.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// 1-based index of the frame among the published frames.
    pub frame: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// BGRA8, tightly packed, top-left origin (CEF `OnPaint` layout).
    pub bgra: Vec<u8>,
}

/// JSON-friendly description of a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SnapshotInfo {
    /// 1-based frame index.
    pub frame: u64,
    /// SHA-256 of the raw BGRA bytes, lowercase hex.
    pub sha256: String,
    /// Fraction of pixels with max(R,G,B) > 8.
    pub nonblack_fraction: f64,
    /// Mean Rec.601 luma, 0..255.
    pub mean_luma: f64,
    /// Where the PNG was written, when requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub png: Option<String>,
}

impl Snapshot {
    /// SHA-256 of the raw BGRA bytes as lowercase hex.
    pub fn sha256_hex(&self) -> String {
        let digest = Sha256::digest(&self.bgra);
        let mut out = String::with_capacity(64);
        for b in digest {
            let _ = write!(out, "{b:02x}");
        }
        out
    }

    /// Fraction of pixels whose brightest channel exceeds 8 (0.0 = all black).
    pub fn nonblack_fraction(&self) -> f64 {
        let px = self.bgra.len() / 4;
        if px == 0 {
            return 0.0;
        }
        let lit = self
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0].max(p[1]).max(p[2]) > 8)
            .count();
        lit as f64 / px as f64
    }

    /// Mean Rec.601 luma over all pixels.
    pub fn mean_luma(&self) -> f64 {
        let px = self.bgra.len() / 4;
        if px == 0 {
            return 0.0;
        }
        let sum: f64 = self
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| 0.114 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.299 * f64::from(p[2]))
            .sum();
        sum / px as f64
    }

    /// RGB8 copy (alpha dropped: LEDs never see it; the canvas body is opaque black).
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.bgra.len() / 4 * 3);
        for p in self.bgra.as_chunks::<4>().0 {
            rgb.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        rgb
    }

    /// Write the frame as an RGB PNG.
    pub fn write_png(&self, path: &Path) -> Result<(), image::ImageError> {
        let img = image::RgbImage::from_raw(self.width, self.height, self.to_rgb()).ok_or_else(|| {
            image::ImageError::Parameter(image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ))
        })?;
        img.save_with_format(path, image::ImageFormat::Png)
    }

    /// Metrics (and PNG path, if any) for reporting.
    pub fn info(&self, png: Option<&Path>) -> SnapshotInfo {
        SnapshotInfo {
            frame: self.frame,
            sha256: self.sha256_hex(),
            nonblack_fraction: (self.nonblack_fraction() * 10_000.0).round() / 10_000.0,
            mean_luma: (self.mean_luma() * 100.0).round() / 100.0,
            png: png.map(|p| p.display().to_string()),
        }
    }
}

/// PNG path for snapshot `frame`: `base` itself when it is the only snapshot, otherwise
/// `<stem>-f<frame>.<ext>` next to it (`ext` defaults to `png`).
pub fn png_path_for(base: &Path, frame: u64, only_one: bool) -> PathBuf {
    if only_one {
        return base.to_path_buf();
    }
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "frame".into());
    let ext = base
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "png".into());
    base.with_file_name(format!("{stem}-f{frame}.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(frame: u64, b: u8, g: u8, r: u8) -> Snapshot {
        let mut bgra = Vec::new();
        for _ in 0..(4 * 2) {
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
        Snapshot {
            frame,
            width: 4,
            height: 2,
            bgra,
        }
    }

    #[test]
    fn metrics() {
        let black = solid(1, 0, 0, 0);
        assert_eq!(black.nonblack_fraction(), 0.0);
        assert_eq!(black.mean_luma(), 0.0);
        let red = solid(2, 0, 0, 255);
        assert_eq!(red.nonblack_fraction(), 1.0);
        assert!((red.mean_luma() - 76.245).abs() < 1e-9);
        assert_eq!(red.to_rgb()[..3], [255, 0, 0]);
        assert_ne!(black.sha256_hex(), red.sha256_hex());
        assert_eq!(black.sha256_hex().len(), 64);
    }

    #[test]
    fn png_names() {
        let base = Path::new("out/snap.png");
        assert_eq!(png_path_for(base, 10, true), PathBuf::from("out/snap.png"));
        assert_eq!(png_path_for(base, 100, false), PathBuf::from("out/snap-f100.png"));
        assert_eq!(png_path_for(Path::new("x"), 3, false), PathBuf::from("x-f3.png"));
    }
}
