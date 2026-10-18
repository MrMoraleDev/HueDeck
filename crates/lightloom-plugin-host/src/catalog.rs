// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Plugin discovery.
//!
//! [`Catalog::scan`] loads every candidate `*.js` file under the given
//! directories in an isolated runtime with the recording stub host, reads its
//! metadata exports and indexes HID-family plugins by `(vid, pid)`. Directories
//! are scanned in the order given and files in sorted order, so SignalRGB's
//! *last scanned wins* rule ([`PluginIndex`]) lets a later directory override
//! an earlier one. No plugin lifecycle function runs here.

use crate::dryrun::collect_js_files;
use crate::meta::{PluginMeta, PluginType};
use crate::runtime::{run_isolated, PluginRuntime, RuntimeOptions};
use crate::PluginIndex;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Directory names never searched for plugins (tests, fixtures, tooling).
const SKIP_DIRS: &[&str] = &["tests", "test", "fixtures", "_testkit", "node_modules", ".git"];

/// One discovered plugin.
#[derive(Clone, Debug)]
pub struct CatalogEntry {
    /// Metadata (the file path is `meta.source_path`).
    pub meta: PluginMeta,
    /// Directory the plugin was found under; relative imports may not leave it.
    pub root: PathBuf,
}

/// Plugins found under a set of directories.
#[derive(Debug, Default)]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
    index: PluginIndex,
    skipped: Vec<(PathBuf, String)>,
}

/// Whether `path` (below `root`) can hold a device plugin.
pub fn is_candidate(root: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let in_skipped_dir = rel
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .any(|c| SKIP_DIRS.iter().any(|s| c.as_os_str().eq_ignore_ascii_case(s)));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    !in_skipped_dir && name.ends_with(".js") && !name.ends_with(".test.js")
}

impl Catalog {
    /// Scan `dirs` (missing directories are reported in [`Catalog::skipped`]).
    /// `budget` bounds parse + evaluation of each file.
    pub fn scan(dirs: &[PathBuf], budget: Duration) -> Self {
        let mut catalog = Catalog::default();
        for dir in dirs {
            let files = match collect_js_files(dir) {
                Ok(f) => f,
                Err(e) => {
                    catalog
                        .skipped
                        .push((dir.clone(), format!("cannot list directory: {e}")));
                    continue;
                }
            };
            for file in files.into_iter().filter(|f| is_candidate(dir, f)) {
                match read_meta(dir, &file, budget) {
                    Ok(meta) if !meta.product_ids.is_empty() && meta.vendor_id != 0 => {
                        catalog.index.insert(&meta);
                        catalog.entries.push(CatalogEntry {
                            meta,
                            root: dir.clone(),
                        });
                    }
                    Ok(_) => catalog
                        .skipped
                        .push((file, "no VendorId/ProductId exports (helper module?)".into())),
                    Err(e) => catalog.skipped.push((file, e)),
                }
            }
        }
        catalog
    }

    /// Every plugin found, in scan order.
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// Files that were not plugins or failed to load, with the reason.
    pub fn skipped(&self) -> &[(PathBuf, String)] {
        &self.skipped
    }

    /// The `(vid, pid)` index.
    pub fn index(&self) -> &PluginIndex {
        &self.index
    }

    /// The plugin owning `(vid, pid)` under the last-scanned-wins rule.
    pub fn lookup(&self, vid: u16, pid: u16) -> Option<&CatalogEntry> {
        let path = self.index.lookup(vid, pid)?;
        self.entries.iter().rev().find(|e| e.meta.source_path == path)
    }
}

/// Load `file` with the stub host and read its metadata (HID-family plugins only).
fn read_meta(root: &Path, file: &Path, budget: Duration) -> Result<PluginMeta, String> {
    let opts = RuntimeOptions {
        plugin_root: Some(root.to_path_buf()),
        ..RuntimeOptions::default()
    };
    let loaded = run_isolated(|| -> Result<PluginMeta, String> {
        let rt = PluginRuntime::new(file, &opts).map_err(|e| e.to_string())?;
        let loaded = rt.load(budget);
        if let Some(e) = loaded.error {
            return Err(e.to_string());
        }
        let exports = loaded.exports.ok_or("no exports")?;
        Ok(exports.meta)
    })
    .map_err(|_| "plugin thread panicked".to_string())??;
    match loaded.plugin_type {
        PluginType::Hid | PluginType::Hybrid => Ok(loaded),
        other => Err(format!(
            "plugin type {other:?} is not supported by device sessions yet"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_skip_tests_and_tooling() {
        let root = Path::new("/p");
        assert!(is_candidate(root, Path::new("/p/msi/plugin.js")));
        assert!(is_candidate(root, Path::new("/p/Vendor/Thing.JS")));
        assert!(!is_candidate(root, Path::new("/p/msi/tests/packet.test.mjs")));
        assert!(!is_candidate(root, Path::new("/p/msi/tests/helper.js")));
        assert!(!is_candidate(root, Path::new("/p/_testkit/mock-device.js")));
        assert!(!is_candidate(root, Path::new("/p/x/thing.test.js")));
        assert!(!is_candidate(root, Path::new("/p/x/node_modules/y.js")));
        assert!(!is_candidate(root, Path::new("/p/x/fixtures/expected.js")));
    }

    #[test]
    fn first_party_plugins_are_indexed() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let catalog = Catalog::scan(&[dir], Duration::from_secs(10));
        let msi = catalog.lookup(0x1462, 0x7C91).expect("MSI plugin indexed");
        assert_eq!(msi.meta.name, "MSI Mystic Light 185 (MAG B550 Tomahawk)");
        assert_eq!(msi.meta.size, [6, 1]);
        assert_eq!(msi.meta.led_positions.len(), 6);
        assert!(catalog.lookup(0x1462, 0x0001).is_none());
    }
}
