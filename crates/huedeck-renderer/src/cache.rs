// SPDX-License-Identifier: GPL-3.0-or-later
//! Chromium user-data dir ("root cache path") handling.
//!
//! Every run starts from an empty profile, so the previous run's files are wiped first. The path
//! comes from the command line (`--cef-cache`, default `<out>.cef-cache`), so wiping is limited to
//! directories this tool created: they carry [`CACHE_MARKER`]. A missing or empty directory is
//! adopted; anything else is refused and left untouched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// File that marks a directory as a huedeck-renderer CEF cache.
pub const CACHE_MARKER: &str = ".huedeck-renderer-cache";

const MARKER_TEXT: &str = "huedeck-renderer CEF user-data dir; wiped at the start of every run\n";

/// Why a cache directory cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// The path exists and is not a plain directory (file, symlink, junction).
    #[error("{0} exists and is not a directory")]
    NotADirectory(PathBuf),
    /// A non-empty directory without [`CACHE_MARKER`]: never wiped.
    #[error(
        "{0} is not empty and was not created by huedeck-renderer (no {CACHE_MARKER}); refusing to use it"
    )]
    NotOurs(PathBuf),
    /// File system error.
    #[error("{0}: {1}")]
    Io(PathBuf, io::Error),
}

/// What [`prepare`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prepared {
    /// The directory did not exist and was created.
    Created,
    /// An empty directory without marker was adopted.
    Adopted,
    /// One of our directories; the previous contents were removed. `stale` lists entries that
    /// could not be removed (still locked by an exiting process), for diagnostics.
    Wiped {
        /// `name: error` of each entry that survived.
        stale: Vec<String>,
    },
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> CacheError + '_ {
    move |e| CacheError::Io(path.to_path_buf(), e)
}

fn write_marker(dir: &Path) -> Result<(), CacheError> {
    let marker = dir.join(CACHE_MARKER);
    fs::write(&marker, MARKER_TEXT).map_err(io_err(&marker))
}

/// Whether `dir` carries our marker.
pub fn is_ours(dir: &Path) -> bool {
    dir.join(CACHE_MARKER).is_file()
}

/// Remove everything in `dir` except the marker. Returns `name: error` for each entry left.
fn wipe_contents(dir: &Path) -> Result<Vec<String>, CacheError> {
    let mut stale = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_err(dir))? {
        let entry = entry.map_err(io_err(dir))?;
        if entry.file_name() == CACHE_MARKER {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(io_err(&path))?;
        // remove_dir_all never follows symlinks/junctions; a link is removed as a link.
        let removed = if kind.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path).or_else(|e| {
                if kind.is_symlink() {
                    fs::remove_dir(&path)
                } else {
                    Err(e)
                }
            })
        };
        if let Err(e) = removed {
            stale.push(format!("{}: {e}", entry.file_name().to_string_lossy()));
        }
    }
    Ok(stale)
}

/// Make `dir` an empty, marked Chromium user-data dir. See the module docs for the rules.
pub fn prepare(dir: &Path) -> Result<Prepared, CacheError> {
    let meta = match fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(dir).map_err(io_err(dir))?;
            write_marker(dir)?;
            return Ok(Prepared::Created);
        }
        Err(e) => return Err(CacheError::Io(dir.to_path_buf(), e)),
    };
    if !meta.is_dir() {
        return Err(CacheError::NotADirectory(dir.to_path_buf()));
    }
    if is_ours(dir) {
        let stale = wipe_contents(dir)?;
        return Ok(Prepared::Wiped { stale });
    }
    let empty = fs::read_dir(dir).map_err(io_err(dir))?.next().is_none();
    if !empty {
        return Err(CacheError::NotOurs(dir.to_path_buf()));
    }
    write_marker(dir)?;
    Ok(Prepared::Adopted)
}

/// Remove one of our cache dirs, `attempts` times with `pause` in between while entries are
/// still locked. Does nothing to a directory without the marker. When the directory survives,
/// the marker survives with it, so the next run can still wipe it. Returns whether the
/// directory is gone. (CEF's browser process keeps a few profile LevelDB locks until it exits,
/// so from inside the renderer this usually returns `false`.)
pub fn remove(dir: &Path, attempts: u32, pause: std::time::Duration) -> bool {
    if !dir.exists() {
        return true;
    }
    if !is_ours(dir) {
        return false;
    }
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            std::thread::sleep(pause);
        }
        if matches!(wipe_contents(dir), Ok(stale) if stale.is_empty())
            && fs::remove_file(dir.join(CACHE_MARKER)).is_ok()
        {
            if fs::remove_dir(dir).is_ok() {
                return true;
            }
            // Something re-appeared between the wipe and remove_dir: mark it ours again.
            let _ = write_marker(dir);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!("hd-cache-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn creates_adopts_wipes_and_refuses() {
        let root = TempDir::new("rules");
        let dir = root.0.join("x.cef-cache");
        assert_eq!(prepare(&dir).unwrap(), Prepared::Created);
        assert!(is_ours(&dir));

        // previous run's profile is wiped, marker kept
        fs::create_dir_all(dir.join("GrShaderCache")).unwrap();
        fs::write(dir.join("GrShaderCache").join("index"), b"x").unwrap();
        fs::write(dir.join("Local State"), b"{}").unwrap();
        assert_eq!(prepare(&dir).unwrap(), Prepared::Wiped { stale: vec![] });
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from(CACHE_MARKER)]);

        // empty foreign dir is adopted
        let empty = root.0.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(prepare(&empty).unwrap(), Prepared::Adopted);
        assert!(is_ours(&empty));

        // non-empty foreign dir is refused and untouched
        let foreign = root.0.join("foreign");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("precious.txt"), b"keep me").unwrap();
        assert!(matches!(prepare(&foreign), Err(CacheError::NotOurs(_))));
        assert!(!remove(&foreign, 1, std::time::Duration::ZERO));
        assert_eq!(fs::read(foreign.join("precious.txt")).unwrap(), b"keep me");

        // a file is refused
        let file = root.0.join("file");
        fs::write(&file, b"f").unwrap();
        assert!(matches!(prepare(&file), Err(CacheError::NotADirectory(_))));

        // our dir is removed completely
        assert!(remove(&dir, 1, std::time::Duration::ZERO));
        assert!(!dir.exists());
        assert!(remove(&dir, 1, std::time::Duration::ZERO));
    }
}
