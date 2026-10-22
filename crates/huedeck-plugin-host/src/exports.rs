// SPDX-License-Identifier: GPL-3.0-or-later
//! Conversion of the JSON image of a plugin namespace into [`PluginMeta`].
//!
//! The runtime applies the call-if-function rule inside the engine and hands
//! over a JSON object `{ exports, errors, lifecycle, hooks, names }`
//! (see `js/host.js`, `readMeta`). This module turns it into the typed
//! model leniently: malformed fields fall back to their defaults and are
//! reported as warnings instead of rejecting the plugin, so the corpus
//! dry-run can count every problem at once.

use crate::meta::{
    coerce_number, smbus_kind, ControllableParameter, DeviceMessage, DeviceType, LifecycleExports, ParamType,
    ParamValue, PluginMeta, PluginType, ProductIdExport,
};
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::path::Path;

/// Result of reading a namespace.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportsReading {
    /// Typed metadata (defaults where an export is missing or malformed).
    pub meta: PluginMeta,
    /// Which metadata exports exist in the namespace.
    pub present: Vec<String>,
    /// Exports whose call threw: `export name → "Error: message"`.
    pub errors: BTreeMap<String, String>,
    /// Shape problems found while converting.
    pub warnings: Vec<String>,
}

impl ExportsReading {
    /// Whether the module exported `name`.
    pub fn has(&self, name: &str) -> bool {
        self.present.iter().any(|n| n == name)
    }
}

fn as_string(v: &J) -> Option<String> {
    match v {
        J::String(s) => Some(s.clone()),
        J::Number(n) => Some(n.to_string()),
        J::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn as_f64(v: &J) -> Option<f64> {
    match v {
        J::Number(n) => n.as_f64(),
        J::String(s) => coerce_number(s),
        _ => None,
    }
}

fn truthy(v: &J) -> bool {
    match v {
        J::Null => false,
        J::Bool(b) => *b,
        J::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        J::String(s) => !s.is_empty(),
        J::Array(_) | J::Object(_) => true,
    }
}

fn pair_u32(v: &J) -> Option<[u32; 2]> {
    let a = v.as_array()?;
    let x = as_f64(a.first()?)?;
    let y = as_f64(a.get(1)?)?;
    (x >= 0.0 && y >= 0.0).then_some([x as u32, y as u32])
}

fn param_value(v: &J) -> Option<ParamValue> {
    match v {
        J::Number(n) => n.as_f64().map(ParamValue::Number),
        J::Bool(b) => Some(ParamValue::Bool(*b)),
        J::String(s) => Some(ParamValue::Text(s.clone())),
        _ => None,
    }
}

fn parameter(v: &J, warnings: &mut Vec<String>) -> Option<ControllableParameter> {
    let o = v.as_object()?;
    let property = o.get("property").and_then(as_string)?;
    let raw_type = o.get("type").and_then(as_string).unwrap_or_default();
    let param_type = match raw_type.parse::<ParamType>() {
        Ok(t) => t,
        Err(e) => {
            warnings.push(format!("ControllableParameters[{property}]: {e}"));
            return None;
        }
    };
    let s = |k: &str| o.get(k).and_then(as_string);
    let f = |k: &str| o.get(k).and_then(as_f64);
    Some(ControllableParameter {
        label: s("label").unwrap_or_else(|| property.clone()),
        group: s("group"),
        description: s("description"),
        tooltip: s("tooltip"),
        param_type,
        values: o
            .get("values")
            .and_then(J::as_array)
            .map(|a| a.iter().filter_map(as_string).collect())
            .unwrap_or_default(),
        min: f("min"),
        max: f("max"),
        step: f("step"),
        default: o.get("default").and_then(param_value),
        live: o.get("live").map(truthy).unwrap_or(true),
        order: f("order").map(|v| v as i64),
        filter: s("filter"),
        property,
    })
}

/// Build typed metadata from the namespace JSON produced by the engine.
pub fn read_exports(source_path: &Path, ns: &J) -> ExportsReading {
    let empty = serde_json::Map::new();
    let exports = ns.get("exports").and_then(J::as_object).unwrap_or(&empty);
    let mut warnings = Vec::new();
    let mut meta = PluginMeta::minimal(source_path, "", 0, Vec::new());
    let get = |k: &str| exports.get(k).filter(|v| !v.is_null());

    if let Some(v) = get("Name") {
        meta.name = as_string(v).unwrap_or_default();
    }
    meta.publisher = get("Publisher").and_then(as_string);
    if let Some(v) = get("VendorId") {
        match as_f64(v).filter(|f| (0.0..=65535.0).contains(f)) {
            Some(f) => meta.vendor_id = f as u16,
            None => warnings.push(format!("VendorId() is not a 16-bit int: {v}")),
        }
    }
    if let Some(v) = get("ProductId") {
        match ProductIdExport::from_json(v).map(|p| p.product_ids()) {
            Some(Ok(ids)) => meta.product_ids = ids,
            Some(Err(e)) => warnings.push(e.to_string()),
            None => warnings.push(format!("ProductId() must be of type int, or int[]: {v}")),
        }
    }
    if let Some(v) = get("Type") {
        match as_string(v).unwrap_or_default().parse::<PluginType>() {
            Ok(t) => meta.plugin_type = t,
            Err(e) => warnings.push(e),
        }
    }
    if let Some(v) = get("DeviceType").and_then(as_string) {
        meta.device_type = v.parse::<DeviceType>().unwrap_or_default();
    }
    if let Some(v) = get("Size") {
        match pair_u32(v) {
            Some(p) => meta.size = p,
            None => warnings.push(format!("Size() is not [w, h]: {v}")),
        }
    }
    if let Some(p) = get("DefaultPosition").and_then(J::as_array) {
        if let (Some(x), Some(y)) = (p.first().and_then(as_f64), p.get(1).and_then(as_f64)) {
            meta.default_position = [x as f32, y as f32];
        }
    }
    if let Some(f) = get("DefaultScale").and_then(as_f64) {
        meta.default_scale = f as f32;
    }
    if let Some(a) = get("LedNames").and_then(J::as_array) {
        meta.led_names = a.iter().map(|v| as_string(v).unwrap_or_default()).collect();
    }
    if let Some(a) = get("LedPositions").and_then(J::as_array) {
        meta.led_positions = a.iter().map(|v| pair_u32(v).unwrap_or([0, 0])).collect();
    }
    meta.image_url = get("ImageUrl").and_then(as_string);
    meta.image_base64 = get("Image").and_then(as_string);
    meta.documentation = get("Documentation").and_then(as_string);
    meta.version = get("Version").and_then(as_string);
    if let Some(a) = get("ControllableParameters").and_then(J::as_array) {
        meta.controllable_parameters = a.iter().filter_map(|p| parameter(p, &mut warnings)).collect();
    }
    if let Some(a) = get("ConflictingProcesses").and_then(J::as_array) {
        meta.conflicting_processes = a.iter().filter_map(as_string).filter(|s| !s.is_empty()).collect();
    }
    meta.supports_subdevices = get("SupportsSubdevices").is_some_and(truthy);
    meta.subdevice_controller = get("SubdeviceController").is_some_and(truthy) || meta.supports_subdevices;
    meta.lacks_on_board_leds = get("LacksOnBoardLeds").is_some_and(truthy);
    meta.default_component_brand = get("DefaultComponentBrand").and_then(as_string);
    meta.supports_fan_control = get("SupportsFanControl").is_some_and(truthy);
    if let Some(a) = get("DeviceMessages").and_then(J::as_array) {
        meta.device_messages = a
            .iter()
            .filter_map(|m| {
                let o = m.as_object()?;
                Some(DeviceMessage {
                    property: o.get("property").and_then(as_string).unwrap_or_default(),
                    message: o.get("message").and_then(as_string).unwrap_or_default(),
                    tooltip: o.get("tooltip").and_then(as_string),
                })
            })
            .collect();
    }
    meta.uses_corsair_mutex = get("UsesCorsairMutex").is_some_and(truthy);
    meta.uses_usb_sensor_mutex = get("UsesUSBSensorMutex").is_some_and(truthy);
    meta.allow_conflict_bypass = get("AllowConflictBypass").map(truthy).unwrap_or(true);
    if let Some(a) = get("BrandGPUList").and_then(J::as_array) {
        meta.brand_gpu_list = a.clone();
    }
    if let Some(a) = get("HidUsages").and_then(J::as_array) {
        for v in a {
            let pair = v.as_array().and_then(|p| {
                let page = u16::try_from(p.first()?.as_u64()?).ok()?;
                let usage = u16::try_from(p.get(1)?.as_u64()?).ok()?;
                (p.len() == 2).then_some([page, usage])
            });
            match pair {
                Some(pair) => meta.hid_usages.push(pair),
                None => warnings.push(format!("HidUsages() entries must be [usage page, usage]: {v}")),
            }
        }
    }

    if let Some(a) = get("SmbusAddresses").and_then(J::as_array) {
        for v in a {
            match v
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .filter(|n| *n <= 0x7F)
            {
                Some(addr) if !meta.smbus_addresses.contains(&addr) => meta.smbus_addresses.push(addr),
                Some(_) => {}
                None => warnings.push(format!("SmbusAddresses() entries must be 7-bit addresses: {v}")),
            }
        }
    }
    if let Some(a) = get("SmbusDeniedRegisters").and_then(J::as_array) {
        for v in a {
            match v.as_u64().and_then(|n| u8::try_from(n).ok()) {
                Some(reg) if !meta.smbus_denied_registers.contains(&reg) => {
                    meta.smbus_denied_registers.push(reg)
                }
                Some(_) => {}
                None => warnings.push(format!(
                    "SmbusDeniedRegisters() entries must be register bytes: {v}"
                )),
            }
        }
    }
    if let Some(a) = get("SpdManufacturers").and_then(J::as_array) {
        for v in a {
            let entry = v.as_object().and_then(|o| {
                let byte = |k: &str| o.get(k)?.as_u64().and_then(|n| u8::try_from(n).ok());
                Some(crate::spd::SpdManufacturer {
                    bank: byte("bank").filter(|b| (1..=32).contains(b))?,
                    id: byte("id").filter(|id| id & 0x7F != 0 && id & 0x7F != 0x7F)?,
                    name: o.get("name").and_then(as_string).unwrap_or_default(),
                })
            });
            match entry {
                Some(m) if !meta.spd_manufacturers.contains(&m) => meta.spd_manufacturers.push(m),
                Some(_) => {}
                None => warnings.push(format!(
                    "SpdManufacturers() entries must be {{bank: 1..=32, id: JEP106 id byte, name}}: {v}"
                )),
            }
        }
    }
    meta.smbus_batch_writes = get("SmbusBatchWrites").is_some_and(|v| v.as_bool() == Some(true));
    if let Some(a) = get("SmbusBuses").and_then(J::as_array) {
        for v in a {
            let name = as_string(v).unwrap_or_default();
            match smbus_kind(&name) {
                Some(kind) => meta.smbus_buses.push(kind),
                None if name.eq_ignore_ascii_case("any") => {}
                None => warnings.push(format!("SmbusBuses(): unknown bus kind {v}")),
            }
        }
    }

    let flag = |k: &str| {
        ns.get("lifecycle")
            .and_then(|l| l.get(k))
            .and_then(J::as_bool)
            .unwrap_or(false)
    };
    meta.lifecycle = LifecycleExports {
        validate: flag("Validate"),
        initialize: flag("Initialize"),
        render: flag("Render"),
        shutdown: flag("Shutdown"),
        scan: flag("Scan"),
        discovery_service: flag("DiscoveryService"),
        change_hooks: ns
            .get("hooks")
            .and_then(J::as_array)
            .map(|a| a.iter().filter_map(as_string).collect())
            .unwrap_or_default(),
    };

    let errors = ns
        .get("errors")
        .and_then(J::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), as_string(v).unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default();
    let mut present: Vec<String> = exports.keys().cloned().collect();
    if let Some(m) = ns.get("errors").and_then(J::as_object) {
        present.extend(m.keys().cloned());
    }
    ExportsReading {
        meta,
        present,
        errors,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_typical_hid_namespace() {
        let ns = json!({
            "exports": {
                "Name": "Test Mouse", "Publisher": "HueDeck", "VendorId": 0x1532,
                "ProductId": ["124", "0x7D"], "Size": [3, 3], "DefaultPosition": [10, 100],
                "DefaultScale": "8.0", "LedNames": ["Logo"], "LedPositions": [[1, 1]],
                "Type": "hid", "DeviceType": "mouse",
                "ControllableParameters": [
                    {"property": "shutdownColor", "label": "Shutdown Color", "type": "color", "default": "#000000"},
                    {"property": "dpi", "type": "number", "min": "200", "max": "16000", "default": "800"},
                    {"property": "weird", "type": "slider"}
                ],
                "ConflictingProcesses": [""], "SubdeviceController": true
            },
            "errors": {"ImageUrl": "TypeError: boom"},
            "lifecycle": {"Validate": true, "Initialize": true, "Render": true, "Shutdown": true},
            "hooks": ["onBrightnessChanged"]
        });
        let r = read_exports(Path::new("x.js"), &ns);
        let m = &r.meta;
        assert_eq!(m.name, "Test Mouse");
        assert_eq!(m.vendor_id, 0x1532);
        assert_eq!(m.product_ids, vec![124, 0x7D]);
        assert_eq!(m.device_type, DeviceType::Mouse);
        assert_eq!(m.default_scale, 8.0);
        assert_eq!(m.controllable_parameters.len(), 2);
        assert_eq!(m.controllable_parameters[1].min, Some(200.0));
        assert!(m.conflicting_processes.is_empty());
        assert!(m.subdevice_controller);
        assert!(m.lifecycle.is_device());
        assert_eq!(m.lifecycle.change_hooks, vec!["onBrightnessChanged".to_string()]);
        assert!(m.is_complete());
        assert!(m.validate().is_ok());
        assert_eq!(
            r.errors.get("ImageUrl").map(String::as_str),
            Some("TypeError: boom")
        );
        assert!(r.has("ImageUrl") && r.has("Name") && !r.has("Version"));
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    }

    #[test]
    fn reads_smbus_extensions() {
        let ns = json!({
            "exports": {
                "Name": "GPU", "Type": "SMBUS", "SmbusAddresses": [0x68, 0x68, 300],
                "SmbusBuses": ["nvidia", "any", "toaster"], "SmbusDeniedRegisters": [0x3F, 0x3F, 256],
                "SmbusBatchWrites": true,
                "SpdManufacturers": [
                    {"bank": 1, "id": 0x2C, "name": "Micron Technology"},
                    {"bank": 6, "id": 0x9B},
                    {"bank": 0, "id": 0x2C}, {"bank": 1, "id": 0x7F}, "Micron"
                ]
            },
            "lifecycle": {"Scan": true, "Initialize": true, "Render": true}
        });
        let r = read_exports(Path::new("gpu.js"), &ns);
        assert_eq!(r.meta.smbus_addresses, vec![0x68]);
        assert_eq!(r.meta.smbus_buses, vec![huedeck_hal::SmbusBusKind::Nvidia]);
        assert_eq!(r.meta.smbus_denied_registers, vec![0x3F]);
        let spd: Vec<(u8, u8, &str)> = r
            .meta
            .spd_manufacturers
            .iter()
            .map(|m| (m.bank, m.id, m.name.as_str()))
            .collect();
        assert_eq!(spd, vec![(1, 0x2C, "Micron Technology"), (6, 0x9B, "")]);
        assert!(r.meta.spd_gated());
        assert!(r.meta.smbus_batch_writes);
        assert_eq!(r.warnings.len(), 6, "{:?}", r.warnings);
    }

    #[test]
    fn helper_namespace_is_incomplete() {
        let r = read_exports(Path::new("lib.js"), &json!({"exports": {}, "lifecycle": {}}));
        assert!(!r.meta.is_complete());
        assert!(!r.has("ProductId"));
    }
}
