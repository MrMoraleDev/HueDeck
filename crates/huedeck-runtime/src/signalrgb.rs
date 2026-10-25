// SPDX-License-Identifier: GPL-3.0-or-later
//! A local SignalRGB installation's setup, read in place: never copied into
//! HueDeck's files, never written. Everything here is empty on hosts without
//! SignalRGB (and on non-Windows hosts).
//!
//! Its settings tree is `HKCU\Software\WhirlwindFX\SignalRgb` (Qt `QSettings`),
//! read through a [`SettingsSource`] so tests use a [`Fixture`] instead of the
//! registry:
//!
//! * Channel components: `devices\<key>`, one JSON array value per channel.
//! * Canvas placements: `lighting\endpoint\<uid>` (`scale` as JSON, `enabled`,
//!   `rotation`, `flipped`, `flippedV`, `brightness`) with a `position` subkey
//!   (`x`, `y`); the layout brightness and grid under `layouts`; the global
//!   brightness as `lighting\globalBrightness`.
//! * Effect state: `effects\<id>` (`<file name>.html` for bundled effects, the
//!   catalog id for downloaded ones), one value per `<meta property>`; values
//!   whose name starts with `-` are SignalRGB's own bookkeeping. Subkeys of an
//!   effect are its saved presets (same value shape). `effects\selected\id` is
//!   the effect that was playing. Colours are `@Variant(...)` QColor streams.
//! * Effects and components: the newest `app-*` directory's `Signal-x64\Effects`
//!   and `Signal-x64\Components`, and the catalog cache.

use huedeck_engine::{ComponentInstance, Layout, Placement, Point, Scale};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// Read-only access to SignalRGB's settings tree. Paths are relative to
/// `HKCU\Software\WhirlwindFX\SignalRgb`, `\`-separated, `""` for the root. A
/// missing key reads as empty.
pub trait SettingsSource {
    /// The values directly under `path`, as text (numbers in decimal, Qt
    /// `@Variant(...)` strings with one character per byte).
    fn values(&self, path: &str) -> io::Result<BTreeMap<String, String>>;
    /// The names of the keys directly under `path`.
    fn subkeys(&self, path: &str) -> io::Result<Vec<String>>;
}

/// The registry of the current user (read-only).
#[cfg(windows)]
pub struct Registry;

#[cfg(windows)]
const ROOT: &str = r"Software\WhirlwindFX\SignalRgb";

#[cfg(windows)]
impl SettingsSource for Registry {
    fn values(&self, path: &str) -> io::Result<BTreeMap<String, String>> {
        let values = huedeck_hal_windows::registry::current_user_values(&registry_path(path))?;
        Ok(values.into_iter().map(|(n, v)| (n, v.as_text())).collect())
    }

    fn subkeys(&self, path: &str) -> io::Result<Vec<String>> {
        huedeck_hal_windows::registry::current_user_subkeys(&registry_path(path))
    }
}

#[cfg(windows)]
fn registry_path(path: &str) -> String {
    if path.is_empty() {
        ROOT.to_string()
    } else {
        format!(r"{ROOT}\{path}")
    }
}

/// No settings at all (hosts without SignalRGB's registry).
pub struct Empty;

impl SettingsSource for Empty {
    fn values(&self, _path: &str) -> io::Result<BTreeMap<String, String>> {
        Ok(BTreeMap::new())
    }

    fn subkeys(&self, _path: &str) -> io::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

/// This machine's SignalRGB settings: the registry on Windows, [`Empty`] elsewhere.
pub fn local() -> Box<dyn SettingsSource + Send + Sync> {
    #[cfg(windows)]
    {
        Box::new(Registry)
    }
    #[cfg(not(windows))]
    {
        Box::new(Empty)
    }
}

/// An in-memory settings tree (tests, dry runs): `{ "<path>": { "<name>": value } }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fixture {
    keys: BTreeMap<String, BTreeMap<String, String>>,
}

impl Fixture {
    /// An empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// A tree from JSON: an object of key paths, each an object of values
    /// (strings, numbers or booleans, stored as their text).
    pub fn from_json(doc: &Value) -> Result<Self, String> {
        let mut fixture = Self::new();
        let keys = doc.as_object().ok_or("a fixture is an object of key paths")?;
        for (path, values) in keys {
            let values = values
                .as_object()
                .ok_or_else(|| format!("{path}: values must be an object"))?;
            fixture.keys.entry(path.clone()).or_default();
            for (name, value) in values {
                let text = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(_) | Value::Bool(_) => value.to_string(),
                    _ => return Err(format!("{path}\\{name}: not a string, number or boolean")),
                };
                fixture.set(path, name, &text);
            }
        }
        Ok(fixture)
    }

    /// Set one value (creating its key).
    pub fn set(&mut self, path: &str, name: &str, value: &str) {
        self.keys
            .entry(path.to_string())
            .or_default()
            .insert(name.to_string(), value.to_string());
    }
}

impl SettingsSource for Fixture {
    fn values(&self, path: &str) -> io::Result<BTreeMap<String, String>> {
        Ok(self.keys.get(path).cloned().unwrap_or_default())
    }

    fn subkeys(&self, path: &str) -> io::Result<Vec<String>> {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!(r"{path}\")
        };
        let mut out: Vec<String> = self
            .keys
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|rest| !rest.is_empty())
            .map(|rest| rest.split('\\').next().unwrap_or(rest).to_string())
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
    }
}

/// SignalRGB's component setup of the device `key` (read-only): the channel
/// arrays under `HKCU\Software\WhirlwindFX\SignalRgb\devices\<key>`. Other
/// values there (plugin property values) are not component arrays and are
/// skipped. Returns the channels and notes about what was skipped.
pub fn channels(key: &str) -> (BTreeMap<String, Vec<ComponentInstance>>, Vec<String>) {
    channels_from(&*local(), key)
}

/// [`channels`] from `src`.
pub fn channels_from(
    src: &dyn SettingsSource,
    key: &str,
) -> (BTreeMap<String, Vec<ComponentInstance>>, Vec<String>) {
    let mut channels = BTreeMap::new();
    let mut notes = Vec::new();
    let values = match src.values(&format!(r"devices\{key}")) {
        Ok(v) => v,
        Err(e) => {
            notes.push(format!("cannot read SignalRGB's settings for {key}: {e}"));
            return (channels, notes);
        }
    };
    for (name, value) in values {
        if !value.trim_start().starts_with('[') {
            continue;
        }
        match huedeck_engine::parse_channel(&value) {
            Ok((instances, warnings)) if !instances.is_empty() || value.trim() == "[]" => {
                notes.extend(warnings.into_iter().map(|w| format!("{name}: {w}")));
                channels.insert(name, instances);
            }
            _ => {}
        }
    }
    (channels, notes)
}

/// One endpoint's placement from its registry values (`name -> text`) and its
/// `position` subkey values. Unknown values are ignored; malformed ones keep the
/// default and add a note.
pub fn placement_from_values(
    uid: &str,
    values: &BTreeMap<String, String>,
    position: &BTreeMap<String, String>,
    notes: &mut Vec<String>,
) -> Placement {
    let mut p = Placement::default();
    let num = |map: &BTreeMap<String, String>, name: &str, notes: &mut Vec<String>| -> Option<f32> {
        let raw = map.get(name)?;
        match raw.trim().parse::<f32>() {
            Ok(v) if v.is_finite() => Some(v),
            _ => {
                notes.push(format!("{uid}: {name} = {raw:?} is not a number"));
                None
            }
        }
    };
    if let (Some(x), Some(y)) = (num(position, "x", notes), num(position, "y", notes)) {
        p.position = Point { x, y };
    }
    if let Some(raw) = values.get("scale") {
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(v) => match (
                v.get("x").and_then(|x| x.as_f64()),
                v.get("y").and_then(|y| y.as_f64()),
            ) {
                (Some(x), Some(y)) if x > 0.0 && y > 0.0 && x.is_finite() && y.is_finite() => {
                    p.scale = Some(Scale {
                        x: x as f32,
                        y: y as f32,
                    });
                }
                _ => notes.push(format!("{uid}: scale {raw:?} has no positive x and y")),
            },
            Err(_) => notes.push(format!("{uid}: scale {raw:?} is not JSON")),
        }
    }
    if let Some(enabled) = flag(values, "enabled") {
        p.enabled = enabled;
    }
    if let Some(r) = num(values, "rotation", notes) {
        p.rotation = r.rem_euclid(360.0);
    }
    if let Some(f) = flag(values, "flipped") {
        p.flip_x = f;
    }
    if let Some(f) = flag(values, "flippedV") {
        p.flip_y = f;
    }
    if let Some(b) = num(values, "brightness", notes) {
        p.brightness = percent(b);
    }
    p
}

fn flag(map: &BTreeMap<String, String>, name: &str) -> Option<bool> {
    map.get(name)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1"))
}

fn percent(v: f32) -> u8 {
    v.clamp(0.0, 100.0).round() as u8
}

/// A `0..=100` value, if `raw` is a number.
fn percent_text(raw: Option<&String>) -> Option<u8> {
    raw.and_then(|b| b.trim().parse::<f32>().ok())
        .filter(|b| b.is_finite())
        .map(percent)
}

/// SignalRGB's canvas setup as a HueDeck layout named `name`: every endpoint
/// under `lighting\endpoint` and the layout brightness and grid. `None` when
/// SignalRGB has no endpoint settings on this machine.
pub fn layout(name: &str) -> (Option<Layout>, Vec<String>) {
    layout_from(&*local(), name)
}

/// [`layout`] from `src`.
pub fn layout_from(src: &dyn SettingsSource, name: &str) -> (Option<Layout>, Vec<String>) {
    let mut notes = Vec::new();
    let as_map = |path: &str, notes: &mut Vec<String>| -> BTreeMap<String, String> {
        src.values(path).unwrap_or_else(|e| {
            notes.push(format!("cannot read {path}: {e}"));
            BTreeMap::new()
        })
    };
    let endpoints_key = r"lighting\endpoint";
    let uids = match src.subkeys(endpoints_key) {
        Ok(u) => u,
        Err(e) => {
            notes.push(format!("cannot read SignalRGB's placements: {e}"));
            return (None, notes);
        }
    };
    if uids.is_empty() {
        return (None, notes);
    }
    let mut endpoints = BTreeMap::new();
    for uid in uids {
        // A stray `position` key sits next to the endpoints on some installations.
        if uid == "position" {
            continue;
        }
        let values = as_map(&format!(r"{endpoints_key}\{uid}"), &mut notes);
        let position = as_map(&format!(r"{endpoints_key}\{uid}\position"), &mut notes);
        let placement = placement_from_values(&uid, &values, &position, &mut notes);
        endpoints.insert(uid, placement);
    }
    let settings = as_map("layouts", &mut notes);
    let brightness = percent_text(settings.get("brightness")).unwrap_or(100);
    let grid_size = settings
        .get("gridSize")
        .and_then(|g| g.trim().parse::<u32>().ok())
        .unwrap_or(4);
    let layout = Layout {
        name: name.to_string(),
        brightness,
        grid_size,
        endpoints,
        ..Layout::default()
    };
    (Some(layout), notes)
}

/// The stored state of one SignalRGB effect.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct EffectSettings {
    /// Control values by `<meta property>` name (best-effort types: colours as
    /// `#rrggbb`, `true`/`false` as booleans, numeric text as numbers; the
    /// importer coerces them to the effect's control types when it knows them).
    pub params: Map<String, Value>,
    /// Saved presets by name.
    pub presets: BTreeMap<String, Map<String, Value>>,
}

/// One device's own settings (a `vid:pid:serial` endpoint).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSettings {
    /// `vid:pid:serial`.
    pub key: String,
    /// Shown on the canvas (`None`: not set, SignalRGB's default is on).
    pub enabled: Option<bool>,
    /// Device brightness, percent (`None`: not set, 100).
    pub brightness: Option<u8>,
}

/// Everything the import wizard takes from SignalRGB.
#[derive(Clone, Debug, Default)]
pub struct Setup {
    /// The canvas as a layout.
    pub layout: Option<Layout>,
    /// Component arrays by device key and channel.
    pub channels: BTreeMap<String, BTreeMap<String, Vec<ComponentInstance>>>,
    /// Effect state by HueDeck effect id (`signalrgb:<name>`).
    pub effects: BTreeMap<String, EffectSettings>,
    /// The effect that was playing (HueDeck id).
    pub selected_effect: Option<String>,
    /// Global brightness, percent (`None`: not set).
    pub global_brightness: Option<u8>,
    /// Per-device enable and brightness.
    pub devices: Vec<DeviceSettings>,
    /// What was skipped and why.
    pub notes: Vec<String>,
}

/// HueDeck's id of the SignalRGB effect stored under `effects\<key>`: the file
/// stem of a bundled effect (`Side To Side.html`), or the catalog id.
pub fn effect_id(key: &str) -> String {
    let name = match key.len().checked_sub(5) {
        Some(at) if key.is_char_boundary(at) && key[at..].eq_ignore_ascii_case(".html") && at > 0 => {
            &key[..at]
        }
        _ => key,
    };
    format!("signalrgb:{name}")
}

/// `vid:pid:serial` (a device, not a subdevice or a component instance).
fn is_device_key(uid: &str) -> bool {
    let parts: Vec<&str> = uid.split(':').collect();
    parts.len() == 3
        && parts[..2]
            .iter()
            .all(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Read SignalRGB's whole setup (read-only); the layout is named `layout_name`.
pub fn read_setup(src: &dyn SettingsSource, layout_name: &str) -> Setup {
    let (layout, mut notes) = layout_from(src, layout_name);
    let mut setup = Setup::default();

    for key in src.subkeys("devices").unwrap_or_default() {
        let (channels, channel_notes) = channels_from(src, &key);
        notes.extend(channel_notes);
        if !channels.is_empty() {
            setup.channels.insert(key, channels);
        }
    }

    if let Ok(uids) = src.subkeys(r"lighting\endpoint") {
        for uid in uids.into_iter().filter(|u| is_device_key(u)) {
            let values = src
                .values(&format!(r"lighting\endpoint\{uid}"))
                .unwrap_or_default();
            let enabled = flag(&values, "enabled");
            let brightness = percent_text(values.get("brightness"));
            if enabled.is_some() || brightness.is_some() {
                setup.devices.push(DeviceSettings {
                    key: uid,
                    enabled,
                    brightness,
                });
            }
        }
    }

    let lighting = src.values("lighting").unwrap_or_default();
    setup.global_brightness = percent_text(
        lighting
            .get("globalBrightness")
            .or_else(|| lighting.get("global_brightness")),
    );

    for key in src.subkeys("effects").unwrap_or_default() {
        let path = format!(r"effects\{key}");
        if key == "selected" {
            let values = src.values(&path).unwrap_or_default();
            setup.selected_effect = values
                .get("id")
                .map(|id| id.trim())
                .filter(|id| !id.is_empty())
                .map(effect_id);
            continue;
        }
        let id = effect_id(&key);
        let params = effect_values(&src.values(&path).unwrap_or_default(), &id, &mut notes);
        let mut presets = BTreeMap::new();
        for preset in src.subkeys(&path).unwrap_or_default() {
            let values = src.values(&format!(r"{path}\{preset}")).unwrap_or_default();
            let values = effect_values(&values, &format!("{id} preset {preset}"), &mut notes);
            if !values.is_empty() {
                presets.insert(preset, values);
            }
        }
        if !params.is_empty() || !presets.is_empty() {
            setup.effects.insert(id, EffectSettings { params, presets });
        }
    }

    setup.layout = layout;
    setup.notes = notes;
    setup
}

/// An effect key's control values (SignalRGB's `-` bookkeeping values left out).
fn effect_values(
    values: &BTreeMap<String, String>,
    what: &str,
    notes: &mut Vec<String>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, raw) in values {
        if name.starts_with('-') {
            continue;
        }
        match setting_value(raw) {
            Some(v) => {
                out.insert(name.clone(), v);
            }
            None => notes.push(format!("{what}: {name} has a value HueDeck cannot read")),
        }
    }
    out
}

/// One stored control value as JSON: a QColor `@Variant` as `#rrggbb`,
/// `true`/`false` as booleans, numeric text as a number, other text as is.
/// `None` for other Qt variant types.
pub fn setting_value(raw: &str) -> Option<Value> {
    if raw.starts_with("@Variant(") {
        return qcolor(raw).map(Value::String);
    }
    if raw.starts_with("@ByteArray(") || raw.starts_with("@Invalid(") {
        return None;
    }
    let t = raw.trim();
    match t {
        "true" => return Some(Value::Bool(true)),
        "false" => return Some(Value::Bool(false)),
        _ => {}
    }
    if let Ok(n) = t.parse::<i64>() {
        return Some(Value::from(n));
    }
    if let Some(n) = t
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .and_then(serde_json::Number::from_f64)
    {
        return Some(Value::Number(n));
    }
    Some(Value::String(raw.to_string()))
}

/// A Qt `QSettings` QColor (`@Variant(` + a `QDataStream` of type 67 (Qt 5) or
/// 0x1003 (Qt 6), one character per byte + `)`) as `#rrggbb`. RGB, HSV, CMYK
/// and HSL specs are read; the alpha channel is dropped.
pub fn qcolor(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix("@Variant(")?.strip_suffix(')')?;
    let bytes: Vec<u8> = inner
        .chars()
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect::<Option<_>>()?;
    if bytes.len() < 4 {
        return None;
    }
    let ty = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if ty != 67 && ty != 0x1003 {
        return None;
    }
    let mut rest = &bytes[4..];
    // Newer streams put an "is null" byte after the type.
    if rest.len() == 12 {
        rest = &rest[1..];
    }
    if rest.len() != 11 {
        return None;
    }
    let word = |i: usize| f64::from(u16::from_be_bytes([rest[1 + 2 * i], rest[2 + 2 * i]]));
    let unit = |i: usize| word(i) / 65535.0;
    // Hue in hundredths of a degree; 0xFFFF = achromatic.
    let hue = |i: usize| if word(i) >= 65535.0 { 0.0 } else { word(i) / 100.0 };
    let (r, g, b) = match rest[0] {
        1 => (unit(1), unit(2), unit(3)),
        2 => hsv_to_rgb(hue(1), unit(2), unit(3)),
        3 => {
            let k = unit(4);
            (
                (1.0 - unit(1)) * (1.0 - k),
                (1.0 - unit(2)) * (1.0 - k),
                (1.0 - unit(3)) * (1.0 - k),
            )
        }
        4 => hsl_to_rgb(hue(1), unit(2), unit(3)),
        _ => return None,
    };
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b)))
}

/// `(r, g, b)` in `0..=1` from a hue in degrees and a chroma; `m` is added to each.
fn hue_to_rgb(h: f64, c: f64, m: f64) -> (f64, f64, f64) {
    let h = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    hue_to_rgb(h, c, l - c / 2.0)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let c = v * s;
    hue_to_rgb(h, c, v - c)
}

/// Effect directories of the local SignalRGB installation that exist:
/// `(directory, recursive)`. The bundled `Signal-x64\Effects` has one
/// subdirectory per category; the catalog cache has one directory per effect
/// holding `effect.html`.
pub fn effect_dirs() -> Vec<(PathBuf, bool)> {
    let mut out = Vec::new();
    if let Some(app) = huedeck_plugin_host::catalog::signalrgb_app_dir() {
        out.push((app.join("Signal-x64").join("Effects"), true));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        out.push((
            local
                .join("WhirlwindFX")
                .join("SignalRgb")
                .join("cache")
                .join("effects"),
            true,
        ));
    }
    out.retain(|(d, _)| d.is_dir());
    out
}

/// Component directories of the local SignalRGB installation that exist (bundled
/// and catalog cache).
pub fn component_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(app) = huedeck_plugin_host::catalog::signalrgb_app_dir() {
        out.push(app.join("Signal-x64").join("Components"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        out.push(
            local
                .join("WhirlwindFX")
                .join("SignalRgb")
                .join("cache")
                .join("plugin_cdn")
                .join("main")
                .join("Components"),
        );
    }
    out.retain(|d| d.is_dir());
    out
}

/// The local SignalRGB installation's plugin, effect and component directories
/// that exist: files below them are third-party, read in place.
pub fn third_party_dirs() -> Vec<PathBuf> {
    let mut dirs = huedeck_plugin_host::catalog::signalrgb_plugin_dirs();
    dirs.extend(effect_dirs().into_iter().map(|(d, _)| d));
    dirs.extend(component_dirs());
    dirs
}

/// Whether `path` is below one of `dirs` (compared as given, no file access).
pub fn is_inside(path: &Path, dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|d| path.starts_with(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// A QColor `@Variant` as QSettings stores it: one character per stream byte.
    fn variant(bytes: &[u8]) -> String {
        let mut s = String::from("@Variant(");
        s.extend(bytes.iter().map(|&b| char::from(b)));
        s.push(')');
        s
    }

    #[test]
    fn placements_map_signalrgb_values() {
        let mut notes = Vec::new();
        let p = placement_from_values(
            "1462:7c91:X",
            &map(&[
                ("scale", r#"{"x":38.85714340209961,"y":38.85714340209961}"#),
                ("enabled", "false"),
                ("rotation", "450"),
                ("flipped", "true"),
                ("brightness", "55"),
            ]),
            &map(&[("x", "24"), ("y", "61")]),
            &mut notes,
        );
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((p.position.x, p.position.y), (24.0, 61.0));
        assert!((p.scale.unwrap().x - 38.857143).abs() < 1e-4);
        assert!(!p.enabled && p.flip_x && !p.flip_y);
        assert_eq!((p.rotation, p.brightness), (90.0, 55));
    }

    #[test]
    fn the_local_installation_reads_without_errors() {
        // Empty on machines without SignalRGB (CI); the reference machine has its setup.
        // Read-only: nothing here writes the registry.
        let setup = read_setup(&*local(), "SignalRGB");
        if let Some(l) = &setup.layout {
            eprintln!(
                "SignalRGB layout: {} endpoints, brightness {}; {} effects, selected {:?}, {} devices; notes {:?}",
                l.endpoints.len(),
                l.brightness,
                setup.effects.len(),
                setup.selected_effect,
                setup.devices.len(),
                setup.notes
            );
            assert!(l.endpoints.keys().all(|k| k != "position"));
        }
        eprintln!(
            "effect dirs {:?}, component dirs {:?}",
            effect_dirs(),
            component_dirs()
        );
    }

    #[test]
    fn malformed_values_keep_defaults_and_are_noted() {
        let mut notes = Vec::new();
        let p = placement_from_values(
            "u",
            &map(&[("scale", "{bad"), ("rotation", "x")]),
            &map(&[("x", "1")]),
            &mut notes,
        );
        assert_eq!(p, Placement::default());
        assert_eq!(notes.len(), 2, "{notes:?}");
    }

    #[test]
    fn qcolors_decode_in_every_spec() {
        // HSL red, as SignalRGB stores a colour picked at hue 0.
        let red = variant(&[0, 0, 0, 0x43, 4, 0xff, 0xff, 0, 0, 0xff, 0xff, 0x80, 0, 0, 0]);
        assert_eq!(qcolor(&red).as_deref(), Some("#ff0000"));
        // HSL green (hue 120.00 degrees).
        let green = variant(&[
            0, 0, 0, 0x43, 4, 0xff, 0xff, 0x2e, 0xe0, 0xff, 0xff, 0x80, 0, 0, 0,
        ]);
        assert_eq!(qcolor(&green).as_deref(), Some("#00ff00"));
        // HSL achromatic white.
        let white = variant(&[0, 0, 0, 0x43, 4, 0xff, 0xff, 0xff, 0xff, 0, 0, 0xff, 0xff, 0, 0]);
        assert_eq!(qcolor(&white).as_deref(), Some("#ffffff"));
        // RGB spec, and a Qt 6 stream with its null flag.
        let rgb = variant(&[
            0, 0, 0x10, 0x03, 0, 1, 0xff, 0xff, 0x12, 0x12, 0x34, 0x34, 0x56, 0x56, 0, 0,
        ]);
        assert_eq!(qcolor(&rgb).as_deref(), Some("#123456"));
        // HSV blue (hue 240.00 degrees).
        let blue = variant(&[
            0, 0, 0, 0x43, 2, 0xff, 0xff, 0x5d, 0xc0, 0xff, 0xff, 0xff, 0xff, 0, 0,
        ]);
        assert_eq!(qcolor(&blue).as_deref(), Some("#0000ff"));
        // Not a colour.
        assert_eq!(qcolor(&variant(&[0, 0, 0, 2, 0, 0, 0, 1])), None);
        assert_eq!(qcolor("@Variant(\u{1234})"), None);
    }

    #[test]
    fn setting_values_take_their_likely_type() {
        assert_eq!(setting_value("true"), Some(Value::Bool(true)));
        assert_eq!(setting_value("35"), Some(Value::from(35)));
        assert_eq!(setting_value("0.5"), Some(Value::from(0.5)));
        assert_eq!(
            setting_value("All Directions"),
            Some(Value::from("All Directions"))
        );
        assert_eq!(setting_value("@ByteArray(x)"), None);
    }

    #[test]
    fn effect_ids_follow_the_library() {
        assert_eq!(effect_id("Side To Side.html"), "signalrgb:Side To Side");
        assert_eq!(
            effect_id("-MTwqhnV_LPbIrkfZOfo"),
            "signalrgb:-MTwqhnV_LPbIrkfZOfo"
        );
        assert_eq!(effect_id(".html"), "signalrgb:.html");
        assert!(is_device_key("1462:7c91:A020200512A3"));
        assert!(!is_device_key("1462:7c91:A020200512A3:JRainbow 1"));
        assert!(!is_device_key("8d3978fc-c8c7-40a8-af61-cb99b5f47c0b"));
    }

    #[test]
    fn a_whole_setup_reads_from_a_fixture() {
        let doc: Value =
            serde_json::from_str(include_str!("../tests/fixtures/signalrgb-settings.json")).unwrap();
        let src = Fixture::from_json(&doc).unwrap();
        let setup = read_setup(&src, "Imported");

        let layout = setup.layout.expect("layout");
        assert_eq!(
            (layout.name.as_str(), layout.brightness, layout.grid_size),
            ("Imported", 80, 4)
        );
        assert_eq!(layout.endpoints.len(), 3, "{:?}", layout.endpoints.keys());
        assert!(!layout.endpoints["1532:007d:000000000000"].enabled);

        assert_eq!(setup.global_brightness, Some(65));
        assert_eq!(setup.selected_effect.as_deref(), Some("signalrgb:Side To Side"));
        assert_eq!(
            setup.devices,
            vec![
                DeviceSettings {
                    key: "1462:7c91:A020200512A3".into(),
                    enabled: None,
                    brightness: Some(40),
                },
                DeviceSettings {
                    key: "1532:007d:000000000000".into(),
                    enabled: Some(false),
                    brightness: None,
                },
            ]
        );

        let side = &setup.effects["signalrgb:Side To Side"];
        assert_eq!(side.params["direction"], "All Directions");
        assert_eq!(side.params["speed"], 2);
        assert_eq!(side.params["rainbow"], false);
        assert_eq!(side.params["color1"], "#ff0000");
        assert_eq!(side.presets["savedStateA"]["color1"], "#00ff00");
        let catalog = &setup.effects["signalrgb:-MTwqhnV_LPbIrkfZOfo"];
        assert!(!catalog.params.contains_key("-RemoteFileByteSize"));
        assert_eq!(catalog.params["color1"], "#ffffff");

        assert_eq!(setup.channels.len(), 1);
        let channels = &setup.channels["1462:7c91:A020200512A3"];
        assert_eq!(channels["JRainbow 1"].len(), 1);
        assert!(
            setup
                .notes
                .iter()
                .any(|n| n.contains("blob has a value HueDeck cannot read")),
            "{:?}",
            setup.notes
        );
    }

    #[test]
    fn fixture_subkeys_include_intermediate_keys() {
        let mut f = Fixture::new();
        f.set(r"a\b\c", "x", "1");
        f.set(r"a\b c", "x", "1");
        f.set(r"a\d", "y", "2");
        assert_eq!(f.subkeys("a").unwrap(), vec!["b", "b c", "d"]);
        assert_eq!(f.subkeys("").unwrap(), vec!["a"]);
        assert!(f.values("a").unwrap().is_empty());
    }
}
