// SPDX-License-Identifier: GPL-3.0-or-later
//! What the dashboard shows from a plugin's `manifest.json`
//! (`schemas/plugin-manifest-v2.schema.json`): its version, and which devices
//! someone has run it on (`tested`: product ids of a USB claim, PCI tuples of
//! an SMBus claim). The host binds devices from the plugin's exports; the
//! manifest is only read for these facts, and a plugin without one (a
//! third-party SignalRGB plugin) simply has none.

use std::path::Path;

use lightloom_hal::smbus::PciIds;
use serde_json::Value;

/// The facts of one `manifest.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManifestFacts {
    /// `version`.
    pub version: Option<String>,
    /// `(vid, pid)` of every `tested` product id of the USB claims.
    pub tested_usb: Vec<(u16, u16)>,
    /// The `tested` cards of the SMBus claims.
    pub tested_pci: Vec<PciIds>,
}

/// A `usbId`: a number, or a hex string with or without `0x`.
fn id(v: &Value) -> Option<u16> {
    match v {
        Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        Value::String(s) => {
            let s = s.trim();
            let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
            u16::from_str_radix(hex, 16).ok()
        }
        _ => None,
    }
}

impl ManifestFacts {
    /// Parse a manifest; `None` when it is not JSON or not an object.
    pub fn parse(json: &str) -> Option<Self> {
        let m: Value = serde_json::from_str(json).ok()?;
        let m = m.as_object()?;
        let mut facts = Self {
            version: m.get("version").and_then(Value::as_str).map(String::from),
            ..Self::default()
        };
        for claim in m.get("devices").and_then(Value::as_array).into_iter().flatten() {
            if let Some(vid) = claim.get("vid").and_then(id) {
                for pid in claim
                    .get("tested")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(pid) = id(pid) {
                        facts.tested_usb.push((vid, pid));
                    }
                }
            }
            let cards = claim
                .get("smbus")
                .and_then(|s| s.get("tested"))
                .and_then(Value::as_array);
            for card in cards.into_iter().flatten() {
                let field = |k: &str| card.get(k).and_then(id);
                if let (Some(vendor), Some(device), Some(sub_vendor), Some(sub_device)) = (
                    field("vendor"),
                    field("device"),
                    field("subVendor"),
                    field("subDevice"),
                ) {
                    facts.tested_pci.push(PciIds {
                        vendor,
                        device,
                        sub_vendor,
                        sub_device,
                    });
                }
            }
        }
        Some(facts)
    }

    /// The `manifest.json` next to a plugin's entry file, if there is one.
    pub fn for_plugin(entry: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(entry.parent()?.join("manifest.json")).ok()?;
        Self::parse(&text)
    }

    /// Someone ran the plugin on this USB product.
    pub fn tests_usb(&self, vid: u16, pid: u16) -> bool {
        self.tested_usb.contains(&(vid, pid))
    }

    /// Someone ran the plugin on this card.
    pub fn tests_pci(&self, pci: &PciIds) -> bool {
        self.tested_pci.contains(pci)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_version_and_tested_usb_products() {
        let m = ManifestFacts::parse(
            r#"{ "version": "0.2.0", "devices": [
                { "vid": "0x1462", "pids": ["0x7C91", "0x7C92"], "tested": ["0x7C91"] },
                { "vid": 4152, "pids": [6198], "tested": [6198] } ] }"#,
        )
        .unwrap();
        assert_eq!(m.version.as_deref(), Some("0.2.0"));
        assert!(m.tests_usb(0x1462, 0x7c91));
        assert!(!m.tests_usb(0x1462, 0x7c92));
        assert!(m.tests_usb(4152, 6198));
    }

    #[test]
    fn reads_tested_cards() {
        let m = ManifestFacts::parse(
            r#"{ "version": "1.0.0", "devices": [ { "smbus": { "addresses": [104],
                "tested": [{ "vendor": "0x10DE", "device": "0x2504", "subVendor": "0x1462", "subDevice": "0x3903" }] } } ] }"#,
        )
        .unwrap();
        let card = PciIds {
            vendor: 0x10de,
            device: 0x2504,
            sub_vendor: 0x1462,
            sub_device: 0x3903,
        };
        assert!(m.tests_pci(&card));
        assert!(!m.tests_pci(&PciIds {
            sub_device: 1,
            ..card
        }));
        assert!(m.tested_usb.is_empty());
    }

    #[test]
    fn untested_and_broken_manifests() {
        let m = ManifestFacts::parse(
            r#"{ "devices": [ { "vid": "0x1038", "pids": ["0x1836"], "tested": [] } ] }"#,
        )
        .unwrap();
        assert!(!m.tests_usb(0x1038, 0x1836));
        assert_eq!(m.version, None);
        assert_eq!(ManifestFacts::parse("not json"), None);
        assert_eq!(ManifestFacts::parse("[]"), None);
    }

    #[test]
    fn every_first_party_manifest_parses() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let mut seen = 0;
        for e in std::fs::read_dir(dir).unwrap() {
            let entry = e.unwrap().path().join("plugin.js");
            if entry.parent().unwrap().join("manifest.json").is_file() {
                let m = ManifestFacts::for_plugin(&entry).expect("manifest parses");
                assert!(m.version.is_some(), "{}", entry.display());
                seen += 1;
            }
        }
        assert!(seen > 10);
        let msi = ManifestFacts::for_plugin(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/msi-mystic-light-7c91/plugin.js"),
        )
        .unwrap();
        assert!(msi.tests_usb(0x1462, 0x7c91));
    }
}
