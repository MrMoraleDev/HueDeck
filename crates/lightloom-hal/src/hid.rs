// SPDX-License-Identifier: GPL-3.0-or-later
//! HID backend traits.
//!
//! One [`HidEndpointInfo`] per HID *top-level collection* (TLC). On Windows a
//! TLC is its own device-interface path (`…&MI_01&Col02#…`); on Linux one
//! hidraw node covers the whole interface and `lightloom-hid-desc` splits it
//! into TLCs so plugins observe identical tuples on both OSes.

use crate::error::{HalError, HalResult};
use crate::key::DeviceKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Maximum report sizes of an endpoint, in bytes, **including** the leading
/// report-id byte (the same convention as Windows `HIDP_CAPS` and hidapi).
/// `0` means the endpoint has no report of that kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportLengths {
    /// Longest input report.
    pub input: usize,
    /// Longest output report.
    pub output: usize,
    /// Longest feature report.
    pub feature: usize,
}

/// One report declared by an endpoint's report descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportSpec {
    /// Report id (`0` when the collection uses unnumbered reports).
    pub id: u8,
    /// Length in bytes **including** the report-id byte (hidapi convention).
    pub len: usize,
}

/// The `{interface, usage, usage_page, collection}` tuple a plugin's
/// `Validate(endpoint)` and `set_endpoint(...)` operate on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HidEndpointTuple {
    /// USB interface number (`bInterfaceNumber`, Windows `MI_xx`). A
    /// non-composite USB device (no `MI_xx`) reports `0`, as hidapi and
    /// SignalRGB do; `-1` only when the transport has no USB interface (e.g.
    /// Bluetooth HID).
    pub interface: i32,
    /// HID usage of the top-level collection.
    pub usage: u16,
    /// HID usage page of the top-level collection.
    pub usage_page: u16,
    /// Top-level-collection number within the interface as SignalRGB reports
    /// it: the Windows `ColNN` number (two hex digits, 1-based: `Col05` → 5,
    /// `Col0A` → 10), `0` when the interface has a single collection (no
    /// `ColNN`). See `lightloom_hal_windows::path` for the evidence.
    pub collection: u16,
}

impl HidEndpointTuple {
    /// Match rule of `device.set_endpoint(interface, usage, usage_page[, collection])`:
    /// the 3-argument form treats `collection` as a wildcard.
    pub fn matches(&self, interface: i32, usage: u16, usage_page: u16, collection: Option<u16>) -> bool {
        self.interface == interface
            && self.usage == usage
            && self.usage_page == usage_page
            && collection.is_none_or(|c| c == self.collection)
    }
}

/// Everything enumeration knows about one HID top-level collection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HidEndpointInfo {
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// USB serial string (empty when the device reports none).
    pub serial: String,
    /// USB manufacturer string.
    pub manufacturer: String,
    /// USB product string.
    pub product: String,
    /// Backend-specific opaque path handed back to [`HidBackend::open`].
    pub path: String,
    /// USB interface number, or [`HidEndpointInfo::UNKNOWN_INTERFACE`].
    pub interface_number: i32,
    /// Usage page of the top-level collection.
    pub usage_page: u16,
    /// Usage of the top-level collection.
    pub usage: u16,
    /// Top-level-collection number ([`HidEndpointTuple::collection`] semantics:
    /// the `ColNN` number, `0` when absent).
    pub collection: u16,
    /// Report sizes (may be all-zero until the endpoint is opened on
    /// backends that only learn them from the report descriptor).
    pub report_lengths: ReportLengths,
    /// Feature reports the report descriptor declares, ascending by id.
    /// Filled when the backend reads the descriptor (Windows: at open);
    /// empty when unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_reports: Vec<ReportSpec>,
}

impl HidEndpointInfo {
    /// Interface number reported when the transport has no USB interface
    /// number (e.g. Bluetooth HID). A non-composite USB device reports `0`.
    pub const UNKNOWN_INTERFACE: i32 = -1;

    /// `vid:pid:serial` built from the HID-reported serial string. On Windows
    /// that string can be empty (e.g. DualSense, Sonix keyboards) while
    /// SignalRGB keys the device by its USB parent instance id; use the
    /// backend's device grouping (`lightloom_hal_windows::group`) for the
    /// SignalRGB-shaped key.
    pub fn key(&self) -> DeviceKey {
        DeviceKey::new(self.vid, self.pid, self.serial.clone())
    }

    /// The tuple exposed to `Validate(endpoint)`.
    pub fn endpoint_tuple(&self) -> HidEndpointTuple {
        HidEndpointTuple {
            interface: self.interface_number,
            usage: self.usage,
            usage_page: self.usage_page,
            collection: self.collection,
        }
    }
}

/// Group enumerated endpoints by physical device (`vid:pid:serial`), keeping
/// enumeration order inside each group.
pub fn group_by_device(endpoints: Vec<HidEndpointInfo>) -> BTreeMap<DeviceKey, Vec<HidEndpointInfo>> {
    let mut map: BTreeMap<DeviceKey, Vec<HidEndpointInfo>> = BTreeMap::new();
    for ep in endpoints {
        map.entry(ep.key()).or_default().push(ep);
    }
    map
}

/// An open HID top-level collection.
///
/// All buffers follow the hidapi convention: the first byte is the report id
/// (`0x00` when the endpoint uses no report ids).
pub trait HidHandle: Send {
    /// Static information about this endpoint.
    fn info(&self) -> &HidEndpointInfo;

    /// Write an output report. Returns `data.len()` on success (backends may
    /// pad the transfer to the collection's output report length).
    fn write(&mut self, data: &[u8]) -> HalResult<usize>;

    /// Read an input report, waiting at most `timeout_ms` (`0` = non-blocking
    /// poll, `-1` = block). The report id comes first, `0x00` when the
    /// collection uses unnumbered reports, as SignalRGB plugins expect
    /// (hid-usb.md). Returns the bytes stored, `Ok(0)` on timeout.
    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize>;

    /// SET_REPORT(Feature). Returns the number of bytes sent.
    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize>;

    /// GET_REPORT(Feature); `buf[0]` holds the report id on entry. Returns bytes read.
    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize>;

    /// GET_REPORT(Input); `buf[0]` holds the report id on entry. Returns bytes read.
    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize>;

    /// Discard any queued input reports. Default: drain with non-blocking reads.
    fn clear_read_buffer(&mut self) -> HalResult<()> {
        let mut scratch = vec![0u8; 4096];
        while self.read_timeout(&mut scratch, 0)? > 0 {}
        Ok(())
    }

    /// The report descriptor of this collection, as the OS reports it (on
    /// Windows: reconstructed from the collection's preparsed data). No report
    /// I/O. Default: unsupported.
    fn report_descriptor(&mut self) -> HalResult<Vec<u8>> {
        Err(HalError::Unsupported("report descriptor"))
    }
}

/// Enumerates and opens HID top-level collections.
pub trait HidBackend: Send + Sync {
    /// Enumerate every HID top-level collection visible to this process.
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>>;

    /// Open the endpoint whose [`HidEndpointInfo::path`] equals `path`.
    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>>;
}

/// A backend without devices: enumerates nothing and opens nothing. Device
/// sessions that drive an SMBus device instead of HID use it.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHid;

impl HidBackend for NoHid {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        Ok(Vec::new())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        Err(HalError::NotFound(path.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_hid_has_nothing() {
        assert!(NoHid.enumerate().unwrap().is_empty());
        assert!(matches!(NoHid.open("x"), Err(HalError::NotFound(_))));
    }

    #[test]
    fn tuple_matching_follows_set_endpoint_rules() {
        let t = HidEndpointTuple {
            interface: 1,
            usage: 0x0602,
            usage_page: 0xFF43,
            collection: 2,
        };
        assert!(t.matches(1, 0x0602, 0xFF43, None));
        assert!(t.matches(1, 0x0602, 0xFF43, Some(2)));
        assert!(!t.matches(1, 0x0602, 0xFF43, Some(0)));
        assert!(!t.matches(0, 0x0602, 0xFF43, None));
    }

    #[test]
    fn grouping_keeps_order() {
        let mk = |serial: &str, col: u16| HidEndpointInfo {
            vid: 1,
            pid: 2,
            serial: serial.into(),
            collection: col,
            ..Default::default()
        };
        let groups = group_by_device(vec![mk("A", 0), mk("B", 0), mk("A", 1)]);
        assert_eq!(groups.len(), 2);
        let a = &groups[&DeviceKey::new(1, 2, "A")];
        assert_eq!(a.iter().map(|e| e.collection).collect::<Vec<_>>(), vec![0, 1]);
    }
}
