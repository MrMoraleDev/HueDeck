// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Loader trait and the VID:PID plugin index.

use crate::meta::{MetaError, PluginMeta};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Errors from a [`Loader`].
#[derive(Debug, thiserror::Error)]
pub enum LoaderError {
    /// Could not read the file.
    #[error("cannot read {}: {source}", .path.display())]
    Io {
        /// File.
        path: PathBuf,
        /// Cause.
        #[source]
        source: std::io::Error,
    },
    /// The module did not parse or threw at top level.
    #[error("{}: {message}", .path.display())]
    Evaluate {
        /// File.
        path: PathBuf,
        /// Engine message.
        message: String,
    },
    /// A required export is missing (`VendorId`, `ProductId`).
    #[error("{}: missing export {export}", .path.display())]
    MissingExport {
        /// File.
        path: PathBuf,
        /// Export name.
        export: &'static str,
    },
    /// Helper module without `ProductId`: skipped, not an error for the crawler.
    #[error("{}: helper module (no ProductId export)", .0.display())]
    HelperModule(PathBuf),
    /// Metadata failed validation.
    #[error("{}: {source}", .path.display())]
    Meta {
        /// File.
        path: PathBuf,
        /// Cause.
        #[source]
        source: MetaError,
    },
    /// Nothing can be loaded yet.
    #[error("plugin loader not implemented (planned for milestone {0})")]
    NotImplemented(&'static str),
}

/// Reads a plugin module and applies the call-if-function rule to its exports.
///
/// The rquickjs implementation arrives with M0 spike B; the simulator in
/// `sdk/` and the conformance harness provide alternative implementations.
pub trait Loader: Send + Sync {
    /// Evaluate `path` as an ES module and read its metadata exports.
    fn load_meta(&self, path: &Path) -> Result<PluginMeta, LoaderError>;
}

/// Placeholder loader that refuses everything with [`LoaderError::NotImplemented`].
#[derive(Debug, Default, Clone, Copy)]
pub struct UnimplementedLoader;

impl Loader for UnimplementedLoader {
    fn load_meta(&self, _path: &Path) -> Result<PluginMeta, LoaderError> {
        Err(LoaderError::NotImplemented("M1"))
    }
}

/// Registry of USB plugins keyed by `(vid, pid)`.
///
/// SignalRGB's crawler rule is *last scanned wins*: a later file claiming an
/// existing key overwrites it and the collision is logged. [`PluginIndex::insert`]
/// returns the displaced path for exactly that log line.
#[derive(Debug, Default, Clone)]
pub struct PluginIndex {
    by_key: HashMap<(u16, u16), PathBuf>,
    collisions: Vec<Collision>,
}

/// A recorded overwrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collision {
    /// `(vid, pid)`.
    pub key: (u16, u16),
    /// The plugin that was displaced.
    pub previous: PathBuf,
    /// The plugin that now owns the key.
    pub winner: PathBuf,
}

impl PluginIndex {
    /// Empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register every `(vid, pid)` of `meta`. Returns the keys that displaced an earlier plugin.
    pub fn insert(&mut self, meta: &PluginMeta) -> Vec<Collision> {
        let mut out = Vec::new();
        for key in meta.keys() {
            if let Some(previous) = self.by_key.insert(key, meta.source_path.clone()) {
                if previous != meta.source_path {
                    let c = Collision {
                        key,
                        previous,
                        winner: meta.source_path.clone(),
                    };
                    self.collisions.push(c.clone());
                    out.push(c);
                }
            }
        }
        out
    }

    /// Plugin file owning `(vid, pid)`.
    pub fn lookup(&self, vid: u16, pid: u16) -> Option<&Path> {
        self.by_key.get(&(vid, pid)).map(PathBuf::as_path)
    }

    /// Number of registered keys (SignalRGB logs "656 modules → 1366 USB plugins").
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// Every overwrite seen so far.
    pub fn collisions(&self) -> &[Collision] {
        &self.collisions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_scanned_wins_and_collisions_are_reported() {
        let mut idx = PluginIndex::new();
        let a = PluginMeta::minimal(
            "/cdn/EVision/EVision_Keyboard_Controller.js",
            "EVision",
            0x320F,
            vec![0x5088, 0x5064],
        );
        let b = PluginMeta::minimal("/addons/qmk/GMMK_Numpad.js", "GMMK", 0x320F, vec![0x5088]);
        assert!(idx.insert(&a).is_empty());
        let c = idx.insert(&b);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].key, (0x320F, 0x5088));
        assert_eq!(c[0].previous, a.source_path);
        assert_eq!(idx.lookup(0x320F, 0x5088), Some(b.source_path.as_path()));
        assert_eq!(idx.lookup(0x320F, 0x5064), Some(a.source_path.as_path()));
        assert_eq!(idx.len(), 2);
        assert_eq!(idx.collisions().len(), 1);
        // Re-inserting the same file is not a collision.
        assert!(idx.insert(&b).is_empty());
    }

    #[test]
    fn unimplemented_loader_names_the_milestone() {
        let err = UnimplementedLoader.load_meta(Path::new("x.js")).unwrap_err();
        assert!(err.to_string().contains("M1"));
    }

    #[test]
    fn errors_display_paths() {
        let e = LoaderError::HelperModule(PathBuf::from("lib/Util.js"));
        assert!(e.to_string().contains("Util.js"));
        let e = LoaderError::MissingExport {
            path: PathBuf::from("x.js"),
            export: "ProductId",
        };
        assert_eq!(e.to_string(), "x.js: missing export ProductId");
    }
}
