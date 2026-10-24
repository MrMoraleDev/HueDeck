// SPDX-License-Identifier: GPL-3.0-or-later
//! The LampArray backend of this OS (Windows Dynamic Lighting), shared by
//! the daemon and the CLI, and which of its devices HueDeck drives itself.
//!
//! Ownership: a HID plugin always wins over the LampArray backend. A
//! LampArray is left alone when a plugin-driven HID device (one the catalog
//! matched, enabled or not) has, in this order:
//!
//! 1. its interface path among its collections (the `hid-lamparray` plugin
//!    claims every 0x59/0x01 collection, so with it installed every HID
//!    LampArray it can open is the plugin's);
//! 2. the same container id (`DEVPKEY_Device_ContainerId`: another
//!    interface, or another product id of the same physical device);
//! 3. the same VID:PID (plugins claim by VID:PID; HueDeck never drives one
//!    product through two routes).

use huedeck_hal::lamparray::{device_key, plugin_owner, LampArrayBackend, OwnedBy, OwnedHid};
use huedeck_hal::HalResult;
use huedeck_plugin_host::lamparray::builtin_entry;
use huedeck_plugin_host::manager::Present;
use huedeck_plugin_host::session::SessionDevice;
use std::sync::Arc;

/// Start the OS LampArray backend: on Windows the WinRT one
/// (`huedeck_hal_windows::lamparray`); `None` where there is none (other
/// OSes, Windows without `Windows.Devices.Lights`).
#[cfg(windows)]
pub fn open() -> Option<Arc<dyn LampArrayBackend>> {
    match huedeck_hal_windows::lamparray::WinLampArray::start() {
        Ok(b) => Some(Arc::new(b)),
        Err(e) => {
            tracing::info!("Windows Dynamic Lighting is not available: {e}");
            None
        }
    }
}

/// No LampArray backend on this OS (HID LampArrays go through the
/// `hid-lamparray` plugin).
#[cfg(not(windows))]
pub fn open() -> Option<Arc<dyn LampArrayBackend>> {
    None
}

/// What a LampArray scan found.
#[derive(Debug, Default)]
pub struct LampScan {
    /// LampArrays the built-in session drives (no plugin owns them).
    pub present: Vec<Present>,
    /// LampArrays a plugin's device owns, as `(LampArray key, plugin device key, why)`.
    pub owned: Vec<(String, String, OwnedBy)>,
}

/// The LampArrays of `backend` HueDeck drives itself: every one no
/// plugin-driven HID device of `hid` owns (see the module docs) and whose key
/// `[devices] ignore` does not match.
pub fn present(
    backend: &Arc<dyn LampArrayBackend>,
    hid: &[Present],
    ignore: &[String],
) -> HalResult<LampScan> {
    let lamps = backend.enumerate()?;
    let mut scan = LampScan::default();
    if lamps.is_empty() {
        return Ok(scan);
    }
    let want_containers = lamps.iter().any(|l| l.container_id.is_some());
    let owned: Vec<OwnedHid> = hid
        .iter()
        .filter(|p| p.device.smbus.is_none() && p.device.lamparray.is_none())
        .map(|p| {
            let paths: Vec<String> = p.device.endpoints.iter().map(|e| e.path.clone()).collect();
            let mut containers: Vec<String> = if want_containers {
                paths
                    .iter()
                    .filter_map(|path| backend.container_of(path))
                    .collect()
            } else {
                Vec::new()
            };
            containers.sort();
            containers.dedup();
            OwnedHid {
                key: p.device.key.clone(),
                vid: p.device.vid,
                pid: p.device.pid,
                paths,
                containers,
            }
        })
        .collect();
    for info in lamps {
        let key = device_key(&info);
        if let Some((o, why)) = plugin_owner(&info, &owned) {
            scan.owned.push((key, o.key.clone(), why));
            continue;
        }
        if !ignore.is_empty() && crate::hardware::matches_filter(&key, ignore) {
            continue;
        }
        scan.present.push(Present {
            plugin: builtin_entry(&info),
            device: SessionDevice::lamparray(Arc::clone(backend), info),
        });
    }
    Ok(scan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use huedeck_hal::mock::{MockHid, MockLampArray};
    use huedeck_hal::ReportLengths;
    use huedeck_plugin_host::catalog::CatalogEntry;
    use huedeck_plugin_host::meta::PluginMeta;

    const LAMP_PATH: &str =
        r"\\?\HID#VID_1532&PID_007C&MI_03#8&5cc7230&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    const OTHER_PATH: &str = r"\\?\hid#vid_1532&pid_007c&mi_00#1&2&3#{x}";

    fn hid_device(key: &str, vid: u16, pid: u16, paths: &[&str]) -> Present {
        let endpoints = paths
            .iter()
            .map(|p| {
                let mut e = MockHid::endpoint_info(vid, pid, "S", 0, 0xFF00, 1, 0, ReportLengths::default());
                e.path = p.to_string();
                e
            })
            .collect();
        Present {
            device: SessionDevice::hid(key, vid, pid, endpoints),
            plugin: CatalogEntry {
                meta: PluginMeta::minimal("p.js", "Vendor plugin", vid, vec![pid]),
                root: Default::default(),
            },
        }
    }

    fn backend(mock: &MockLampArray) -> Arc<dyn LampArrayBackend> {
        Arc::new(mock.clone())
    }

    #[test]
    fn unowned_lamparrays_are_driven_by_the_builtin_session() {
        let mock = MockLampArray::new();
        mock.add(MockLampArray::row(LAMP_PATH, 0x1532, 0x007c, 4));
        let scan = present(&backend(&mock), &[], &[]).unwrap();
        assert_eq!(scan.present.len(), 1);
        let p = &scan.present[0];
        assert_eq!(p.device.key, "lamparray:1532:007c:8-5cc7230-0-0000");
        assert_eq!(p.plugin.meta.name, huedeck_plugin_host::lamparray::BUILTIN_NAME);
        assert!(p.device.lamparray.is_some() && p.device.endpoints.is_empty());
        // `[devices] ignore` applies to LampArray keys too.
        let scan = present(&backend(&mock), &[], &["lamparray:1532".into()]).unwrap();
        assert!(scan.present.is_empty() && scan.owned.is_empty());
    }

    #[test]
    fn a_plugin_on_the_same_interface_wins() {
        // The hid-lamparray plugin (or a vendor plugin) drives the 0x59 collection itself.
        let mock = MockLampArray::new();
        mock.add(MockLampArray::row(LAMP_PATH, 0x1532, 0x007c, 4));
        let lower = LAMP_PATH.to_ascii_lowercase();
        let hid = [hid_device("1532:007c:X", 0x1532, 0x007c, &[&lower])];
        let scan = present(&backend(&mock), &hid, &[]).unwrap();
        assert!(scan.present.is_empty());
        assert_eq!(
            scan.owned,
            vec![(
                "lamparray:1532:007c:8-5cc7230-0-0000".to_string(),
                "1532:007c:X".to_string(),
                OwnedBy::Path
            )]
        );
    }

    #[test]
    fn a_plugin_on_the_same_physical_device_wins_by_container_id() {
        // The plugin drives the wired product id; Windows lists the LampArray of the dongle one.
        let mock = MockLampArray::new();
        let mut lamp = MockLampArray::row(LAMP_PATH, 0x1532, 0x007d, 2);
        lamp.container_id = Some("c0ffee".into());
        mock.add(lamp);
        mock.set_container(OTHER_PATH, "c0ffee");
        let hid = [hid_device("1532:007c:Y", 0x1532, 0x007c, &[OTHER_PATH])];
        let scan = present(&backend(&mock), &hid, &[]).unwrap();
        assert!(scan.present.is_empty());
        assert_eq!(scan.owned[0].2, OwnedBy::Container);
    }

    #[test]
    fn a_plugin_for_the_same_product_wins_and_others_stay() {
        let mock = MockLampArray::new();
        mock.add(MockLampArray::row("lamp-owned", 0x1532, 0x007c, 2));
        mock.add(MockLampArray::row("lamp-free", 0x0b05, 0x1234, 2));
        let unrelated = MockLampArray::row("unrelated", 0x0b05, 0x1234, 1);
        let hid = [
            hid_device("1532:007c:Z", 0x1532, 0x007c, &["some-other-interface"]),
            // LampArray (and SMBus) devices own nothing.
            Present {
                plugin: huedeck_plugin_host::lamparray::builtin_entry(&unrelated),
                device: SessionDevice::lamparray(backend(&mock), unrelated),
            },
        ];
        let scan = present(&backend(&mock), &hid, &[]).unwrap();
        assert_eq!(scan.owned.len(), 1);
        assert_eq!(scan.owned[0].2, OwnedBy::VidPid);
        assert_eq!(scan.present.len(), 1);
        assert_eq!(scan.present[0].device.vid, 0x0b05);
    }
}
