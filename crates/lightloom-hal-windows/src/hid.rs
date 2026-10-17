// SPDX-License-Identifier: Apache-2.0 OR MIT
//! hidapi-backed HID backend for Windows.
//!
//! hidapi's Windows backend already exposes one entry per top-level
//! collection (each is its own device-interface path), so enumeration maps
//! 1:1 onto [`HidEndpointInfo`]. Report lengths are read from the report
//! descriptor when an endpoint is opened (hidapi does not expose them at
//! enumeration time without opening the device, which could disturb devices
//! another process holds exclusively).

use crate::path::parse_hid_path;
use lightloom_hal::hid::{HidBackend, HidEndpointInfo, HidHandle, ReportLengths};
use lightloom_hal::{DeviceKey, HalError, HalResult};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::sync::Mutex;

/// Largest report descriptor hidapi returns (HID_API_MAX_REPORT_DESCRIPTOR_SIZE).
const MAX_REPORT_DESCRIPTOR: usize = 4096;

fn map_err(e: hidapi::HidError) -> HalError {
    let text = e.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("access is denied") || lower.contains("permission") {
        HalError::PermissionDenied(text)
    } else if lower.contains("not found") || lower.contains("cannot find") {
        HalError::NotFound(text)
    } else if lower.contains("device is not connected") || lower.contains("disconnected") {
        HalError::Disconnected
    } else {
        HalError::Backend(text)
    }
}

/// Endpoints of one physical device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceGroup {
    /// `vid:pid:serial`, or `vid:pid:<instance prefix>` when the device has no serial.
    pub key: DeviceKey,
    /// Top-level collections, in enumeration order.
    pub endpoints: Vec<HidEndpointInfo>,
}

/// hidapi `windows-native` backend.
pub struct WindowsHidBackend {
    api: Mutex<hidapi::HidApi>,
}

impl std::fmt::Debug for WindowsHidBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WindowsHidBackend")
    }
}

impl WindowsHidBackend {
    /// Initialise hidapi.
    pub fn new() -> HalResult<Self> {
        let api = hidapi::HidApi::new().map_err(map_err)?;
        Ok(Self { api: Mutex::new(api) })
    }

    fn info_from(d: &hidapi::DeviceInfo) -> HidEndpointInfo {
        let path = d.path().to_string_lossy().into_owned();
        let parsed = parse_hid_path(&path);
        HidEndpointInfo {
            vid: d.vendor_id(),
            pid: d.product_id(),
            serial: d.serial_number().unwrap_or("").to_string(),
            manufacturer: d.manufacturer_string().unwrap_or("").to_string(),
            product: d.product_string().unwrap_or("").to_string(),
            path,
            // hidapi reports -1 when the path has no MI_xx; prefer our parse when they disagree.
            interface_number: if d.interface_number() >= 0 {
                d.interface_number()
            } else {
                parsed.interface_number
            },
            usage_page: d.usage_page(),
            usage: d.usage(),
            collection: parsed.collection,
            report_lengths: ReportLengths::default(),
        }
    }

    /// Enumerate and group endpoints by parent device.
    ///
    /// Grouping uses the USB serial when present, otherwise the shared
    /// instance-id prefix of the HID path. Exact grouping across interfaces of
    /// a composite device without a serial needs `CM_Get_Parent` (M2).
    pub fn enumerate_grouped(&self) -> HalResult<Vec<DeviceGroup>> {
        let endpoints = self.enumerate()?;
        let mut groups: BTreeMap<DeviceKey, Vec<HidEndpointInfo>> = BTreeMap::new();
        for ep in endpoints {
            let key = if ep.serial.is_empty() {
                let hint = parse_hid_path(&ep.path)
                    .instance_prefix()
                    .unwrap_or("")
                    .to_string();
                DeviceKey::new(ep.vid, ep.pid, hint)
            } else {
                ep.key()
            };
            groups.entry(key).or_default().push(ep);
        }
        Ok(groups
            .into_iter()
            .map(|(key, endpoints)| DeviceGroup { key, endpoints })
            .collect())
    }
}

impl HidBackend for WindowsHidBackend {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        let mut api = self.api.lock().unwrap_or_else(|e| e.into_inner());
        api.refresh_devices().map_err(map_err)?;
        Ok(api.device_list().map(Self::info_from).collect())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        let api = self.api.lock().unwrap_or_else(|e| e.into_inner());
        let cpath = CString::new(path).map_err(|e| HalError::InvalidArgument(e.to_string()))?;
        let device = api.open_path(&cpath).map_err(map_err)?;
        let mut info = api
            .device_list()
            .find(|d| d.path().to_bytes() == cpath.as_bytes())
            .map(Self::info_from)
            .unwrap_or_else(|| HidEndpointInfo {
                path: path.to_string(),
                ..Default::default()
            });
        drop(api);

        // Report lengths from the descriptor (best effort).
        let mut buf = vec![0u8; MAX_REPORT_DESCRIPTOR];
        // TODO(compile-check): hidapi 2.6 `HidDevice::get_report_descriptor(&self, &mut [u8]) -> HidResult<usize>`.
        match device.get_report_descriptor(&mut buf) {
            Ok(n) => match lightloom_hid_desc::parse(&buf[..n]) {
                Ok(desc) => {
                    let tlc = desc
                        .collection(info.collection)
                        .filter(|c| c.usage_page == info.usage_page && c.usage == info.usage)
                        .or_else(|| desc.find(info.usage_page, info.usage));
                    if let Some(c) = tlc {
                        info.report_lengths = ReportLengths {
                            input: c.input_len,
                            output: c.output_len,
                            feature: c.feature_len,
                        };
                    }
                }
                Err(e) => tracing::debug!(path, error = %e, "report descriptor not parsed"),
            },
            Err(e) => tracing::debug!(path, error = %e, "report descriptor unavailable"),
        }
        device.set_blocking_mode(true).map_err(map_err)?;
        tracing::debug!(path, report_lengths = ?info.report_lengths, "opened HID endpoint");
        Ok(Box::new(WindowsHidHandle { device, info }))
    }
}

/// An open HID collection.
pub struct WindowsHidHandle {
    device: hidapi::HidDevice,
    info: HidEndpointInfo,
}

impl HidHandle for WindowsHidHandle {
    fn info(&self) -> &HidEndpointInfo {
        &self.info
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        self.device.write(data).map_err(map_err)
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        self.device.read_timeout(buf, timeout_ms).map_err(map_err)
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        self.device.send_feature_report(data).map_err(map_err)?;
        Ok(data.len())
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        self.device.get_feature_report(buf).map_err(map_err)
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        // hidapi's windows-native backend does not expose HidD_GetInputReport (2.6.7);
        // TODO(M2): call HidD_GetInputReport via the `windows` crate on the raw handle.
        let _ = buf;
        Err(HalError::Unsupported(
            "get_input_report (HidD_GetInputReport) not wired on Windows yet",
        ))
    }
}
