// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Windows HID device-interface path parsing.
//!
//! A hidapi path on Windows looks like
//! `\\?\HID#VID_1532&PID_007C&MI_03&Col02#8&2a1b3c4d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}`.
//!
//! * `MI_xx` is the USB interface number in **hex** (`MI_0A` = 10); absent on
//!   single-interface devices → `interface_number = -1`, exactly what
//!   SignalRGB plugins test with `endpoint.interface === -1`.
//! * `ColNN` is Windows' 1-based top-level-collection index in **decimal**;
//!   absent when the interface has a single TLC. HueDeck reports it 0-based
//!   (`Col01` → 0) so plugin templates comparing `collection === 0` hold.
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
    /// `ColNN - 1`, `0` when absent.
    pub collection: u16,
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
    let collection = lower
        .find("&col")
        .and_then(|i| lower.get(i + 4..i + 6))
        .filter(|d| d.chars().all(|c| c.is_ascii_digit()))
        .and_then(|d| d.parse::<u16>().ok())
        .map(|n| n.saturating_sub(1))
        .unwrap_or(0);
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
        instance_id,
    }
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
        assert_eq!(p.collection, 1);
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
    }

    #[test]
    fn hex_interface_and_decimal_collection() {
        let p = parse_hid_path(r"\\?\HID#VID_046D&PID_C539&MI_0A&Col10#9&abc&0&0009#{guid}");
        assert_eq!(p.interface_number, 10);
        assert_eq!(p.collection, 9);
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
