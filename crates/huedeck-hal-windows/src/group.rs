// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Grouping of HID top-level collections into physical devices.
//!
//! Pure logic (no OS calls) so it is unit-tested on every platform. The
//! Windows probe in [`crate::hid`] fills [`ProbedEndpoint`]s and hands them to
//! [`group_endpoints`].
//!
//! Identity rules, in priority order:
//!
//! 1. **USB parent** (`USB\VID_xxxx&PID_xxxx\<instance>` found by walking
//!    `CM_Get_Parent`, accepted only when its VID/PID equals the endpoint's —
//!    see [`own_usb_parent`]): every collection under the same USB device is
//!    one device. The key's serial is the parent's last instance-id segment with
//!    `&` → `-`, which reproduces the keys SignalRGB logs on the reference
//!    machine (`1462:7c91:A00000000001`, `054c:0ce6:6-00000000-0-10`).
//! 2. **USB serial string** reported by the HID stack (`vid:pid:serial`) when
//!    no USB parent could be resolved (e.g. Bluetooth).
//! 3. **Instance-id prefix** of the HID path (groups the collections of one
//!    HID interface only) when there is neither.

use crate::path::{parse_hid_path, signalrgb_serial_from_instance, usb_instance_vid_pid};
use huedeck_hal::hid::{HidEndpointInfo, ReportLengths};
use huedeck_hal::DeviceKey;
use std::collections::BTreeMap;

/// How a [`DeviceGroup`] was formed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GroupingSource {
    /// Common USB parent device node (cfgmgr32).
    UsbParent,
    /// Same `vid:pid:serial` string (no USB parent resolved).
    Serial,
    /// Same HID instance-id prefix (no parent, no serial).
    InstancePrefix,
}

impl GroupingSource {
    /// Short label for CLI output.
    pub fn as_str(self) -> &'static str {
        match self {
            GroupingSource::UsbParent => "usb-parent",
            GroupingSource::Serial => "serial",
            GroupingSource::InstancePrefix => "instance-prefix",
        }
    }
}

/// One enumerated HID top-level collection plus everything probing learned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbedEndpoint {
    /// Enumeration data; `report_lengths` is filled when `caps` succeeded.
    pub info: HidEndpointInfo,
    /// `ColNN` from the path (hex-decoded, 1-based), `None` when absent.
    pub collection_raw: Option<u16>,
    /// USB parent device instance id, or why it could not be resolved.
    pub usb_parent: Result<String, String>,
    /// Report lengths, or why they could not be read.
    pub caps: Result<ReportLengths, String>,
}

impl ProbedEndpoint {
    /// Build from enumeration data only (no OS probing); parent/caps unknown.
    pub fn from_info(info: HidEndpointInfo) -> Self {
        let collection_raw = parse_hid_path(&info.path).collection_raw;
        Self {
            info,
            collection_raw,
            usb_parent: Err("not probed".into()),
            caps: Err("not probed".into()),
        }
    }
}

/// Accept `parent` (the first USB *device* above a HID collection) only when it
/// is the endpoint's own USB device, i.e. its `VID_xxxx&PID_xxxx` equals the
/// endpoint's. Above a Bluetooth HID collection the first USB device is the
/// Bluetooth radio (e.g. `USB\VID_0B05&PID_17CB\…`); using it would merge
/// every paired device into one group under the radio's id.
pub fn own_usb_parent(parent: Result<String, String>, vid: u16, pid: u16) -> Result<String, String> {
    let parent = parent?;
    match usb_instance_vid_pid(&parent) {
        Some(ids) if ids == (vid, pid) => Ok(parent),
        Some((v, p)) => Err(format!(
            "first USB ancestor {parent} is {v:04x}:{p:04x}, not {vid:04x}:{pid:04x} \
             (Bluetooth radio or bridge, not this device)"
        )),
        None => Err(format!("first USB ancestor {parent} has no VID/PID")),
    }
}

/// Endpoints of one physical device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceGroup {
    /// `vid:pid:serial`, SignalRGB-shaped.
    pub key: DeviceKey,
    /// How the group was formed.
    pub source: GroupingSource,
    /// USB parent instance id when `source == UsbParent`.
    pub usb_parent: Option<String>,
    /// Manufacturer string of the first endpoint.
    pub manufacturer: String,
    /// Product string of the first endpoint.
    pub product: String,
    /// Top-level collections sorted by `(interface, collection_raw, usage_page, usage)`.
    pub endpoints: Vec<ProbedEndpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Identity {
    Parent(String),
    Serial(u16, u16, String),
    Prefix(u16, u16, String),
}

fn identity_of(ep: &ProbedEndpoint) -> (Identity, DeviceKey, GroupingSource) {
    let (vid, pid) = (ep.info.vid, ep.info.pid);
    if let Ok(parent) = &ep.usb_parent {
        if let Some(serial) = signalrgb_serial_from_instance(parent) {
            return (
                Identity::Parent(parent.to_ascii_uppercase()),
                DeviceKey::new(vid, pid, serial),
                GroupingSource::UsbParent,
            );
        }
    }
    if !ep.info.serial.is_empty() {
        return (
            Identity::Serial(vid, pid, ep.info.serial.clone()),
            DeviceKey::new(vid, pid, ep.info.serial.clone()),
            GroupingSource::Serial,
        );
    }
    let prefix = parse_hid_path(&ep.info.path)
        .instance_prefix()
        .unwrap_or("")
        .to_ascii_uppercase();
    (
        Identity::Prefix(vid, pid, prefix.clone()),
        DeviceKey::new(vid, pid, prefix.replace('&', "-")),
        GroupingSource::InstancePrefix,
    )
}

/// Group probed endpoints into physical devices. Output is sorted by key;
/// endpoints inside a group by `(interface, collection_raw, usage_page, usage)`.
pub fn group_endpoints(endpoints: Vec<ProbedEndpoint>) -> Vec<DeviceGroup> {
    let mut map: BTreeMap<Identity, DeviceGroup> = BTreeMap::new();
    for ep in endpoints {
        let (id, key, source) = identity_of(&ep);
        let group = map.entry(id).or_insert_with(|| DeviceGroup {
            key,
            source,
            usb_parent: ep
                .usb_parent
                .as_ref()
                .ok()
                .cloned()
                .filter(|_| source == GroupingSource::UsbParent),
            manufacturer: ep.info.manufacturer.clone(),
            product: ep.info.product.clone(),
            endpoints: Vec::new(),
        });
        if group.manufacturer.is_empty() {
            group.manufacturer = ep.info.manufacturer.clone();
        }
        if group.product.is_empty() {
            group.product = ep.info.product.clone();
        }
        group.endpoints.push(ep);
    }
    let mut groups: Vec<DeviceGroup> = map.into_values().collect();
    for g in &mut groups {
        g.endpoints.sort_by_key(|e| {
            (
                e.info.interface_number,
                e.collection_raw.unwrap_or(0),
                e.info.usage_page,
                e.info.usage,
            )
        });
    }
    groups.sort_by(|a, b| a.key.cmp(&b.key).then(a.source.cmp(&b.source)));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(
        path: &str,
        vid: u16,
        pid: u16,
        serial: &str,
        page: u16,
        usage: u16,
        parent: Option<&str>,
    ) -> ProbedEndpoint {
        let parsed = parse_hid_path(path);
        let mut p = ProbedEndpoint::from_info(HidEndpointInfo {
            vid,
            pid,
            serial: serial.into(),
            manufacturer: "M".into(),
            product: "P".into(),
            path: path.into(),
            interface_number: parsed.interface_number,
            usage_page: page,
            usage,
            collection: parsed.collection,
            report_lengths: ReportLengths::default(),
            feature_reports: Vec::new(),
        });
        p.usb_parent = parent.map(str::to_string).ok_or_else(|| "no parent".to_string());
        p
    }

    #[test]
    fn composite_interfaces_share_usb_parent() {
        let parent = r"USB\VID_1532&PID_007C\000000000000";
        let eps = vec![
            ep(
                r"\\?\hid#vid_1532&pid_007c&mi_03#8&3&0&0000#{g}",
                0x1532,
                0x7c,
                "",
                0xff00,
                1,
                Some(parent),
            ),
            ep(
                r"\\?\hid#vid_1532&pid_007c&mi_00#8&1&0&0000#{g}",
                0x1532,
                0x7c,
                "",
                1,
                2,
                Some(parent),
            ),
            ep(
                r"\\?\hid#vid_1532&pid_007c&mi_01&col02#8&2&0&0001#{g}",
                0x1532,
                0x7c,
                "",
                1,
                0,
                Some(parent),
            ),
            ep(
                r"\\?\hid#vid_1532&pid_007c&mi_01&col01#8&2&0&0000#{g}",
                0x1532,
                0x7c,
                "",
                1,
                6,
                Some(parent),
            ),
        ];
        let groups = group_endpoints(eps);
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.key.to_string(), "1532:007c:000000000000");
        assert_eq!(g.source, GroupingSource::UsbParent);
        assert_eq!(g.usb_parent.as_deref(), Some(parent));
        let order: Vec<(i32, Option<u16>)> = g
            .endpoints
            .iter()
            .map(|e| (e.info.interface_number, e.collection_raw))
            .collect();
        assert_eq!(order, vec![(0, None), (1, Some(1)), (1, Some(2)), (3, None)]);
        assert_eq!(g.endpoints[2].info.collection, 2);
    }

    #[test]
    fn generated_instance_id_becomes_dashed_serial() {
        let parent = r"USB\VID_054C&PID_0CE6\6&00000000&0&10";
        let groups = group_endpoints(vec![ep(
            r"\\?\hid#vid_054c&pid_0ce6&mi_03#8&9&0&0000#{g}",
            0x054c,
            0x0ce6,
            "",
            1,
            5,
            Some(parent),
        )]);
        assert_eq!(groups[0].key.to_string(), "054c:0ce6:6-00000000-0-10");
    }

    #[test]
    fn two_identical_devices_on_different_parents_stay_separate() {
        let a = r"USB\VID_1E71&PID_2007\A";
        let b = r"USB\VID_1E71&PID_2007\B";
        let groups = group_endpoints(vec![
            ep(
                r"\\?\hid#vid_1e71&pid_2007#7&a&0&0000#{g}",
                0x1e71,
                0x2007,
                "X",
                0xff00,
                1,
                Some(a),
            ),
            ep(
                r"\\?\hid#vid_1e71&pid_2007#7&b&0&0000#{g}",
                0x1e71,
                0x2007,
                "X",
                0xff00,
                1,
                Some(b),
            ),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key.serial, "A");
        assert_eq!(groups[1].key.serial, "B");
    }

    #[test]
    fn fallback_to_serial_then_instance_prefix() {
        let groups = group_endpoints(vec![
            // Same serial, no parent (e.g. Bluetooth): grouped by serial.
            ep(
                r"\\?\hid#{00001124}_vid&0002054c_pid&0ce6&col01#9&a&0&0000#{g}",
                0x054c,
                0x0ce6,
                "abc",
                1,
                5,
                None,
            ),
            ep(
                r"\\?\hid#{00001124}_vid&0002054c_pid&0ce6&col02#9&a&0&0001#{g}",
                0x054c,
                0x0ce6,
                "abc",
                0xff00,
                1,
                None,
            ),
            // No parent, no serial: grouped by instance prefix (per interface).
            ep(
                r"\\?\hid#vid_320f&pid_5064&mi_01&col01#8&5&0&0000#{g}",
                0x320f,
                0x5064,
                "",
                1,
                1,
                None,
            ),
            ep(
                r"\\?\hid#vid_320f&pid_5064&mi_01&col02#8&5&0&0001#{g}",
                0x320f,
                0x5064,
                "",
                0xc,
                1,
                None,
            ),
            ep(
                r"\\?\hid#vid_320f&pid_5064&mi_00#8&6&0&0000#{g}",
                0x320f,
                0x5064,
                "",
                1,
                6,
                None,
            ),
        ]);
        assert_eq!(groups.len(), 3);
        let serial = groups
            .iter()
            .find(|g| g.source == GroupingSource::Serial)
            .unwrap();
        assert_eq!(serial.key.to_string(), "054c:0ce6:abc");
        assert_eq!(serial.endpoints.len(), 2);
        let prefixes: Vec<_> = groups
            .iter()
            .filter(|g| g.source == GroupingSource::InstancePrefix)
            .map(|g| (g.key.serial.clone(), g.endpoints.len()))
            .collect();
        assert_eq!(prefixes, vec![("8-5-0".to_string(), 2), ("8-6-0".to_string(), 1)]);
    }

    #[test]
    fn bluetooth_radio_is_not_a_device_parent() {
        // Two Bluetooth HID devices whose first USB ancestor is the same radio.
        let radio = r"USB\VID_0B05&PID_17CB\000000000001";
        let mk = |path: &str, vid: u16, pid: u16, serial: &str| {
            let mut e = ep(path, vid, pid, serial, 1, 5, Some(radio));
            e.usb_parent = own_usb_parent(e.usb_parent.clone(), vid, pid);
            e
        };
        let a = mk(
            r"\\?\hid#{00001124-0000-1000-8000-00805f9b34fb}_vid&0002054c_pid&0ce6#9&a&0&0000#{g}",
            0x054c,
            0x0ce6,
            "a0:b1:c2:d3:e4:f5",
        );
        let b = mk(
            r"\\?\hid#{00001124-0000-1000-8000-00805f9b34fb}_vid&0002046d_pid&b01a#9&b&0&0000#{g}",
            0x046d,
            0xb01a,
            "11:22:33:44:55:66",
        );
        assert!(
            a.usb_parent.as_ref().unwrap_err().contains("0b05:17cb"),
            "{:?}",
            a.usb_parent
        );
        let groups = group_endpoints(vec![a, b]);
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|g| g.source == GroupingSource::Serial));
        assert_eq!(groups[0].key.to_string(), "046d:b01a:11:22:33:44:55:66");
        assert_eq!(groups[1].key.to_string(), "054c:0ce6:a0:b1:c2:d3:e4:f5");
    }

    #[test]
    fn own_usb_parent_accepts_matching_vid_pid_only() {
        let p = r"USB\VID_1532&PID_007C\000000000000".to_string();
        assert_eq!(own_usb_parent(Ok(p.clone()), 0x1532, 0x007c), Ok(p.clone()));
        assert!(own_usb_parent(Ok(p), 0x1532, 0x007d).is_err());
        assert_eq!(own_usb_parent(Err("x".into()), 1, 2), Err("x".to_string()));
        assert!(own_usb_parent(Ok(r"USB\ROOT_HUB30\4&1&0&0".into()), 1, 2).is_err());
    }

    #[test]
    fn parent_wins_over_serial_string() {
        // HID-reported serial differs in case from the parent instance id; the
        // parent decides identity and the key mirrors the instance segment.
        let parent = r"USB\VID_1E71&PID_2007\00000000001A";
        let groups = group_endpoints(vec![ep(
            r"\\?\hid#vid_1e71&pid_2007#7&c&0&0000#{g}",
            0x1e71,
            0x2007,
            "00000000001a",
            0xff00,
            1,
            Some(parent),
        )]);
        assert_eq!(groups[0].key.to_string(), "1e71:2007:00000000001A");
    }
}
