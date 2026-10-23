// SPDX-License-Identifier: GPL-3.0-or-later
//! Splitting a hidraw node into top-level collections (TLCs) and grouping
//! them into physical devices, so plugins see the Windows tuples.
//!
//! Linux has one hidraw node per USB HID interface, holding every TLC of that
//! interface; Windows HIDClass makes one device-interface path per TLC
//! (`…&MI_01&Col02`). The rules here reproduce the Windows tuple
//! (docs/research/m7-linux-backend.md §1.4):
//!
//! * **collection**: TLCs in descriptor order; `0` when the interface has one
//!   TLC (no `ColNN` on Windows), otherwise `1..=N` (`ColNN`, 1-based);
//! * **interface**: `bInterfaceNumber` for USB (a non-composite device gives
//!   `0`, as on Windows), `-1` for other buses
//!   ([`huedeck_hal_windows::path::endpoint_interface`], shared with Windows);
//! * **usage / usage page**: the TLC's, from `huedeck-hid-desc`;
//! * **report lengths**: from the descriptor, including the report-id byte.
//!
//! Pure logic: runs on every OS so the parity fixtures run in Windows CI too.
//!
//! Endpoint paths are `"/dev/hidrawN#tlcI"` with `I` the 0-based TLC index;
//! [`parse_endpoint_path`] also accepts a bare `/dev/hidrawN` (TLC 0).

use crate::sysfs::HidrawNode;
use huedeck_hal::hid::{HidEndpointInfo, HidEndpointTuple, ReportLengths, ReportSpec};
use huedeck_hal_windows::path::endpoint_interface;
use huedeck_hal_windows::{group_endpoints, DeviceGroup, ProbedEndpoint};
use huedeck_hid_desc::{ReportDescriptor, ReportKind, TopLevelCollection};
use std::collections::BTreeMap;

/// The Windows `ColNN` number of TLC `index` (0-based) among `count`: `0` for
/// a single-TLC interface, else `index + 1`.
pub fn collection_number(index: u16, count: usize) -> u16 {
    if count <= 1 {
        0
    } else {
        index + 1
    }
}

/// `"/dev/hidraw3#tlc1"`.
pub fn endpoint_path(devnode: &str, index: u16) -> String {
    format!("{devnode}#tlc{index}")
}

/// `(devnode, tlc index)` from an endpoint path; a bare devnode is TLC 0.
pub fn parse_endpoint_path(path: &str) -> Option<(&str, u16)> {
    let (dev, index) = match path.split_once("#tlc") {
        Some((dev, i)) => (dev, i.parse().ok()?),
        None => (path, 0),
    };
    (dev.starts_with("/dev/") && dev.len() > 5).then_some((dev, index))
}

/// What the handle of one TLC needs to know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tlc {
    /// 0-based index in the descriptor.
    pub index: u16,
    /// Usage page.
    pub usage_page: u16,
    /// Usage.
    pub usage: u16,
    /// Windows `ColNN` number (0 for a single-TLC interface).
    pub collection: u16,
    /// Longest reports, incl. the report-id byte.
    pub lengths: ReportLengths,
    /// Feature reports, ascending by id.
    pub feature_reports: Vec<ReportSpec>,
    /// Report ids of the TLC's input reports (empty when unnumbered).
    pub input_ids: Vec<u8>,
    /// Report ids of every report in the TLC.
    pub report_ids: Vec<u8>,
    /// Whether the TLC uses report ids.
    pub numbered: bool,
    /// How many TLCs the interface has.
    pub siblings: usize,
}

fn tlc_from(c: &TopLevelCollection, count: usize) -> Tlc {
    let mut feature_reports: Vec<ReportSpec> = c
        .reports
        .iter()
        .filter(|r| r.kind == ReportKind::Feature)
        .map(|r| ReportSpec {
            id: r.id,
            len: r.byte_length(),
        })
        .collect();
    feature_reports.sort_by_key(|r| r.id);
    feature_reports.dedup_by_key(|r| r.id);
    let mut input_ids: Vec<u8> = c
        .reports
        .iter()
        .filter(|r| r.kind == ReportKind::Input && r.id != 0)
        .map(|r| r.id)
        .collect();
    input_ids.sort_unstable();
    input_ids.dedup();
    Tlc {
        index: c.index,
        usage_page: c.usage_page,
        usage: c.usage,
        collection: collection_number(c.index, count),
        lengths: ReportLengths {
            input: c.input_len,
            output: c.output_len,
            feature: c.feature_len,
        },
        feature_reports,
        input_ids,
        report_ids: c.report_ids.clone(),
        numbered: c.uses_report_ids(),
        siblings: count,
    }
}

/// The TLCs of a parsed descriptor.
pub fn tlcs(desc: &ReportDescriptor) -> Vec<Tlc> {
    let n = desc.collections.len();
    desc.collections.iter().map(|c| tlc_from(c, n)).collect()
}

/// The tuples `Validate(endpoint)` sees for an interface with this descriptor.
pub fn tuples(
    descriptor: &[u8],
    interface: i32,
) -> Result<Vec<HidEndpointTuple>, huedeck_hid_desc::DescError> {
    let desc = huedeck_hid_desc::parse(descriptor)?;
    Ok(tlcs(&desc)
        .into_iter()
        .map(|t| HidEndpointTuple {
            interface,
            usage: t.usage,
            usage_page: t.usage_page,
            collection: t.collection,
        })
        .collect())
}

/// The tuple `interface` of a node.
pub fn interface_of(node: &HidrawNode) -> i32 {
    let mi = match &node.usb {
        Some(u) => u.interface,
        None if node.is_usb() => crate::sysfs::interface_from_phys(&node.hid.phys).unwrap_or(-1),
        None => -1,
    };
    endpoint_interface(mi, node.is_usb())
}

/// One endpoint per TLC of `node`. A descriptor that does not parse (or has
/// no collection) gives one endpoint with usage 0/0 and the reason in `caps`.
pub fn endpoints(node: &HidrawNode) -> Vec<ProbedEndpoint> {
    let (vid, pid) = match &node.usb {
        Some(u) => (u.vid, u.pid),
        None => (node.hid.vid, node.hid.pid),
    };
    let (manufacturer, product) = match &node.usb {
        Some(u) => (u.manufacturer.clone(), u.product.clone()),
        None => (String::new(), node.hid.name.clone()),
    };
    let base = HidEndpointInfo {
        vid,
        pid,
        serial: node.hid.uniq.clone(),
        manufacturer,
        product,
        path: endpoint_path(&node.devnode, 0),
        interface_number: interface_of(node),
        ..Default::default()
    };
    let parsed = huedeck_hid_desc::parse(&node.descriptor)
        .map_err(|e| format!("report descriptor not parsed: {e}"))
        .and_then(|d| {
            if d.collections.is_empty() {
                Err("report descriptor has no collection".to_string())
            } else {
                Ok(d)
            }
        });
    match parsed {
        Ok(desc) => tlcs(&desc)
            .into_iter()
            .map(|t| {
                let info = HidEndpointInfo {
                    path: endpoint_path(&node.devnode, t.index),
                    usage_page: t.usage_page,
                    usage: t.usage,
                    collection: t.collection,
                    report_lengths: t.lengths,
                    feature_reports: t.feature_reports.clone(),
                    ..base.clone()
                };
                let mut ep = ProbedEndpoint::from_info(info);
                ep.collection_raw = (t.collection != 0).then_some(t.collection);
                ep.caps = Ok(t.lengths);
                ep
            })
            .collect(),
        Err(reason) => {
            let mut ep = ProbedEndpoint::from_info(base);
            ep.collection_raw = None;
            ep.caps = Err(reason);
            vec![ep]
        }
    }
}

/// Group nodes into physical devices with the Windows rules
/// ([`huedeck_hal_windows::group`]): every interface under one USB device is
/// one device. Its SignalRGB-shaped key serial is the USB serial when it is
/// unique among the present devices with that VID:PID (Windows uses the
/// serial as the instance id then), else the port chain (`1-4.2`; Windows
/// would generate an id such as `6&00000000&0&10`, which Linux cannot
/// reproduce). The `usb_parent` shown is `USB\VID_xxxx&PID_xxxx\<that id>`.
/// Non-USB nodes group by `HID_UNIQ` (e.g. a Bluetooth address).
pub fn group(nodes: &[HidrawNode]) -> Vec<DeviceGroup> {
    // Serial usage count per (vid, pid), counting each USB device once.
    let mut seen: BTreeMap<&std::path::Path, (u16, u16, &str)> = BTreeMap::new();
    for u in nodes.iter().filter_map(|n| n.usb.as_ref()) {
        seen.insert(&u.sysfs, (u.vid, u.pid, &u.serial));
    }
    let mut serial_count: BTreeMap<(u16, u16, &str), usize> = BTreeMap::new();
    for v in seen.values() {
        *serial_count.entry(*v).or_default() += 1;
    }
    let mut probed = Vec::new();
    for node in nodes {
        let parent = match &node.usb {
            Some(u) => {
                let unique = !u.serial.is_empty()
                    && !u.serial.contains('\\')
                    && serial_count.get(&(u.vid, u.pid, u.serial.as_str())) == Some(&1);
                let id = if unique {
                    u.serial.as_str()
                } else {
                    u.port.as_str()
                };
                Ok(format!("USB\\VID_{:04X}&PID_{:04X}\\{id}", u.vid, u.pid))
            }
            None => Err(format!("not a USB device (bus 0x{:04x})", node.hid.bus)),
        };
        for mut ep in endpoints(node) {
            ep.usb_parent = parent.clone();
            probed.push(ep);
        }
    }
    group_endpoints(probed)
}

/// `data` padded with zeros or truncated to `len` bytes (the TLC's longest
/// report of that kind), as hidapi windows-native does for `WriteFile` and
/// `HidD_SetFeature`; unchanged when `len` is unknown (0).
pub fn fit_report(data: &[u8], len: usize) -> std::borrow::Cow<'_, [u8]> {
    if len == 0 || data.len() == len {
        return std::borrow::Cow::Borrowed(data);
    }
    let mut v = data[..data.len().min(len)].to_vec();
    v.resize(len, 0);
    std::borrow::Cow::Owned(v)
}

impl Tlc {
    /// Whether an input report read from the shared hidraw node belongs to
    /// this TLC: every report when the interface has one TLC (or the TLC is
    /// unnumbered), else only the TLC's input report ids.
    pub fn accepts_input(&self, report: &[u8]) -> bool {
        if self.siblings <= 1 || !self.numbered {
            return true;
        }
        report.first().is_some_and(|id| self.input_ids.contains(id))
    }
}

/// Copy an input report read from hidraw into `buf` in the HAL convention:
/// unnumbered reports get a leading `0x00` (hidraw adds none). Returns the
/// bytes stored.
pub fn place_input(numbered: bool, report: &[u8], buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    if numbered {
        let n = report.len().min(buf.len());
        buf[..n].copy_from_slice(&report[..n]);
        n
    } else {
        buf[0] = 0;
        let n = report.len().min(buf.len() - 1);
        buf[1..=n].copy_from_slice(&report[..n]);
        n + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::{HidUevent, UsbParent, BUS_BLUETOOTH, BUS_USB};

    pub(crate) const VENDOR_64: &[u8] = &[
        0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x02, 0x09, 0x02, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75,
        0x08, 0x95, 0x40, 0x81, 0x02, 0x09, 0x03, 0x91, 0x02, 0x85, 0x03, 0x09, 0x04, 0x95, 0x20, 0xB1, 0x02,
        0xC0,
    ];

    fn node(name: &str, port: &str, iface: i32, serial: &str, desc: &[u8]) -> HidrawNode {
        HidrawNode {
            name: name.into(),
            devnode: format!("/dev/{name}"),
            hid: HidUevent {
                bus: BUS_USB,
                vid: 0x1e71,
                pid: 0x2007,
                uniq: serial.into(),
                ..Default::default()
            },
            descriptor: desc.to_vec(),
            usb: Some(UsbParent {
                sysfs: format!("/sys/devices/usb1/{port}").into(),
                port: port.into(),
                interface: iface,
                vid: 0x1e71,
                pid: 0x2007,
                serial: serial.into(),
                manufacturer: "NZXT".into(),
                product: "Kraken".into(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn paths_round_trip() {
        assert_eq!(endpoint_path("/dev/hidraw3", 2), "/dev/hidraw3#tlc2");
        assert_eq!(
            parse_endpoint_path("/dev/hidraw3#tlc2"),
            Some(("/dev/hidraw3", 2))
        );
        assert_eq!(parse_endpoint_path("/dev/hidraw3"), Some(("/dev/hidraw3", 0)));
        assert_eq!(parse_endpoint_path("/dev/hidraw3#tlcx"), None);
        assert_eq!(parse_endpoint_path(r"\\?\hid#vid_1"), None);
    }

    #[test]
    fn collection_numbers() {
        assert_eq!(collection_number(0, 1), 0);
        assert_eq!(collection_number(0, 2), 1);
        assert_eq!(collection_number(9, 12), 10);
    }

    #[test]
    fn single_tlc_endpoint_has_lengths_and_feature_reports() {
        let eps = endpoints(&node("hidraw2", "1-4", 0, "S", VENDOR_64));
        assert_eq!(eps.len(), 1);
        let i = &eps[0].info;
        assert_eq!(i.path, "/dev/hidraw2#tlc0");
        assert_eq!(
            (i.interface_number, i.usage_page, i.usage, i.collection),
            (0, 0xff00, 1, 0)
        );
        assert_eq!(
            i.report_lengths,
            ReportLengths {
                input: 65,
                output: 65,
                feature: 33
            }
        );
        assert_eq!(i.feature_reports, vec![ReportSpec { id: 3, len: 33 }]);
        assert_eq!(eps[0].collection_raw, None);
        assert_eq!(eps[0].caps, Ok(i.report_lengths));
    }

    #[test]
    fn unparsable_descriptor_gives_one_unknown_endpoint() {
        let eps = endpoints(&node("hidraw2", "1-4", 1, "", &[0xA1, 0x01]));
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].info.interface_number, 1);
        assert!(eps[0].caps.as_ref().unwrap_err().contains("not parsed"));
    }

    #[test]
    fn interfaces_of_one_usb_device_group_together() {
        let groups = group(&[
            node("hidraw4", "1-4", 1, "00000000001A", VENDOR_64),
            node("hidraw3", "1-4", 0, "00000000001A", VENDOR_64),
        ]);
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.key.to_string(), "1e71:2007:00000000001A");
        assert_eq!(
            g.usb_parent.as_deref(),
            Some(r"USB\VID_1E71&PID_2007\00000000001A")
        );
        let ifaces: Vec<i32> = g.endpoints.iter().map(|e| e.info.interface_number).collect();
        assert_eq!(ifaces, vec![0, 1]);
    }

    #[test]
    fn duplicate_or_missing_serials_fall_back_to_the_port() {
        let groups = group(&[
            node("hidraw1", "1-4", 0, "000000000000", VENDOR_64),
            node("hidraw2", "1-5.2", 0, "000000000000", VENDOR_64),
            node("hidraw3", "2-1", 0, "", VENDOR_64),
        ]);
        let keys: Vec<String> = groups.iter().map(|g| g.key.to_string()).collect();
        assert_eq!(keys, vec!["1e71:2007:1-4", "1e71:2007:1-5.2", "1e71:2007:2-1"]);
    }

    #[test]
    fn bluetooth_groups_by_uniq_with_interface_minus_one() {
        let bt = HidrawNode {
            name: "hidraw9".into(),
            devnode: "/dev/hidraw9".into(),
            hid: HidUevent {
                bus: BUS_BLUETOOTH,
                vid: 0x054c,
                pid: 0x0ce6,
                name: "DualSense Wireless Controller".into(),
                uniq: "a0:b1:c2:d3:e4:f5".into(),
                ..Default::default()
            },
            descriptor: VENDOR_64.to_vec(),
            ..Default::default()
        };
        let groups = group(&[bt]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key.to_string(), "054c:0ce6:a0:b1:c2:d3:e4:f5");
        assert_eq!(groups[0].product, "DualSense Wireless Controller");
        assert_eq!(groups[0].endpoints[0].info.interface_number, -1);
    }

    #[test]
    fn reports_are_fitted_to_the_tlc_length() {
        assert_eq!(&*fit_report(&[1, 2], 4), &[1, 2, 0, 0]);
        assert_eq!(&*fit_report(&[1, 2, 3, 4, 5], 4), &[1, 2, 3, 4]);
        assert_eq!(&*fit_report(&[1, 2, 3], 0), &[1, 2, 3]);
    }

    #[test]
    fn input_demultiplexing_and_placement() {
        let mut bytes = crate::fixtures::KEYBOARD.to_vec();
        bytes.extend_from_slice(crate::fixtures::CONSUMER_ID5);
        let d = huedeck_hid_desc::parse(&bytes).unwrap();
        let t = tlcs(&d);
        // The keyboard TLC is unnumbered: it takes everything.
        assert!(t[0].accepts_input(&[5, 1, 0]));
        assert!(t[1].accepts_input(&[5, 1, 0]));
        assert!(!t[1].accepts_input(&[1, 0, 0]));
        assert!(!t[1].accepts_input(&[]));
        let mut buf = [0xEEu8; 4];
        assert_eq!(place_input(false, &[7, 8, 9, 10], &mut buf), 4);
        assert_eq!(buf, [0, 7, 8, 9]);
        assert_eq!(place_input(true, &[5, 1], &mut buf), 2);
        assert_eq!(&buf[..2], &[5, 1]);
        assert_eq!(place_input(true, &[5, 1], &mut []), 0);
    }
}
