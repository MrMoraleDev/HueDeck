// SPDX-License-Identifier: GPL-3.0-or-later
//! Device facts for the dashboard's device reports: whether the plugin's
//! manifest lists the device as tested, the plugin's version and type, and the
//! device's interfaces as enumeration saw them (no paths, no serial numbers).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lightloom_plugin_host::manager::Present;
use lightloom_plugin_host::manifest::ManifestFacts;
use lightloom_plugin_host::session::SessionDevice;
use lightloom_plugin_host::PluginType;

/// Each plugin's `manifest.json` facts, read once per plugin file: the device
/// list is rebuilt every second while devices run.
#[derive(Default)]
pub(super) struct ManifestCache(Mutex<HashMap<PathBuf, Option<Arc<ManifestFacts>>>>);

impl ManifestCache {
    pub(super) fn get(&self, entry: &Path) -> Option<Arc<ManifestFacts>> {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(entry.to_path_buf())
            .or_insert_with(|| ManifestFacts::for_plugin(entry).map(Arc::new))
            .clone()
    }

    /// Plugins were scanned again: read the manifests again when asked.
    pub(super) fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// The device is in the `tested` list of its plugin's manifest.
pub(super) fn tested(facts: Option<&ManifestFacts>, device: &SessionDevice) -> bool {
    let Some(facts) = facts else { return false };
    match &device.smbus {
        Some(t) => facts.tests_pci(&t.bus.descriptor().pci),
        None if device.network.is_none() && device.lamparray.is_none() => {
            facts.tests_usb(device.vid, device.pid)
        }
        None => false,
    }
}

/// How the plugin reaches the device.
pub(super) fn plugin_type(p: &Present) -> &'static str {
    if p.device.lamparray.is_some() {
        return "lamparray";
    }
    match p.plugin.meta.plugin_type {
        PluginType::Hid => "hid",
        PluginType::RawUsb => "rawusb",
        PluginType::Hybrid => "hybrid",
        PluginType::Serial => "serial",
        PluginType::Smbus => "smbus",
        PluginType::Network => "network",
    }
}

/// One line per interface: HID collections, the USB interfaces (class and
/// driver), serial ports. Paths, serial numbers and addresses stay out.
pub(super) fn interfaces(device: &SessionDevice) -> Vec<String> {
    let mut out: Vec<String> = device
        .endpoints
        .iter()
        .map(|e| {
            let interface = if e.interface_number < 0 {
                "interface ?".to_string()
            } else {
                format!("interface {}", e.interface_number)
            };
            format!(
                "HID {interface} · usage page 0x{:04x} · usage 0x{:04x} · collection {}",
                e.usage_page, e.usage, e.collection
            )
        })
        .collect();
    if let Some(usb) = &device.usb {
        for i in &usb.info.interfaces {
            out.push(format!(
                "USB interface {} · class 0x{:02x}/0x{:02x}/0x{:02x} · driver {}{}",
                i.number,
                i.class,
                i.subclass,
                i.protocol,
                if i.driver.is_empty() { "none" } else { &i.driver },
                if i.accessible { " (usable)" } else { "" }
            ));
        }
    }
    if let Some(serial) = &device.serial {
        for port in &serial.ports {
            out.push(match port.interface {
                Some(i) => format!("serial port on interface {i}"),
                None => "serial port".to_string(),
            });
        }
    }
    if let Some(t) = &device.smbus {
        let d = t.bus.descriptor();
        out.push(format!("SMBus {:?} · address 0x{:02x}", d.kind, t.addr).to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::HidEndpointInfo;

    fn hid(vid: u16, pid: u16) -> SessionDevice {
        let endpoint = HidEndpointInfo {
            vid,
            pid,
            serial: "SECRET123456".into(),
            path: r"\\?\HID#VID_1462&PID_7C91#secret".into(),
            interface_number: 0,
            usage_page: 0xff00,
            usage: 1,
            ..HidEndpointInfo::default()
        };
        SessionDevice::hid(
            format!("{vid:04x}:{pid:04x}:SECRET123456"),
            vid,
            pid,
            vec![endpoint],
        )
    }

    #[test]
    fn hid_interfaces_leave_paths_and_serials_out() {
        let lines = interfaces(&hid(0x1462, 0x7c91));
        assert_eq!(
            lines,
            vec!["HID interface 0 · usage page 0xff00 · usage 0x0001 · collection 0"]
        );
        assert!(!lines.join(" ").contains("SECRET"));
    }

    #[test]
    fn tested_follows_the_manifest() {
        let facts = ManifestFacts::parse(
            r#"{ "devices": [ { "vid": "0x1462", "pids": ["0x7C91"], "tested": ["0x7C91"] } ] }"#,
        );
        assert!(tested(facts.as_ref(), &hid(0x1462, 0x7c91)));
        assert!(!tested(facts.as_ref(), &hid(0x1462, 0x7c92)));
        assert!(!tested(None, &hid(0x1462, 0x7c91)));
    }
}
