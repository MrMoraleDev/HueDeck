// SPDX-License-Identifier: GPL-3.0-or-later
//! Live hardware modes (Lightloom extension, docs/host-api/hardware-modes.md): the
//! device's own built-in effects (a GPU's colour cycle, a board's static zones), run
//! by the controller instead of frames from the canvas.
//!
//! A plugin declares them with `HardwareModes()` (`[{id, name, colors, speed,
//! brightness}]`) and carries one out with `SetHardwareMode(request)`:
//!
//! * `request = {id, colors: [[r, g, b], ...], speed, brightness}` puts the device in
//!   that mode with **live** commands only: never a save-to-flash register or byte
//!   (the host keeps the SMBus `SmbusDeniedRegisters()` refused, as at any time
//!   outside `SaveToDevice`). The mode is gone after a power cycle; "Save to
//!   devices" stays the only way to store lighting.
//! * `request = null` ("Follow effect") puts the device back in its streaming mode.
//!
//! While a device is in a hardware mode its session stops calling `Render()`: the
//! canvas no longer reaches it until "Follow effect".

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Most colours a mode may take.
pub const MAX_COLORS: u8 = 8;
/// Longest mode id and name.
const MAX_TEXT: usize = 64;
/// `speed` / `brightness` when the request leaves them out.
pub const DEFAULT_SPEED: u8 = 50;
/// See [`DEFAULT_SPEED`].
pub const DEFAULT_BRIGHTNESS: u8 = 100;
/// A colour input's value when the request leaves it out.
pub const DEFAULT_COLOR: &str = "#ffffff";

/// One entry of `HardwareModes()`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareModeInfo {
    /// What `SetHardwareMode` gets as `request.id`.
    pub id: String,
    /// Shown name.
    pub name: String,
    /// Colours the mode takes (0 to [`MAX_COLORS`]).
    pub colors: u8,
    /// Names of those colours (`["Onboard", "JRGB1"]`); empty: "Colour 1", ...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub color_names: Vec<String>,
    /// It has a speed (`request.speed`, 0-100).
    pub speed: bool,
    /// It has a brightness (`request.brightness`, 0-100).
    pub brightness: bool,
}

/// Read `HardwareModes()`'s value; entries that do not fit are dropped with a warning.
pub fn modes_from_json(v: &Value, warnings: &mut Vec<String>) -> Vec<HardwareModeInfo> {
    let mut out: Vec<HardwareModeInfo> = Vec::new();
    let Some(list) = v.as_array() else {
        warnings.push("HardwareModes() must return an array".into());
        return out;
    };
    for entry in list {
        let Some(o) = entry.as_object() else {
            warnings.push(format!("HardwareModes(): not an object: {entry}"));
            continue;
        };
        let text = |k: &str| {
            o.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= MAX_TEXT)
                .map(String::from)
        };
        let Some(id) = text("id") else {
            warnings.push(format!(
                "HardwareModes(): an entry needs an id (1-{MAX_TEXT} characters): {entry}"
            ));
            continue;
        };
        if out.iter().any(|m| m.id == id) {
            warnings.push(format!("HardwareModes(): id {id:?} given twice"));
            continue;
        }
        let colors = match o.get("colors") {
            None | Some(Value::Null) => 0,
            Some(c) => match c.as_u64().and_then(|n| u8::try_from(n).ok()) {
                Some(n) if n <= MAX_COLORS => n,
                _ => {
                    warnings.push(format!("HardwareModes(): {id}: colors must be 0-{MAX_COLORS}"));
                    continue;
                }
            },
        };
        let color_names: Vec<String> = o
            .get("colorNames")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default();
        let flag = |k: &str| o.get(k).and_then(Value::as_bool).unwrap_or(false);
        out.push(HardwareModeInfo {
            name: text("name").unwrap_or_else(|| id.clone()),
            id,
            colors,
            color_names: if color_names.len() == usize::from(colors) {
                color_names
            } else {
                Vec::new()
            },
            speed: flag("speed"),
            brightness: flag("brightness"),
        });
    }
    out
}

/// A hardware mode chosen for a device, as clients send it and the daemon keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareModeChoice {
    /// One of the plugin's mode ids.
    pub id: String,
    /// `#rrggbb` colours, one per colour of the mode.
    #[serde(default)]
    pub colors: Vec<String>,
    /// 0-100, for a mode with a speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u8>,
    /// 0-100, for a mode with a brightness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(digits, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl HardwareModeChoice {
    /// This choice checked against `modes` and completed: the colours the mode
    /// takes (missing ones [`DEFAULT_COLOR`], lower-case), a speed and brightness
    /// only where the mode has them (defaults when left out). The error says why not.
    pub fn normalized(&self, modes: &[HardwareModeInfo]) -> Result<Self, String> {
        let Some(mode) = modes.iter().find(|m| m.id == self.id) else {
            let known: Vec<&str> = modes.iter().map(|m| m.id.as_str()).collect();
            return Err(if known.is_empty() {
                "this device has no hardware modes".into()
            } else {
                format!(
                    "no hardware mode {:?} (this device has {})",
                    self.id,
                    known.join(", ")
                )
            });
        };
        if self.colors.len() > usize::from(mode.colors) {
            return Err(format!(
                "{} takes {} colour(s), {} given",
                mode.name,
                mode.colors,
                self.colors.len()
            ));
        }
        let mut colors = Vec::new();
        for i in 0..usize::from(mode.colors) {
            let c = self.colors.get(i).map_or(DEFAULT_COLOR, String::as_str);
            if parse_hex(c).is_none() {
                return Err(format!("colour {} must be #rrggbb (got {c:?})", i + 1));
            }
            colors.push(c.to_ascii_lowercase());
        }
        let percent = |what: &str, v: Option<u8>, default: u8| match v {
            Some(v) if v > 100 => Err(format!("{what} must be 0-100 (got {v})")),
            Some(v) => Ok(v),
            None => Ok(default),
        };
        Ok(Self {
            id: mode.id.clone(),
            colors,
            speed: if mode.speed {
                Some(percent("speed", self.speed, DEFAULT_SPEED)?)
            } else {
                None
            },
            brightness: if mode.brightness {
                Some(percent("brightness", self.brightness, DEFAULT_BRIGHTNESS)?)
            } else {
                None
            },
        })
    }

    /// The argument of `SetHardwareMode(request)` (call it on a [`Self::normalized`] choice).
    pub fn request_json(&self) -> Value {
        json!({
            "id": self.id,
            "colors": self
                .colors
                .iter()
                .map(|c| {
                    let [r, g, b] = parse_hex(c).unwrap_or([255, 255, 255]);
                    json!([r, g, b])
                })
                .collect::<Vec<_>>(),
            "speed": self.speed,
            "brightness": self.brightness,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes() -> Vec<HardwareModeInfo> {
        let mut w = Vec::new();
        let m = modes_from_json(
            &json!([
                {"id": "rainbow", "name": "Rainbow", "colors": 0, "speed": true, "brightness": true},
                {"id": "static", "name": "Static", "colors": 2, "colorNames": ["A", "B"], "brightness": true},
                {"id": "rainbow"},
                {"name": "no id"},
                {"id": "many", "colors": 9},
                "nope"
            ]),
            &mut w,
        );
        assert_eq!(w.len(), 4, "{w:?}");
        m
    }

    #[test]
    fn declarations_are_read_strictly() {
        let m = modes();
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].id.as_str(), m[0].colors, m[0].speed), ("rainbow", 0, true));
        assert_eq!(m[1].color_names, ["A", "B"]);
        let mut w = Vec::new();
        assert!(modes_from_json(&json!({"id": "x"}), &mut w).is_empty());
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn choices_are_checked_and_completed() {
        let m = modes();
        let c = HardwareModeChoice {
            id: "static".into(),
            colors: vec!["#FF0000".into()],
            speed: Some(10),
            brightness: None,
        }
        .normalized(&m)
        .unwrap();
        assert_eq!(c.colors, ["#ff0000", DEFAULT_COLOR]);
        assert_eq!((c.speed, c.brightness), (None, Some(100)));
        assert_eq!(
            c.request_json(),
            json!({"id": "static", "colors": [[255, 0, 0], [255, 255, 255]], "speed": null, "brightness": 100})
        );
        let bad = |c: HardwareModeChoice| c.normalized(&m).unwrap_err();
        assert!(bad(HardwareModeChoice {
            id: "disco".into(),
            ..Default::default()
        })
        .contains("rainbow, static"));
        assert!(bad(HardwareModeChoice {
            id: "rainbow".into(),
            colors: vec!["#000000".into()],
            ..Default::default()
        })
        .contains("0 colour"));
        assert!(bad(HardwareModeChoice {
            id: "static".into(),
            colors: vec!["red".into()],
            ..Default::default()
        })
        .contains("#rrggbb"));
        assert!(bad(HardwareModeChoice {
            id: "rainbow".into(),
            speed: Some(101),
            ..Default::default()
        })
        .contains("speed"));
        assert!(HardwareModeChoice::default()
            .normalized(&[])
            .unwrap_err()
            .contains("no hardware modes"));
    }
}
