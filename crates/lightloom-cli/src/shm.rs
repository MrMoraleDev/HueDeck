// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom shm-dump`: the newest frame of the renderer's shared-memory ring
//! (`lightloom_protocol::shm`) as a PNG, with the ring header, for debugging
//! frame timing and "why is it black". The ring is mapped read-only; the
//! renderer that writes it is never disturbed.
//!
//! Without a path it takes the newest `frames-*.shm` in the daemon's renderer
//! work directory (`<data dir>/renderer`, where Windows keeps the ring; on Linux
//! the ring sits in a private tmpfs directory, so pass its path).

use anyhow::{bail, Context};
use lightloom_renderer::ring::RingReader;
use lightloom_renderer::snapshot::Snapshot;
use serde_json::json;
use std::path::{Path, PathBuf};

/// The newest `frames-*.shm` in `dir`.
pub fn newest_ring(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("frames-") && name.ends_with(".shm")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Dump the newest frame of `ring` (default: [`newest_ring`] in `work_dir`) to
/// `out` (default `frame-<seq>.png`); print what was read (`json`: as JSON).
pub fn dump(ring: Option<&Path>, work_dir: &Path, out: Option<&Path>, json: bool) -> anyhow::Result<()> {
    let ring = match ring {
        Some(r) => r.to_path_buf(),
        None => newest_ring(work_dir).with_context(|| {
            format!(
                "no frames-*.shm in {}: is an effect running in lightloomd? Pass the ring's path",
                work_dir.display()
            )
        })?,
    };
    let reader = RingReader::open_file(&ring).with_context(|| format!("opening {}", ring.display()))?;
    let header = reader.header()?;
    let mut bgra = vec![0u8; header.slot_bytes()];
    let Some(seq) = reader.read_latest(&mut bgra)? else {
        bail!("{}: no frame published yet", ring.display());
    };
    let snapshot = Snapshot {
        frame: seq,
        width: header.width,
        height: header.height,
        bgra,
    };
    let out = out.map_or_else(|| PathBuf::from(format!("frame-{seq}.png")), Path::to_path_buf);
    snapshot
        .write_png(&out)
        .with_context(|| format!("writing {}", out.display()))?;
    let info = snapshot.info(Some(&out));
    if json {
        let report = json!({
            "ring": ring.display().to_string(),
            "width": header.width,
            "height": header.height,
            "seq": header.seq,
            "slotGenerations": header.slot_generations,
            "frame": info,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "{}: {}x{}, seq {}, generations {:?}",
            ring.display(),
            header.width,
            header.height,
            header.seq,
            header.slot_generations
        );
        println!(
            "frame {seq} -> {} (lit {:.1} %, mean luma {:.1}, sha256 {})",
            out.display(),
            info.nonblack_fraction * 100.0,
            info.mean_luma,
            info.sha256
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_renderer::ring::RingWriter;

    #[test]
    fn dumps_the_newest_frame_of_a_ring() {
        let dir = std::env::temp_dir().join(format!("lightloom-shm-dump-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(newest_ring(&dir).is_none());
        let path = dir.join("frames-1.shm");
        let mut writer = RingWriter::create_file(&path, 4, 2).unwrap();
        // Nothing published yet.
        assert!(dump(Some(&path), &dir, Some(&dir.join("x.png")), false).is_err());
        let red = [0u8, 0, 255, 255].repeat(8);
        let blue = [255u8, 0, 0, 255].repeat(8);
        writer.publish(&red, 4, 2).unwrap();
        writer.publish(&blue, 4, 2).unwrap();
        writer.flush().unwrap();
        assert_eq!(newest_ring(&dir).as_deref(), Some(path.as_path()));
        let out = dir.join("frame.png");
        dump(None, &dir, Some(&out), true).unwrap();
        let png = image::open(&out).unwrap().to_rgb8();
        assert_eq!(png.dimensions(), (4, 2));
        assert_eq!(png.get_pixel(3, 1).0, [0, 0, 255], "the newest frame (blue)");
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
