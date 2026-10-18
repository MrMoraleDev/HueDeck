// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Windows HID device-interface path parsing.
//!
//! A hidapi path on Windows looks like
//! `\\?\HID#VID_1532&PID_007C&MI_03&Col02#8&2a1b3c4d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}`.
//!
//! * `MI_xx` is the USB interface number in **hex** (`MI_0A` = 10); absent on
//!   single-interface devices → this parser returns `-1`. The *endpoint
//!   tuple* does not use that `-1` for USB devices: hidapi (and SignalRGB,
//!   whose MSI/NZXT plugins validate `interface === 0` on MI-less paths on the
//!   reference machine) reports `0` for a non-composite USB device; `-1`
//!   survives only for non-USB transports (e.g. Bluetooth HID).
//! * `ColNN` is Windows' 1-based top-level-collection number written as **two
//!   hex digits** (`Col0A` = 10th TLC): Microsoft's "HIDClass Hardware IDs for
//!   Top-Level Collections" defines `&Colb(2)` with `b(2)` = "two hex digits",
//!   and `hidclass.sys` formats it with `Col%02X`. It is absent when the
//!   interface has a single TLC. The SignalRGB `endpoint.collection` value is
//!   that number (`Col05` → 5) and `0` when absent. Evidence on the reference
//!   machine: the Sonix 320F:5064 keyboard exposes its vendor TLC (usage page
//!   0xFF1C / usage 0x0092) as `MI_01&Col04`, and the EVision (Sonix-family)
//!   plugin library matches exactly `{interface 1, usage 0x0092, usage_page
//!   0xFF1C, collection 4}` on all four fields; `Razer_Modern_Mouse.js`
//!   switches to `set_endpoint(1, 0x0000, 0x0001, 0x0005)` and the DeathAdder
//!   V2 Pro has two usage-0 TLCs on MI_01, `Col04` and `Col05` (no sixth, so a
//!   0-based reading would point at nothing); the commented Lenovo Legion 7
//!   tuples enumerate collections 1..=7 with no 0.
//! * The middle `#…#` segment is the device instance id tail
//!   (`8&2a1b3c4d&0&0000`); everything before its last `&` is shared by all
//!   TLCs of one HID interface and is used as a grouping hint when the device
//!   reports no serial number.

/// Result of [`parse_hid_path`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedHidPath {
    /// `VID_xxxx`.
    pub vid: Option<u16>,
    /// `PID_xxxx`.
    pub pid: Option<u16>,
    /// `MI_xx` as a number, `-1` when absent.
    pub interface_number: i32,
    /// SignalRGB `endpoint.collection`: the `ColNN` number, `0` when absent.
    pub collection: u16,
    /// `ColNN` from the path, hex-decoded (1-based; `Col0A` → 10), `None` when absent.
    pub collection_raw: Option<u16>,
    /// Instance-id tail (`8&2a1b3c4d&0&0000`), when the path has one.
    pub instance_id: Option<String>,
}

impl ParsedHidPath {
    /// Instance id without its trailing `&NNNN` collection suffix; equal for
    /// all TLCs of one interface.
    pub fn instance_prefix(&self) -> Option<&str> {
        let id = self.instance_id.as_deref()?;
        Some(id.rsplit_once('&').map(|(head, _)| head).unwrap_or(id))
    }
}

fn hex_after<'a>(lower: &'a str, marker: &str, digits: usize) -> Option<&'a str> {
    let start = lower.find(marker)? + marker.len();
    let candidate = lower.get(start..start + digits)?;
    candidate
        .chars()
        .all(|c| c.is_ascii_hexdigit())
        .then_some(candidate)
}

/// Parse a Windows HID interface path. Tolerates missing parts; never panics.
pub fn parse_hid_path(path: &str) -> ParsedHidPath {
    let lower = path.to_ascii_lowercase();
    let vid = hex_after(&lower, "vid_", 4).and_then(|h| u16::from_str_radix(h, 16).ok());
    let pid = hex_after(&lower, "pid_", 4).and_then(|h| u16::from_str_radix(h, 16).ok());
    let interface_number = hex_after(&lower, "&mi_", 2)
        .and_then(|h| i32::from_str_radix(h, 16).ok())
        .unwrap_or(-1);
    // Two hex digits (`Col0A` is the 10th TLC), never decimal: a decimal-only
    // parse would turn `Col0A` into "no collection" (0).
    let collection_raw = hex_after(&lower, "&col", 2).and_then(|h| u16::from_str_radix(h, 16).ok());
    let collection = collection_raw.unwrap_or(0);
    let instance_id = {
        let mut parts = path.split('#');
        parts.next(); // "\\?\"
        parts.next(); // "HID#VID_...&Col02"
        parts.next().filter(|s| !s.is_empty()).map(str::to_string)
    };
    ParsedHidPath {
        vid,
        pid,
        interface_number,
        collection,
        collection_raw,
        instance_id,
    }
}

/// Convert a HID device-interface path into the device instance id that
/// cfgmgr32 (`CM_Locate_DevNodeW`) understands:
/// `\\?\HID#VID_1532&PID_007C&MI_00#8&2a1b&0&0000#{guid}` →
/// `HID\VID_1532&PID_007C&MI_00\8&2A1B&0&0000`.
///
/// Instance ids are case-insensitive and Windows stores them upper-case.
pub fn interface_path_to_instance_id(path: &str) -> Option<String> {
    let rest = path
        .strip_prefix(r"\\?\")
        .or_else(|| path.strip_prefix(r"\\.\"))
        .unwrap_or(path);
    let mut parts: Vec<&str> = rest.split('#').collect();
    // Drop the trailing interface-class GUID segment.
    if parts.last().is_some_and(|p| p.starts_with('{')) {
        parts.pop();
    }
    if parts.len() < 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    Some(parts.join(BACKSLASH_STR).to_ascii_uppercase())
}

/// `\` as a char / str (kept as named constants for readability).
const BACKSLASH: char = '\u{5c}';
const BACKSLASH_STR: &str = "\u{5c}";

/// `true` for a USB *device* instance id (`USB\VID_xxxx&PID_xxxx\<instance>`),
/// `false` for a USB *interface* of a composite device (`...&MI_xx\...`), for
/// an XUSB interface-group node (`...&IG_xx\...`, which hidapi also skips) and
/// for every other bus.
pub fn is_usb_device_instance(id: &str) -> bool {
    let upper = id.to_ascii_uppercase();
    let mut parts = upper.split(BACKSLASH);
    let (Some(bus), Some(hw), Some(inst)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    bus == "USB" && hw.starts_with("VID_") && !hw.contains("&MI_") && !hw.contains("&IG_") && !inst.is_empty()
}

/// `(vid, pid)` from a `...VID_xxxx&PID_xxxx...` instance id (case-insensitive).
pub fn usb_instance_vid_pid(id: &str) -> Option<(u16, u16)> {
    let lower = id.to_ascii_lowercase();
    let vid = hex_after(&lower, "vid_", 4).and_then(|h| u16::from_str_radix(h, 16).ok())?;
    let pid = hex_after(&lower, "pid_", 4).and_then(|h| u16::from_str_radix(h, 16).ok())?;
    Some((vid, pid))
}

/// Serial component of a SignalRGB device key derived from the USB parent
/// instance id: its last `\` segment with every `&` replaced by `-`.
///
/// Windows uses the USB iSerialNumber as that segment when the device has a
/// usable serial (`USB\VID_1462&PID_7C91\A00000000001` → `A00000000001`) and
/// a generated id otherwise (`USB\VID_054C&PID_0CE6\6&00000000&0&10` →
/// `6-00000000-0-10`), which is exactly the key SignalRGB logs.
pub fn signalrgb_serial_from_instance(id: &str) -> Option<String> {
    let (_, last) = id.rsplit_once(BACKSLASH)?;
    (!last.is_empty()).then(|| last.replace('&', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_device_with_collection() {
        let p = parse_hid_path(
            r"\\?\HID#VID_1532&PID_007C&MI_03&Col02#8&2a1b3c4d&0&0001#{4d1e55b2-f16f-11cf-88cb-001111000030}",
        );
        assert_eq!(p.vid, Some(0x1532));
        assert_eq!(p.pid, Some(0x007C));
        assert_eq!(p.interface_number, 3);
        assert_eq!(p.collection, 2);
        assert_eq!(p.collection_raw, Some(2));
        assert_eq!(p.instance_id.as_deref(), Some("8&2a1b3c4d&0&0001"));
        assert_eq!(p.instance_prefix(), Some("8&2a1b3c4d&0"));
    }

    #[test]
    fn single_interface_device_reports_minus_one() {
        let p = parse_hid_path(
            r"\\?\hid#vid_1462&pid_7c91#7&1f2e3d4c&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}",
        );
        assert_eq!((p.vid, p.pid), (Some(0x1462), Some(0x7C91)));
        assert_eq!(p.interface_number, -1);
        assert_eq!(p.collection, 0);
        assert_eq!(p.collection_raw, None);
    }

    #[test]
    fn col01_is_one_and_mi00_is_zero() {
        let p = parse_hid_path(
            r"\\?\hid#vid_1e71&pid_2007&mi_00&col01#8&aa&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}",
        );
        assert_eq!(p.interface_number, 0);
        assert_eq!(p.collection_raw, Some(1));
        assert_eq!(p.collection, 1);
    }

    #[test]
    fn interface_path_to_instance_id_round_trip() {
        assert_eq!(
            interface_path_to_instance_id(
                r"\\?\hid#vid_1532&pid_007c&mi_03&col02#8&2a1b3c4d&0&0001#{4d1e55b2-f16f-11cf-88cb-001111000030}"
            )
            .as_deref(),
            Some(r"HID\VID_1532&PID_007C&MI_03&COL02\8&2A1B3C4D&0&0001")
        );
        assert_eq!(
            interface_path_to_instance_id(r"\\?\hid#vid_1462&pid_7c91#7&1f2e&0&0000#{guid}").as_deref(),
            Some(r"HID\VID_1462&PID_7C91\7&1F2E&0&0000")
        );
        // Keyboard collections carry a `\KBD` suffix after the class GUID.
        assert_eq!(
            interface_path_to_instance_id(
                r"\\?\HID#VID_320F&PID_5064&MI_01&Col01#9&327435e8&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}\KBD"
            )
            .as_deref(),
            Some(r"HID\VID_320F&PID_5064&MI_01&COL01\9&327435E8&0&0000")
        );
        let p = parse_hid_path(
            r"\\?\HID#VID_320F&PID_5064&MI_01&Col01#9&327435e8&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}\KBD",
        );
        assert_eq!((p.interface_number, p.collection), (1, 1));
        assert_eq!(interface_path_to_instance_id(""), None);
        assert_eq!(interface_path_to_instance_id(r"\\?\hid##{guid}"), None);
    }

    #[test]
    fn usb_device_instance_detection() {
        assert!(is_usb_device_instance(r"USB\VID_1462&PID_7C91\A00000000001"));
        assert!(is_usb_device_instance(r"usb\vid_054c&pid_0ce6\6&28cf390b&0&10"));
        assert!(!is_usb_device_instance(r"USB\VID_1532&PID_007C&MI_00\7&1&0&0000"));
        assert!(!is_usb_device_instance(r"USB\VID_045E&PID_028E&IG_00\6&1&0&00"));
        assert!(!is_usb_device_instance(r"HID\VID_1532&PID_007C&MI_00\8&1&0&0000"));
        assert!(!is_usb_device_instance(r"USB\ROOT_HUB30\4&1&0&0"));
        assert!(!is_usb_device_instance("USB"));
    }

    #[test]
    fn signalrgb_serial_matches_logged_keys() {
        // Keys observed in SignalRGB logs on the reference machine.
        assert_eq!(
            signalrgb_serial_from_instance(r"USB\VID_1462&PID_7C91\A00000000001").as_deref(),
            Some("A00000000001")
        );
        assert_eq!(
            signalrgb_serial_from_instance(r"USB\VID_054C&PID_0CE6\6&00000000&0&10").as_deref(),
            Some("6-00000000-0-10")
        );
        assert_eq!(signalrgb_serial_from_instance("nobackslash"), None);
        assert_eq!(signalrgb_serial_from_instance(r"USB\VID_1&PID_2\"), None);
    }

    #[test]
    fn hex_interface_and_hex_collection() {
        // `Colb(2)` is two hex digits (Microsoft hardware-id spec, hidclass `Col%02X`).
        let p = parse_hid_path(r"\\?\HID#VID_046D&PID_C539&MI_0A&Col0A#9&abc&0&0009#{guid}");
        assert_eq!(p.interface_number, 10);
        assert_eq!(
            p.collection_raw,
            Some(10),
            "10th TLC must not collapse to 'no collection'"
        );
        assert_eq!(p.collection, 10);
        let p = parse_hid_path(r"\\?\hid#vid_046d&pid_c539&mi_02&col10#9&abc&0&000f#{guid}");
        assert_eq!(
            (p.interface_number, p.collection_raw, p.collection),
            (2, Some(16), 16)
        );
    }

    #[test]
    fn tuples_match_plugin_endpoints_seen_on_reference_hardware() {
        use lightloom_hal::hid::HidEndpointInfo;
        let tuple = |path: &str, usage_page: u16, usage: u16| {
            let p = parse_hid_path(path);
            HidEndpointInfo {
                interface_number: p.interface_number,
                usage_page,
                usage,
                collection: p.collection,
                path: path.into(),
                ..Default::default()
            }
            .endpoint_tuple()
        };
        // Sonix 320F:5064, vendor TLC 0xFF1C/0x0092 at MI_01&Col04: the EVision (Sonix family)
        // library entry {interface 1, usage 0x0092, usage_page 0xFF1C, collection 4} must match.
        let sonix = tuple(
            r"\\?\HID#VID_320F&PID_5064&MI_01&Col04#9&327435e8&0&0003#{4d1e55b2-f16f-11cf-88cb-001111000030}",
            0xFF1C,
            0x0092,
        );
        assert!(sonix.matches(1, 0x0092, 0xFF1C, Some(4)));
        assert!(!sonix.matches(1, 0x0092, 0xFF1C, Some(3)));
        // Razer DeathAdder V2 Pro: set_endpoint(1, 0x0000, 0x0001, 5) selects MI_01&Col05, while the
        // 3-argument form also accepts MI_01&Col04 (same interface/usage/page).
        let col05 = tuple(
            r"\\?\HID#VID_1532&PID_007C&MI_01&Col05#8&3b4f7e3e&0&0004#{4d1e55b2-f16f-11cf-88cb-001111000030}",
            0x0001,
            0x0000,
        );
        let col04 = tuple(
            r"\\?\HID#VID_1532&PID_007C&MI_01&Col04#8&3b4f7e3e&0&0003#{4d1e55b2-f16f-11cf-88cb-001111000030}",
            0x0001,
            0x0000,
        );
        assert!(col05.matches(1, 0x0000, 0x0001, Some(5)));
        assert!(!col04.matches(1, 0x0000, 0x0001, Some(5)));
        assert!(col04.matches(1, 0x0000, 0x0001, None) && col05.matches(1, 0x0000, 0x0001, None));
        // Single-TLC interface: collection 0 (Diamondback-style `interface 0 && collection 0`).
        let mouse = tuple(
            r"\\?\HID#VID_1532&PID_007C&MI_00#8&1778407c&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}",
            0x0001,
            0x0002,
        );
        assert!(mouse.matches(0, 0x0002, 0x0001, Some(0)));
    }

    #[test]
    fn usb_instance_vid_pid_parses_device_and_interface_ids() {
        assert_eq!(
            usb_instance_vid_pid(r"USB\VID_1462&PID_7C91\A00000000001"),
            Some((0x1462, 0x7C91))
        );
        assert_eq!(
            usb_instance_vid_pid(r"usb\vid_0b05&pid_17cb\000000000001"),
            Some((0x0B05, 0x17CB))
        );
        assert_eq!(usb_instance_vid_pid(r"USB\ROOT_HUB30\5&5c2d53e&0&0"), None);
        assert_eq!(usb_instance_vid_pid(r"USB\VID_12&PID_3456\x"), None);
    }

    #[test]
    fn garbage_is_tolerated() {
        let p = parse_hid_path("");
        assert_eq!(
            p,
            ParsedHidPath {
                interface_number: -1,
                ..Default::default()
            }
        );
        let p = parse_hid_path(r"\\?\HID#VID_ZZZZ&PID_12&MI_&Col#");
        assert_eq!(p.vid, None);
        assert_eq!(p.pid, None);
        assert_eq!(p.interface_number, -1);
        assert_eq!(p.collection, 0);
        assert_eq!(p.instance_id, None);
    }
}
