// SPDX-License-Identifier: GPL-3.0-or-later
//! Plugin discovery.
//!
//! [`Catalog::scan`] loads every candidate `*.js` file under the given
//! directories in an isolated runtime with the recording stub host, reads its
//! metadata exports and indexes HID plugins by `(vid, pid)`. Directories are
//! scanned in the order given and files in sorted order, so SignalRGB's *last
//! scanned wins* rule ([`PluginIndex`]) lets a later directory override an
//! earlier one. No plugin lifecycle function runs here.
//!
//! SMBus plugins (`Type() == "SMBUS"`) are not keyed by VID:PID: they are
//! offered buses ([`Catalog::smbus_candidates`]) and find their devices with
//! `Scan(bus)`. Only those that declare their scan window with the Lightloom
//! extension `SmbusAddresses()` are kept, so a third-party SMBus plugin never
//! runs its `Scan()` writes on a chipset bus.
//!
//! Loading a file costs a few milliseconds, so a full SignalRGB corpus (about
//! a thousand files) would cost seconds on every start. [`Catalog::scan_with`]
//! keeps an index cache: one entry per file, keyed by path, size and
//! modification time, holding the metadata (or the reason the file was not a
//! plugin). Only new or changed files are loaded again, several at a time.
//!
//! [`signalrgb_plugin_dirs`] lists the plugin directories of a local SignalRGB
//! installation in SignalRGB's own scan order, so its plugins can be used in
//! place (they are read, never copied).

use crate::dryrun::collect_js_files;
use crate::meta::{PluginMeta, PluginType};
use crate::runtime::{run_isolated, PluginRuntime, RuntimeOptions};
use crate::PluginIndex;
use lightloom_hal::hid::HidEndpointInfo;
use lightloom_hal::smbus::{SmbusBusDescriptor, SMBUS_ADDR_MAX, SMBUS_ADDR_MIN};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

/// Directory names never searched for plugins (tests, fixtures, tooling).
const SKIP_DIRS: &[&str] = &["tests", "test", "fixtures", "_testkit", "node_modules", ".git"];

/// Bumped whenever what the cache stores (or how metadata is read) changes.
const INDEX_VERSION: u32 = 6;

/// One discovered plugin.
#[derive(Clone, Debug)]
pub struct CatalogEntry {
    /// Metadata (the file path is `meta.source_path`).
    pub meta: PluginMeta,
    /// Directory the plugin was found under; relative imports may not leave it.
    pub root: PathBuf,
}

/// Counters of one scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanStats {
    /// Candidate files seen.
    pub files: usize,
    /// Files whose cached result was reused.
    pub cached: usize,
    /// Files loaded in a runtime.
    pub loaded: usize,
}

/// Plugins found under a set of directories.
#[derive(Debug, Default)]
pub struct Catalog {
    entries: Vec<CatalogEntry>,
    index: PluginIndex,
    skipped: Vec<(PathBuf, String)>,
    stats: ScanStats,
}

/// How [`Catalog::scan_with`] works.
#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// Bound on parse + evaluation of one file.
    pub budget: Duration,
    /// Index cache file (read, then rewritten when something changed).
    pub cache: Option<PathBuf>,
    /// Files loaded at the same time (each in its own runtime and thread).
    pub jobs: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            budget: Duration::from_secs(10),
            cache: None,
            jobs: std::thread::available_parallelism().map_or(4, |n| n.get()),
        }
    }
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

/// Size and modification time of a file, the cache key next to its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    len: u64,
    modified_ns: u128,
}

impl Stamp {
    fn of(path: &Path) -> Option<Self> {
        let m = std::fs::metadata(path).ok()?;
        let modified_ns = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
        Some(Self {
            len: m.len(),
            modified_ns,
        })
    }
}

/// What loading one file produced: its metadata, or why it is not usable.
#[derive(Clone, Debug, Serialize, Deserialize)]
enum Loaded {
    Meta(Box<PluginMeta>),
    Failed(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedFile {
    stamp: Stamp,
    loaded: Loaded,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct IndexCache {
    version: u32,
    host: String,
    files: BTreeMap<String, CachedFile>,
}

impl IndexCache {
    fn host_tag() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<IndexCache>(&bytes).ok())
            .filter(|c| c.version == INDEX_VERSION && c.host == Self::host_tag())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

impl Catalog {
    /// Scan `dirs` without a cache (missing directories are reported in
    /// [`Catalog::skipped`]). `budget` bounds parse + evaluation of each file.
    pub fn scan(dirs: &[PathBuf], budget: Duration) -> Self {
        Self::scan_with(
            dirs,
            &ScanOptions {
                budget,
                cache: None,
                jobs: 1,
            },
        )
    }

    /// Scan `dirs` with the options' cache and parallelism.
    pub fn scan_with(dirs: &[PathBuf], opts: &ScanOptions) -> Self {
        let mut catalog = Catalog::default();
        let mut cache = opts.cache.as_deref().map(IndexCache::load).unwrap_or_default();

        // 1. Candidate files in scan order.
        let mut files: Vec<(PathBuf, PathBuf, Option<Stamp>)> = Vec::new();
        for dir in dirs {
            match collect_js_files(dir) {
                Ok(list) => files.extend(list.into_iter().filter(|f| is_candidate(dir, f)).map(|f| {
                    let stamp = Stamp::of(&f);
                    (dir.clone(), f, stamp)
                })),
                Err(e) => catalog
                    .skipped
                    .push((dir.clone(), format!("cannot list directory: {e}"))),
            }
        }
        catalog.stats.files = files.len();

        // 2. Reuse cached results; load the rest, several at a time.
        let mut results: Vec<Option<Loaded>> = files
            .iter()
            .map(|(_, file, stamp)| {
                let hit = cache.files.get(&file.to_string_lossy().into_owned())?;
                (Some(hit.stamp) == *stamp).then(|| hit.loaded.clone())
            })
            .collect();
        catalog.stats.cached = results.iter().filter(|r| r.is_some()).count();
        let missing: Vec<usize> = (0..files.len()).filter(|&i| results[i].is_none()).collect();
        catalog.stats.loaded = missing.len();
        if !missing.is_empty() {
            let next = AtomicUsize::new(0);
            let done = Mutex::new(Vec::with_capacity(missing.len()));
            std::thread::scope(|s| {
                for _ in 0..opts.jobs.clamp(1, missing.len()) {
                    s.spawn(|| loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&i) = missing.get(k) else { break };
                        let (root, file, _) = &files[i];
                        let loaded = load_meta(root, file, opts.budget);
                        done.lock().unwrap_or_else(|e| e.into_inner()).push((i, loaded));
                    });
                }
            });
            for (i, loaded) in done.into_inner().unwrap_or_else(|e| e.into_inner()) {
                if let Some(stamp) = files[i].2 {
                    cache.files.insert(
                        files[i].1.to_string_lossy().into_owned(),
                        CachedFile {
                            stamp,
                            loaded: loaded.clone(),
                        },
                    );
                }
                results[i] = Some(loaded);
            }
        }

        // 3. Index in scan order (last scanned wins).
        for ((root, file, _), loaded) in files.iter().zip(results) {
            match loaded.map(accept) {
                Some(Ok(meta)) => {
                    if !matches!(meta.plugin_type, PluginType::Smbus | PluginType::Network) {
                        catalog.index.insert(&meta);
                    }
                    catalog.entries.push(CatalogEntry {
                        meta,
                        root: root.clone(),
                    });
                }
                Some(Err(why)) => catalog.skipped.push((file.clone(), why)),
                None => {}
            }
        }

        // 4. Persist the cache (only entries for files that still exist).
        if let Some(path) = &opts.cache {
            let present: BTreeSet<String> = files
                .iter()
                .map(|(_, f, _)| f.to_string_lossy().into_owned())
                .collect();
            let stale = cache.files.keys().any(|k| !present.contains(k));
            if catalog.stats.loaded > 0 || stale {
                cache.files.retain(|k, _| present.contains(k));
                cache.version = INDEX_VERSION;
                cache.host = IndexCache::host_tag();
                if let Err(e) = cache.save(path) {
                    tracing::warn!(path = %path.display(), error = %e, "plugin index cache not written");
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

    /// Counters of the scan that built this catalog.
    pub fn stats(&self) -> ScanStats {
        self.stats
    }

    /// The network plugins, in scan order; of two with the same `Name()` the
    /// later scanned one is kept (its service and settings are keyed by name).
    pub fn network_plugins(&self) -> Vec<&CatalogEntry> {
        let all: Vec<&CatalogEntry> = self
            .entries
            .iter()
            .filter(|e| e.meta.plugin_type == PluginType::Network)
            .collect();
        all.iter()
            .enumerate()
            .filter(|(i, e)| !all[i + 1..].iter().any(|l| l.meta.name == e.meta.name))
            .map(|(_, e)| *e)
            .collect()
    }

    /// The SMBus plugins, in scan order.
    pub fn smbus_plugins(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.entries
            .iter()
            .filter(|e| e.meta.plugin_type == PluginType::Smbus)
    }

    /// The SMBus plugins whose `Scan(bus)` is offered the bus `d`
    /// ([`PluginMeta::smbus_offered`]), in scan order; of two plugins with the
    /// same `Name()` the later scanned one is kept.
    pub fn smbus_candidates(&self, d: &SmbusBusDescriptor) -> Vec<&CatalogEntry> {
        let offered: Vec<&CatalogEntry> = self.smbus_plugins().filter(|e| e.meta.smbus_offered(d)).collect();
        offered
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                !offered[i + 1..]
                    .iter()
                    .any(|later| later.meta.name == e.meta.name)
            })
            .map(|(_, e)| *e)
            .collect()
    }

    /// The plugin owning `(vid, pid)` under the last-scanned-wins rule.
    pub fn lookup(&self, vid: u16, pid: u16) -> Option<&CatalogEntry> {
        let path = self.index.lookup(vid, pid)?;
        self.entries.iter().rev().find(|e| e.meta.source_path == path)
    }

    /// The plugin for a device: the one owning `(vid, pid)`, else the last
    /// scanned plugin whose `HidUsages()` names the usage page and usage of
    /// one of the device's HID collections (a generic driver, e.g. HID
    /// LampArray).
    pub fn lookup_device(&self, vid: u16, pid: u16, endpoints: &[HidEndpointInfo]) -> Option<&CatalogEntry> {
        self.lookup(vid, pid).or_else(|| {
            self.entries.iter().rev().find(|e| {
                e.meta.hid_usages.iter().any(|[page, usage]| {
                    endpoints
                        .iter()
                        .any(|ep| ep.usage_page == *page && ep.usage == *usage)
                })
            })
        })
    }
}

/// Load `file` with the stub host and read its metadata.
fn load_meta(root: &Path, file: &Path, budget: Duration) -> Loaded {
    let opts = RuntimeOptions {
        plugin_root: Some(root.to_path_buf()),
        ..RuntimeOptions::default()
    };
    let result = run_isolated(|| -> Result<PluginMeta, String> {
        let rt = PluginRuntime::new(file, &opts).map_err(|e| e.to_string())?;
        let loaded = rt.load(budget);
        if let Some(e) = loaded.error {
            return Err(e.to_string());
        }
        Ok(loaded.exports.ok_or("no exports")?.meta)
    });
    match result {
        Ok(Ok(meta)) => Loaded::Meta(Box::new(meta)),
        Ok(Err(e)) => Loaded::Failed(e),
        Err(_) => Loaded::Failed("plugin thread panicked".into()),
    }
}

/// Whether a loaded file is a plugin device sessions can run.
fn accept(loaded: Loaded) -> Result<PluginMeta, String> {
    let meta = match loaded {
        Loaded::Meta(meta) => *meta,
        Loaded::Failed(why) => return Err(why),
    };
    // rawusb and hybrid plugins get WinUSB bulk/control transfers (a device
    // without WinUSB is reported as needing it); serial plugins have no
    // device session yet.
    match meta.plugin_type {
        PluginType::Smbus => {
            if meta.smbus_addresses.is_empty() {
                return Err(
                    "SMBus plugin without a SmbusAddresses() scan window (Lightloom only runs SMBus \
                            plugins that declare it)"
                        .into(),
                );
            }
            if let Some(a) = meta
                .smbus_addresses
                .iter()
                .find(|a| !(SMBUS_ADDR_MIN..=SMBUS_ADDR_MAX).contains(a))
            {
                return Err(format!("SmbusAddresses() lists 0x{a:02x}, outside 0x03..=0x77"));
            }
            if !meta.lifecycle.scan || !meta.lifecycle.is_device() {
                return Err("SMBus plugin without Scan(bus), Initialize() and Render() exports".into());
            }
            Ok(meta)
        }
        PluginType::Network => {
            if !meta.lifecycle.discovery_service || !meta.lifecycle.is_device() {
                return Err(
                    "network plugin without DiscoveryService, Initialize() and Render() exports".into(),
                );
            }
            Ok(meta)
        }
        _ if (meta.product_ids.is_empty() || meta.vendor_id == 0) && meta.hid_usages.is_empty() => {
            Err("no VendorId/ProductId exports (helper module?)".into())
        }
        PluginType::Hid | PluginType::RawUsb | PluginType::Hybrid => Ok(meta),
        other => Err(format!(
            "plugin type {other:?} is not supported by device sessions yet"
        )),
    }
}

/// Plugin directories of a local SignalRGB installation, in the order
/// SignalRGB scans them (later directories win on VID:PID collisions):
/// the bundled plugins of the newest installed app version, the plugin CDN
/// cache, each addon repository, the user's `Documents\WhirlwindFX\Plugins`.
/// Missing directories are left out; empty on non-Windows hosts.
pub fn signalrgb_plugin_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !cfg!(windows) {
        return out;
    }
    if let Some(app) = signalrgb_app_dir() {
        out.push(app.join("Signal-x64").join("Plugins"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        let cache = local.join("WhirlwindFX").join("SignalRgb").join("cache");
        out.push(cache.join("plugin_cdn").join("main").join("Plugins"));
        if let Ok(rd) = std::fs::read_dir(cache.join("addons")) {
            let mut addons: Vec<PathBuf> = rd
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect();
            addons.sort();
            out.extend(addons);
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        out.push(home.join("Documents").join("WhirlwindFX").join("Plugins"));
    }
    out.retain(|d| d.is_dir());
    out
}

/// The newest installed SignalRGB app directory
/// (`%LOCALAPPDATA%\VortxEngine\app-x.y.z`), if any; `None` on non-Windows hosts.
pub fn signalrgb_app_dir() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    newest_app_dir(&local.join("VortxEngine")).filter(|d| d.is_dir())
}

/// `app-x.y.z` directory with the highest version under `root`.
fn newest_app_dir(root: &Path) -> Option<PathBuf> {
    let mut apps: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let version = name.strip_prefix("app-")?;
            let parts = version
                .split('.')
                .map(|p| p.parse().ok())
                .collect::<Option<Vec<u64>>>()?;
            Some((parts, e.path()))
        })
        .collect();
    apps.sort();
    apps.pop().map(|(_, p)| p)
}

/// Where the plugin index cache lives by default: `%LOCALAPPDATA%\lightloom\cache`
/// on Windows, `$XDG_CACHE_HOME/lightloom` (or `~/.cache/lightloom`) elsewhere.
pub fn default_cache_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("lightloom").join("cache"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .map(|d| d.join("lightloom"))
    };
    base.map(|d| d.join("plugin-index.json"))
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
        let gpu = catalog
            .smbus_plugins()
            .find(|e| e.meta.name == "MSI GPU")
            .expect("MSI GPU plugin catalogued");
        assert_eq!(gpu.meta.smbus_addresses, vec![0x68]);
        assert_eq!(gpu.meta.gpu_ids().len(), 1);
        let ram = catalog
            .smbus_plugins()
            .find(|e| e.meta.name == "Crucial Ballistix DRAM")
            .expect("Crucial plugin catalogued");
        assert_eq!(
            ram.meta.smbus_addresses,
            vec![0x20, 0x21, 0x22, 0x23, 0x27, 0x39, 0x3A, 0x3B, 0x3C]
        );
        assert!(ram.meta.gpu_ids().is_empty());
        assert_eq!(ram.meta.size, [8, 1]);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lightloom-catalog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plugin_source(pid: u16) -> String {
        format!(
            "export function Name() {{ return 'P {pid:#06x}'; }}\n\
             export function VendorId() {{ return 0x1234; }}\n\
             export function ProductId() {{ return {pid}; }}\n\
             export function Initialize() {{}}\n\
             export function Render() {{}}\n\
             export function Shutdown() {{}}\n"
        )
    }

    #[test]
    fn the_index_cache_reloads_only_changed_files() {
        let dir = scratch("cache");
        let plugins = dir.join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join("a.js"), plugin_source(1)).unwrap();
        std::fs::write(plugins.join("b.js"), plugin_source(2)).unwrap();
        std::fs::write(plugins.join("helper.js"), "export const x = 1;\n").unwrap();
        let opts = ScanOptions {
            cache: Some(dir.join("index.json")),
            jobs: 2,
            ..ScanOptions::default()
        };
        let first = Catalog::scan_with(std::slice::from_ref(&plugins), &opts);
        assert_eq!(
            first.stats(),
            ScanStats {
                files: 3,
                cached: 0,
                loaded: 3
            }
        );
        assert_eq!(first.entries().len(), 2);
        assert_eq!(first.skipped().len(), 1, "helper module");

        let second = Catalog::scan_with(std::slice::from_ref(&plugins), &opts);
        assert_eq!(
            second.stats(),
            ScanStats {
                files: 3,
                cached: 3,
                loaded: 0
            }
        );
        assert_eq!(second.entries().len(), 2);
        assert!(second.lookup(0x1234, 2).is_some());

        // A changed file (new size) is loaded again; the others stay cached.
        std::fs::write(plugins.join("b.js"), plugin_source(3) + "// changed\n").unwrap();
        let third = Catalog::scan_with(std::slice::from_ref(&plugins), &opts);
        assert_eq!(
            third.stats(),
            ScanStats {
                files: 3,
                cached: 2,
                loaded: 1
            }
        );
        assert!(third.lookup(0x1234, 2).is_none());
        assert!(third.lookup(0x1234, 3).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generic_plugins_are_found_by_hid_usage_after_vid_pid() {
        let dir = scratch("usage");
        std::fs::write(dir.join("specific.js"), plugin_source(7)).unwrap();
        std::fs::write(
            dir.join("generic.js"),
            "export function Name() { return 'Generic'; }\n\
             export function HidUsages() { return [[0x59, 0x01]]; }\n\
             export function Validate() { return true; }\n\
             export function Render() {}\n",
        )
        .unwrap();
        let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
        assert_eq!(catalog.entries().len(), 2, "{:?}", catalog.skipped());
        let endpoint = |page: u16, usage: u16| HidEndpointInfo {
            usage_page: page,
            usage,
            ..HidEndpointInfo::default()
        };
        let lamp_array = [endpoint(0x01, 0x02), endpoint(0x59, 0x01)];
        let generic = catalog.lookup_device(0x9999, 1, &lamp_array).unwrap();
        assert_eq!(generic.meta.name, "Generic");
        assert_eq!(generic.meta.hid_usages, vec![[0x59, 0x01]]);
        // A plugin that claims the device by VID:PID wins.
        assert_eq!(
            catalog.lookup_device(0x1234, 7, &lamp_array).unwrap().meta.name,
            "P 0x0007"
        );
        assert!(catalog
            .lookup_device(0x9999, 1, &[endpoint(0x01, 0x02)])
            .is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    const SMBUS_PLUGIN: &str = "export function Name() { return 'GPU'; }\n\
         export function Type() { return 'SMBUS'; }\n\
         export function BrandGPUList() { return [{ Vendor: 0x10DE, Device: 0x2504, SubVendor: 0x1462, SubDevice: 0x3903, Address: 0x68, Name: 'x' }]; }\n\
         export function Scan(bus) { return [0x68]; }\n\
         export function Initialize() {}\n\
         export function Render() {}\n";

    #[test]
    fn smbus_plugins_need_a_scan_window() {
        let dir = scratch("smbus-window");
        std::fs::write(
            dir.join("a.js"),
            format!("{SMBUS_PLUGIN}export function SmbusAddresses() {{ return [0x68]; }}\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join("b.js"),
            format!("{SMBUS_PLUGIN}export function SmbusAddresses() {{ return [0x02]; }}\n"),
        )
        .unwrap();
        let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
        assert_eq!(catalog.smbus_plugins().count(), 1, "{:?}", catalog.skipped());
        assert!(catalog.skipped()[0].1.contains("outside 0x03..=0x77"));
        assert!(
            catalog.index().is_empty(),
            "SMBus plugins are not keyed by VID:PID"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn signalrgb_style_smbus_plugin_is_skipped_with_reason() {
        let dir = scratch("smbus-signalrgb");
        std::fs::write(dir.join("MSI_GPU.js"), SMBUS_PLUGIN).unwrap();
        let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
        assert!(catalog.entries().is_empty());
        assert!(
            catalog.skipped()[0].1.contains("SmbusAddresses()"),
            "{:?}",
            catalog.skipped()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn later_directories_win_collisions() {
        let dir = scratch("order");
        let (first, second) = (dir.join("first"), dir.join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("x.js"), plugin_source(7)).unwrap();
        std::fs::write(second.join("y.js"), plugin_source(7)).unwrap();
        let catalog = Catalog::scan(&[first, second.clone()], Duration::from_secs(10));
        assert_eq!(catalog.lookup(0x1234, 7).unwrap().root, second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_dirs_sort_by_version_not_text() {
        let dir = scratch("apps");
        for v in ["app-2.5.9", "app-2.5.80", "app-2.4.120", "app-notaversion"] {
            std::fs::create_dir_all(dir.join(v)).unwrap();
        }
        assert_eq!(newest_app_dir(&dir), Some(dir.join("app-2.5.80")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
