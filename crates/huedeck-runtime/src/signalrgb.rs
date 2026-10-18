// SPDX-License-Identifier: Apache-2.0 OR MIT
//! A local SignalRGB installation's setup, read in place: never copied into
//! HueDeck's files, never written. Everything here is empty on hosts without
//! SignalRGB (and on non-Windows hosts).
//!
//! * Channel components: `HKCU\Software\WhirlwindFX\SignalRgb\devices\<key>`, one
//!   JSON array value per channel.
//! * Canvas placements: `HKCU\Software\WhirlwindFX\SignalRgb\lighting\endpoint\<uid>`
//!   (`scale` as JSON, `enabled`, `rotation`, `flipped`, `flippedV`, `brightness`)
//!   with a `position` subkey (`x`, `y`); the layout brightness and grid under
//!   `...\layouts`.
//! * Effects and components: the newest `app-*` directory's `Signal-x64\Effects`
//!   and `Signal-x64\Components`, and the catalog cache.

use huedeck_engine::{ComponentInstance, Layout, Placement, Point, Scale};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[cfg(windows)]
const ROOT: &str = r"Software\WhirlwindFX\SignalRgb";

/// SignalRGB's component setup of the device `key` (read-only): the channel
/// arrays under `HKCU\Software\WhirlwindFX\SignalRgb\devices\<key>`. Other
/// values there (plugin property values) are not component arrays and are
/// skipped. Returns the channels and notes about what was skipped.
#[cfg(windows)]
pub fn channels(key: &str) -> (BTreeMap<String, Vec<ComponentInstance>>, Vec<String>) {
    let mut channels = BTreeMap::new();
    let mut notes = Vec::new();
    let subkey = format!(r"{ROOT}\devices\{key}");
    let values = match huedeck_hal_windows::registry::current_user_strings(&subkey) {
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

/// No SignalRGB on this OS.
#[cfg(not(windows))]
pub fn channels(_key: &str) -> (BTreeMap<String, Vec<ComponentInstance>>, Vec<String>) {
    (BTreeMap::new(), Vec::new())
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
    let flag = |map: &BTreeMap<String, String>, name: &str| -> Option<bool> {
        map.get(name)
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1"))
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
        p.brightness = b.clamp(0.0, 100.0).round() as u8;
    }
    p
}

/// SignalRGB's canvas setup as a HueDeck layout named `name`: every endpoint
/// under `lighting\endpoint` and the layout brightness and grid. `None` when
/// SignalRGB has no endpoint settings on this machine.
#[cfg(windows)]
pub fn layout(name: &str) -> (Option<Layout>, Vec<String>) {
    use huedeck_hal_windows::registry::{current_user_subkeys, current_user_values};
    let mut notes = Vec::new();
    let as_map = |subkey: &str, notes: &mut Vec<String>| -> BTreeMap<String, String> {
        match current_user_values(subkey) {
            Ok(values) => values.into_iter().map(|(n, v)| (n, v.as_text())).collect(),
            Err(e) => {
                notes.push(format!("cannot read {subkey}: {e}"));
                BTreeMap::new()
            }
        }
    };
    let endpoints_key = format!(r"{ROOT}\lighting\endpoint");
    let uids = match current_user_subkeys(&endpoints_key) {
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
    let settings = as_map(&format!(r"{ROOT}\layouts"), &mut notes);
    let brightness = settings
        .get("brightness")
        .and_then(|b| b.trim().parse::<f32>().ok())
        .map_or(100, |b| b.clamp(0.0, 100.0).round() as u8);
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

/// No SignalRGB on this OS.
#[cfg(not(windows))]
pub fn layout(_name: &str) -> (Option<Layout>, Vec<String>) {
    (None, Vec::new())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
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
        // Empty on machines without SignalRGB (CI); the reference machine has its placements.
        let (layout, notes) = layout("SignalRGB");
        if let Some(l) = &layout {
            eprintln!(
                "SignalRGB layout: {} endpoints, brightness {}, notes {notes:?}",
                l.endpoints.len(),
                l.brightness
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
}
