// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Components: the fans, strips and other addressable devices plugged into a
//! controller channel (docs/host-api/components-channels.md §3-4).
//!
//! * [`Component`] is a component descriptor: SignalRGB's component JSON
//!   (PascalCase keys), so existing descriptor files load unchanged.
//! * [`ComponentInstance`] is one component assigned to a channel: the
//!   descriptor plus a `ComponentId` (UUID), which is also the instance's
//!   canvas endpoint uid. Its JSON form is the one SignalRGB persists per
//!   channel (`devices\<vid:pid:serial>\<channel>` = array of instances).
//! * [`ComponentLibrary`] loads descriptor files; a later directory overrides
//!   an earlier one by `ProductName` (user components over built-in ones).
//!
//! Colour stream of an instance: LED `i` sits at `LedCoordinates[i]` in the
//! `Width x Height` footprint, and its colour goes to slot `LedMapping[i]` of
//! the instance's stream (`stream[LedMapping[i]] = sample(LedCoordinates[i])`,
//! [`Component::stream_slots`]). A channel's stream is the concatenation of
//! its instances in chain order.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Most LEDs one component may declare (the largest device budget in the corpus is 8192).
pub const MAX_COMPONENT_LEDS: u32 = 8192;
/// Largest footprint side, in cells.
pub const MAX_COMPONENT_SIDE: u32 = 8192;
/// Largest footprint, in cells.
pub const MAX_COMPONENT_CELLS: u64 = 1 << 20;
/// Largest descriptor file read (built-in ones embedding a base64 image stay far below).
pub const MAX_COMPONENT_FILE: u64 = 4 << 20;
/// `ProductName` prefix of the placeholder strip a channel gets when nothing is assigned.
pub const DEFAULT_STRIP_PREFIX: &str = "Default Strip - ";
/// JSON key of an instance's id.
pub const COMPONENT_ID_KEY: &str = "ComponentId";

/// A component descriptor (SignalRGB component JSON).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Component {
    /// Identity and override key.
    pub product_name: String,
    /// Label shown to the user.
    #[serde(default)]
    pub display_name: String,
    /// Picker filter (`Custom` for generated strips).
    #[serde(default)]
    pub brand: String,
    /// Picker filter (`Fan`, `Strip`, `AIO`, ...; free text).
    #[serde(rename = "Type", default)]
    pub kind: String,
    /// Number of LEDs.
    pub led_count: u32,
    /// Footprint width in cells.
    pub width: u32,
    /// Footprint height in cells.
    pub height: u32,
    /// Stream slot of each LED.
    pub led_mapping: Vec<u32>,
    /// Footprint cell of each LED.
    pub led_coordinates: Vec<[u32; 2]>,
    /// Name of each LED.
    #[serde(default)]
    pub led_names: Vec<String>,
    /// Product image URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    /// Product image, base64 PNG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl Component {
    /// The placeholder strip of `n` LEDs: `n x 1`, identity mapping, `Led 1..n`.
    pub fn default_strip(n: u32) -> Self {
        let name = format!("{DEFAULT_STRIP_PREFIX}{n}");
        Self::strip(name, n)
    }

    /// A plain `n x 1` strip (`Brand` "Custom", `Type` "Strip").
    pub fn strip(name: String, n: u32) -> Self {
        Self {
            product_name: name.clone(),
            display_name: name,
            brand: "Custom".into(),
            kind: "Strip".into(),
            led_count: n,
            width: n,
            height: 1,
            led_mapping: (0..n).collect(),
            led_coordinates: (0..n).map(|i| [i, 0]).collect(),
            led_names: (1..=n).map(|i| format!("Led {i}")).collect(),
            image_url: None,
            image: None,
        }
    }

    /// Whether this is a channel's placeholder strip.
    pub fn is_default_strip(&self) -> bool {
        self.product_name.starts_with(DEFAULT_STRIP_PREFIX)
    }

    /// Check what the engine needs to render the component. `Err` makes it
    /// unusable; `Ok` carries warnings about parts that will not render
    /// (messages follow SignalRGB's validation texts).
    pub fn validate(&self) -> Result<Vec<String>, String> {
        if self.product_name.trim().is_empty() {
            return Err("ProductName is required".into());
        }
        if self.led_count == 0 || self.led_count > MAX_COMPONENT_LEDS {
            return Err(format!(
                "Field \"LedCount\" isn't a Number, or is out of bounds {{1-{MAX_COMPONENT_LEDS}}}"
            ));
        }
        let cells = u64::from(self.width) * u64::from(self.height);
        if self.width == 0
            || self.height == 0
            || self.width > MAX_COMPONENT_SIDE
            || self.height > MAX_COMPONENT_SIDE
            || cells > MAX_COMPONENT_CELLS
        {
            return Err(format!(
                "Width x Height {}x{} is outside 1..={MAX_COMPONENT_SIDE} per side / {MAX_COMPONENT_CELLS} cells",
                self.width, self.height
            ));
        }
        let n = self.led_count as usize;
        if self.led_mapping.len() != n || self.led_coordinates.len() != n {
            return Err(format!(
                "Array Lengths Differ! \"LedMapping\" equals length: {}, \"LedCoordinates\" {}, and \"LedNames\" {} (LedCount {n})",
                self.led_mapping.len(),
                self.led_coordinates.len(),
                self.led_names.len()
            ));
        }
        let mut warnings = Vec::new();
        if self.led_names.len() != n {
            warnings.push(format!(
                "\"LedNames\" has {} entries for {n} LEDs; LED names will be wrong",
                self.led_names.len()
            ));
        }
        for (i, [x, y]) in self.led_coordinates.iter().enumerate() {
            if *x >= self.width || *y >= self.height {
                warnings.push(format!(
                    "Led Coordinate Index {i}:[{x},{y}] is out of Width/Height bounds and will not render properly!"
                ));
            }
        }
        let mut seen = vec![false; n];
        for (i, &slot) in self.led_mapping.iter().enumerate() {
            match seen.get_mut(slot as usize) {
                Some(s) if !*s => *s = true,
                Some(_) => warnings.push(format!("LedMapping index {i}: slot {slot} is used twice")),
                None => warnings.push(format!(
                    "LedMapping index {i}: slot {slot} is outside the {n} LED stream"
                )),
            }
        }
        Ok(warnings)
    }

    /// For each LED (in `LedCoordinates` order), the stream slot it fills, or
    /// `None` when its `LedMapping` entry points outside the stream.
    pub fn stream_slots(&self) -> Vec<Option<usize>> {
        let n = self.led_count as usize;
        self.led_mapping
            .iter()
            .map(|&s| ((s as usize) < n).then_some(s as usize))
            .collect()
    }
}

/// One component assigned to a channel.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentInstance {
    /// `ComponentId`: unique per instance; the instance's canvas endpoint uid.
    pub id: String,
    /// The descriptor.
    pub component: Component,
    /// The full JSON object, unknown keys kept, `ComponentId` included
    /// (`channel.getComponentData(id)`).
    pub data: Map<String, Value>,
}

impl ComponentInstance {
    /// A new instance of `component` with a fresh UUID.
    pub fn new(component: Component) -> Self {
        Self::with_id(component, uuid::Uuid::new_v4().to_string())
    }

    /// An instance of `component` with the given id.
    pub fn with_id(component: Component, id: String) -> Self {
        let mut data = match serde_json::to_value(&component) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        };
        data.insert(COMPONENT_ID_KEY.into(), Value::String(id.clone()));
        Self { id, component, data }
    }

    /// Parse one persisted instance. A missing `ComponentId` gets a fresh UUID.
    /// Returns the instance and its validation warnings.
    pub fn from_json(value: Value) -> Result<(Self, Vec<String>), String> {
        let Value::Object(mut data) = value else {
            return Err("a component instance must be a JSON object".into());
        };
        let component: Component = serde_json::from_value(Value::Object(data.clone()))
            .map_err(|e| format!("invalid component: {e}"))?;
        let warnings = component.validate()?;
        let id = match data.get(COMPONENT_ID_KEY) {
            Some(Value::String(id)) if !id.trim().is_empty() => id.clone(),
            _ => {
                let id = uuid::Uuid::new_v4().to_string();
                data.insert(COMPONENT_ID_KEY.into(), Value::String(id.clone()));
                id
            }
        };
        Ok((Self { id, component, data }, warnings))
    }

    /// The JSON form (what [`parse_channel`] reads back).
    pub fn to_json(&self) -> Value {
        Value::Object(self.data.clone())
    }
}

/// Parse a persisted channel: a JSON array of component instances in chain
/// order. Unusable entries are skipped and reported with the warnings.
pub fn parse_channel(json: &str) -> Result<(Vec<ComponentInstance>, Vec<String>), String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("not JSON: {e}"))?;
    let Value::Array(items) = value else {
        return Err("a channel must be a JSON array of components".into());
    };
    let mut instances = Vec::with_capacity(items.len());
    let mut notes = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        match ComponentInstance::from_json(item) {
            Ok((instance, warnings)) => {
                notes.extend(warnings.into_iter().map(|w| format!("component #{i}: {w}")));
                if instances.iter().any(|x: &ComponentInstance| x.id == instance.id) {
                    notes.push(format!(
                        "component #{i}: duplicate ComponentId {}, skipped",
                        instance.id
                    ));
                } else {
                    instances.push(instance);
                }
            }
            Err(e) => notes.push(format!("component #{i} skipped: {e}")),
        }
    }
    Ok((instances, notes))
}

/// A descriptor in the library.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryEntry {
    /// The descriptor.
    pub component: Component,
    /// Its JSON object as read (unknown keys kept).
    pub data: Map<String, Value>,
    /// File it came from.
    pub source: PathBuf,
}

impl LibraryEntry {
    /// A new instance of this descriptor (fresh `ComponentId`).
    pub fn instantiate(&self) -> ComponentInstance {
        let id = uuid::Uuid::new_v4().to_string();
        let mut data = self.data.clone();
        data.insert(COMPONENT_ID_KEY.into(), Value::String(id.clone()));
        ComponentInstance {
            id,
            component: self.component.clone(),
            data,
        }
    }
}

/// A problem found while loading descriptor files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadIssue {
    /// The file.
    pub path: PathBuf,
    /// `true` when the file was rejected, `false` for a warning.
    pub rejected: bool,
    /// What is wrong.
    pub message: String,
}

/// Component descriptors by `ProductName`.
#[derive(Clone, Debug, Default)]
pub struct ComponentLibrary {
    entries: BTreeMap<String, LibraryEntry>,
    files: usize,
}

impl ComponentLibrary {
    /// Load every `*.json` below `dirs` (recursively, sorted), in order: a
    /// later file with the same `ProductName` replaces an earlier one.
    /// Missing directories are skipped.
    pub fn load(dirs: &[PathBuf]) -> (Self, Vec<LoadIssue>) {
        let mut lib = Self::default();
        let mut issues = Vec::new();
        for dir in dirs {
            let mut files = Vec::new();
            collect_json(dir, 0, &mut files);
            files.sort();
            for path in files {
                lib.files += 1;
                match load_descriptor(&path) {
                    Ok((entry, warnings)) => {
                        issues.extend(warnings.into_iter().map(|message| LoadIssue {
                            path: path.clone(),
                            rejected: false,
                            message,
                        }));
                        lib.entries.insert(entry.component.product_name.clone(), entry);
                    }
                    Err(message) => issues.push(LoadIssue {
                        path,
                        rejected: true,
                        message,
                    }),
                }
            }
        }
        (lib, issues)
    }

    /// Descriptor files read (including rejected and overridden ones).
    pub fn files(&self) -> usize {
        self.files
    }

    /// Descriptors by `ProductName`.
    pub fn entries(&self) -> impl Iterator<Item = &LibraryEntry> {
        self.entries.values()
    }

    /// Number of descriptors.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the library is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The descriptor with this `ProductName`.
    pub fn get(&self, product_name: &str) -> Option<&LibraryEntry> {
        self.entries.get(product_name)
    }

    /// Distinct brands, sorted (the picker's filter list).
    pub fn brands(&self) -> Vec<String> {
        let mut v: Vec<String> = self.entries.values().map(|e| e.component.brand.clone()).collect();
        v.sort();
        v.dedup();
        v
    }
}

fn collect_json(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    // Brand folders are one level deep; a few more levels tolerate user trees.
    if depth > 4 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_json(&path, depth + 1, out);
        } else if kind.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            out.push(path);
        }
    }
}

fn load_descriptor(path: &Path) -> Result<(LibraryEntry, Vec<String>), String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_COMPONENT_FILE {
        return Err(format!(
            "{len} bytes is larger than the {MAX_COMPONENT_FILE} byte limit"
        ));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    // Some editors save a UTF-8 BOM.
    let value: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("not JSON: {e}"))?;
    let Value::Object(data) = value else {
        return Err("a component descriptor must be a JSON object".into());
    };
    let component: Component =
        serde_json::from_value(Value::Object(data.clone())).map_err(|e| format!("invalid component: {e}"))?;
    let warnings = component.validate()?;
    Ok((
        LibraryEntry {
            component,
            data,
            source: path.to_path_buf(),
        },
        warnings,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAN: &str = r#"{
        "ProductName": "Test Fan 4",
        "DisplayName": "Test Fan - 4 LED",
        "Brand": "Generic",
        "Type": "Fan",
        "LedCount": 4,
        "Width": 3,
        "Height": 3,
        "LedMapping": [3, 2, 1, 0],
        "LedCoordinates": [[1,0],[2,1],[1,2],[0,1]],
        "LedNames": ["Top","Right","Bottom","Left"],
        "ImageUrl": "https://example.invalid/fan.png",
        "Extra": {"kept": true}
    }"#;

    #[test]
    fn placeholder_matches_signalrgb_shape() {
        let s = Component::default_strip(3);
        assert!(s.is_default_strip());
        assert_eq!(s.validate(), Ok(vec![]));
        let inst = ComponentInstance::with_id(s, "id-1".into());
        let json = inst.to_json();
        assert_eq!(json["ProductName"], "Default Strip - 3");
        assert_eq!(json["DisplayName"], "Default Strip - 3");
        assert_eq!(json["Brand"], "Custom");
        assert_eq!(json["Type"], "Strip");
        assert_eq!(json["Width"], 3);
        assert_eq!(json["Height"], 1);
        assert_eq!(json["LedMapping"], serde_json::json!([0, 1, 2]));
        assert_eq!(
            json["LedCoordinates"],
            serde_json::json!([[0, 0], [1, 0], [2, 0]])
        );
        assert_eq!(json["LedNames"], serde_json::json!(["Led 1", "Led 2", "Led 3"]));
        assert_eq!(json["ComponentId"], "id-1");
    }

    #[test]
    fn instances_keep_unknown_keys_and_ids() {
        let mut v: Value = serde_json::from_str(FAN).unwrap();
        v["ComponentId"] = "abc".into();
        let (inst, warnings) = ComponentInstance::from_json(v).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(inst.id, "abc");
        assert_eq!(inst.data["Extra"]["kept"], true);
        assert_eq!(inst.component.kind, "Fan");
        assert_eq!(
            inst.component.stream_slots(),
            vec![Some(3), Some(2), Some(1), Some(0)]
        );
        // No id: a fresh one, also written into the JSON.
        let (fresh, _) = ComponentInstance::from_json(serde_json::from_str(FAN).unwrap()).unwrap();
        assert_eq!(fresh.data["ComponentId"], fresh.id.as_str());
        assert_eq!(fresh.id.len(), 36);
    }

    #[test]
    fn validation_rejects_unrenderable_and_warns_about_the_rest() {
        let mut c = Component::default_strip(4);
        c.led_mapping.pop();
        assert!(c.validate().unwrap_err().starts_with("Array Lengths Differ!"));
        let mut c = Component::default_strip(4);
        c.led_count = 0;
        assert!(c.validate().is_err());
        let mut c = Component::default_strip(4);
        c.width = 0;
        assert!(c.validate().is_err());
        let mut c = Component::default_strip(4);
        c.led_coordinates[1] = [9, 0];
        c.led_mapping[2] = 7;
        c.led_names.clear();
        let warnings = c.validate().unwrap();
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert_eq!(c.stream_slots()[2], None);
    }

    #[test]
    fn channels_parse_in_chain_order_and_skip_bad_entries() {
        let strip = ComponentInstance::with_id(Component::default_strip(2), "a".into()).to_json();
        let json = serde_json::json!([strip, {"ProductName": "broken"}, strip]).to_string();
        let (instances, notes) = parse_channel(&json).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(parse_channel("{}").is_err());
        assert!(parse_channel("not json").is_err());
    }

    #[test]
    fn library_overrides_by_product_name_in_directory_order() {
        let base = std::env::temp_dir().join(format!("huedeck-components-{}", std::process::id()));
        let (builtin, user) = (base.join("builtin/Generic"), base.join("user"));
        std::fs::create_dir_all(&builtin).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(builtin.join("fan.json"), FAN).unwrap();
        std::fs::write(builtin.join("bad.json"), "{").unwrap();
        let mut over: Value = serde_json::from_str(FAN).unwrap();
        over["DisplayName"] = "User Fan".into();
        std::fs::write(user.join("fan.JSON"), over.to_string()).unwrap();
        let (lib, issues) =
            ComponentLibrary::load(&[base.join("builtin"), user.clone(), base.join("missing")]);
        std::fs::remove_dir_all(&base).ok();
        assert_eq!((lib.len(), lib.files()), (1, 3));
        assert_eq!(issues.iter().filter(|i| i.rejected).count(), 1);
        let entry = lib.get("Test Fan 4").unwrap();
        assert_eq!(entry.component.display_name, "User Fan");
        assert_eq!(lib.brands(), vec!["Generic".to_string()]);
        let a = entry.instantiate();
        let b = entry.instantiate();
        assert_ne!(a.id, b.id);
        assert_eq!(a.data["ComponentId"], a.id.as_str());
    }
}
