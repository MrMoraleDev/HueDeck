// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared tuple-parity fixtures: one report descriptor, two paths.
//!
//! * **Windows**: HIDClass makes one device-interface path per top-level
//!   collection, `HID\VID_v&PID_p[&MI_ii][&ColNN]` with `ii` and `NN` as two
//!   hex digits, `ColNN` 1-based in descriptor order and only on multi-TLC
//!   interfaces ("HIDClass Hardware IDs for Top-Level Collections"). The
//!   tuple comes from the shipped Windows logic: `parse_hid_path` for
//!   `MI_xx` / `ColNN` and `path::endpoint_interface` for the interface.
//! * **Linux**: one hidraw node per interface; [`crate::tlc`] splits the same
//!   descriptor bytes.
//!
//! Both must give identical `{interface, usage, usage_page, collection}`
//! tuples. The descriptors are written for these tests from the HID usage
//! tables in the shapes of the reference devices (no captured bytes yet:
//! recording `/sys/.../report_descriptor` on Linux next to the Windows paths
//! of the same device is the remaining hardware step).

use huedeck_hal::hid::HidEndpointTuple;
use huedeck_hal_windows::path::{endpoint_interface, parse_hid_path};

/// Boot keyboard (HID 1.11 Appendix E.6), unnumbered.
pub const KEYBOARD: &[u8] = &[
    0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75,
    0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x06, 0x75, 0x08, 0x15, 0x00,
    0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0xC0,
];

/// Consumer control (0x0C/0x01) with report id 5.
pub const CONSUMER_ID5: &[u8] = &[
    0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x05, 0x19, 0x00, 0x2A, 0x3C, 0x02, 0x15, 0x00, 0x26, 0x3C,
    0x02, 0x95, 0x01, 0x75, 0x10, 0x81, 0x00, 0xC0,
];

/// One application collection on `page`/`usage` with report `id`: an input
/// report of `input` bytes and a feature report of `feature` bytes (0 = none).
pub fn tlc(page: u16, usage: u8, id: u8, input: u8, feature: u8) -> Vec<u8> {
    let mut d = vec![0x06, page as u8, (page >> 8) as u8, 0x09, usage, 0xA1, 0x01];
    if id != 0 {
        d.extend_from_slice(&[0x85, id]);
    }
    d.extend_from_slice(&[0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08]);
    if input > 0 {
        d.extend_from_slice(&[0x09, 0x01, 0x95, input, 0x81, 0x02]);
    }
    if feature > 0 {
        d.extend_from_slice(&[0x09, 0x02, 0x95, feature, 0xB1, 0x02]);
    }
    d.push(0xC0);
    d
}

/// Sonix-family keyboard interface 1 (320F:5064 on the reference machine):
/// four TLCs, the vendor one (0xFF1C/0x92) fourth, which Windows names
/// `MI_01&Col04` and the EVision plugin matches as `{1, 0x92, 0xFF1C, 4}`.
pub fn sonix_mi01() -> Vec<u8> {
    let mut d = tlc(0x0001, 0x80, 1, 2, 0); // System Control
    d.extend(tlc(0x000C, 0x01, 2, 2, 0)); // Consumer Control
    d.extend(tlc(0x0001, 0x06, 3, 16, 0)); // NKRO keyboard
    d.extend(tlc(0xFF1C, 0x92, 4, 0, 63)); // vendor lighting
    d
}

/// MSI Mystic Light 1462:7C91 (non-composite, two TLCs: `&col01`, `&col02`).
pub fn msi_7c91() -> Vec<u8> {
    let mut d = tlc(0x0001, 0x00, 0x52, 0, 184);
    d.extend(tlc(0xFF00, 0x01, 0x01, 63, 0));
    d
}

/// An interface with `n` vendor TLCs (exercises `Col0A` and above).
pub fn many(n: u8) -> Vec<u8> {
    (1..=n).flat_map(|i| tlc(0xFF00, i, i, 8, 0)).collect()
}

/// The interface paths Windows HIDClass creates for `descriptor` on USB
/// interface `mi` (`None`: non-composite device, no `MI_xx`).
pub fn windows_paths(vid: u16, pid: u16, mi: Option<u8>, descriptor: &[u8]) -> Vec<(String, u16, u16)> {
    let desc = huedeck_hid_desc::parse(descriptor).unwrap();
    let n = desc.collections.len();
    desc.collections
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mi = mi.map(|m| format!("&MI_{m:02X}")).unwrap_or_default();
            let col = if n > 1 { format!("&Col{:02X}", i + 1) } else { String::new() };
            (
                format!(
                    r"\\?\HID#VID_{vid:04X}&PID_{pid:04X}{mi}{col}#8&1a2b3c4d&0&{i:04}#{{4d1e55b2-f16f-11cf-88cb-001111000030}}"
                ),
                c.usage_page,
                c.usage,
            )
        })
        .collect()
}

/// The tuples the Windows backends build for those paths (USB collections;
/// usage page / usage from `HIDP_CAPS`, i.e. the collection's own).
pub fn windows_tuples(paths: &[(String, u16, u16)]) -> Vec<HidEndpointTuple> {
    paths
        .iter()
        .map(|(path, page, usage)| {
            let p = parse_hid_path(path);
            HidEndpointTuple {
                interface: endpoint_interface(p.interface_number, true),
                usage: *usage,
                usage_page: *page,
                collection: p.collection,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::tests::fake_usb_hid;
    use crate::sysfs::MemFs;

    /// Linux tuples through the whole sysfs path: fake tree → node → TLCs.
    fn linux_tuples(iface: u8, ids: (u16, u16), descriptor: &[u8]) -> Vec<HidEndpointTuple> {
        let mut fs = MemFs::default();
        let hid = fake_usb_hid(&mut fs, "1-4", iface, ids, Some("SER"), 1, descriptor);
        let node = crate::sysfs::read_node(&fs, "hidraw0", &hid).unwrap();
        crate::tlc::endpoints(&node)
            .into_iter()
            .map(|e| e.info.endpoint_tuple())
            .collect()
    }

    fn assert_parity(ids: (u16, u16), mi: Option<u8>, descriptor: &[u8]) -> Vec<HidEndpointTuple> {
        let windows = windows_tuples(&windows_paths(ids.0, ids.1, mi, descriptor));
        let linux = linux_tuples(mi.unwrap_or(0), ids, descriptor);
        assert_eq!(linux, windows, "Linux and Windows tuples differ");
        assert_eq!(
            crate::tlc::tuples(descriptor, i32::from(mi.unwrap_or(0))).unwrap(),
            windows
        );
        windows
    }

    fn t(interface: i32, usage: u16, usage_page: u16, collection: u16) -> HidEndpointTuple {
        HidEndpointTuple {
            interface,
            usage,
            usage_page,
            collection,
        }
    }

    #[test]
    fn sonix_vendor_collection_is_mi01_col04() {
        let tuples = assert_parity((0x320f, 0x5064), Some(1), &sonix_mi01());
        assert_eq!(tuples[3], t(1, 0x92, 0xFF1C, 4));
        assert_eq!(tuples[0], t(1, 0x80, 0x0001, 1));
    }

    #[test]
    fn non_composite_multi_tlc_is_interface_zero() {
        let tuples = assert_parity((0x1462, 0x7c91), None, &msi_7c91());
        assert_eq!(tuples, vec![t(0, 0x00, 0x0001, 1), t(0, 0x01, 0xFF00, 2)]);
    }

    #[test]
    fn single_tlc_interface_is_collection_zero() {
        let tuples = assert_parity((0x1532, 0x007c), Some(3), &tlc(0xFF00, 0x01, 0, 90, 90));
        assert_eq!(tuples, vec![t(3, 0x01, 0xFF00, 0)]);
        assert_parity((0x1532, 0x007c), Some(0), KEYBOARD);
    }

    #[test]
    fn keyboard_plus_consumer() {
        let mut d = KEYBOARD.to_vec();
        d.extend_from_slice(CONSUMER_ID5);
        let tuples = assert_parity((0x1038, 0x161a), Some(2), &d);
        assert_eq!(tuples, vec![t(2, 0x06, 0x0001, 1), t(2, 0x01, 0x000C, 2)]);
    }

    #[test]
    fn tenth_collection_and_above_are_hex_on_windows_and_numbered_alike_on_linux() {
        let tuples = assert_parity((0x046d, 0xc539), Some(2), &many(12));
        assert!(windows_paths(0x046d, 0xc539, Some(2), &many(12))[9]
            .0
            .contains("&Col0A#"));
        assert_eq!(tuples[9].collection, 10);
        assert_eq!(tuples[11].collection, 12);
    }
}
