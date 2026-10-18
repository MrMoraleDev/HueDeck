// SPDX-License-Identifier: Apache-2.0 OR MIT
//! hidapi-backed HID backend for Windows.
//!
//! hidapi's Windows backend already exposes one entry per top-level
//! collection (each is its own device-interface path), so enumeration maps
//! 1:1 onto [`HidEndpointInfo`]. [`HidBackend::enumerate`] leaves report
//! lengths at zero; [`WindowsHidBackend::enumerate_grouped`] fills them from
//! `HIDP_CAPS` through a zero-access handle (see [`crate::probe`]), and
//! [`HidBackend::open`] reads them from the reconstructed report descriptor.

use crate::group::{group_endpoints, own_usb_parent, DeviceGroup, ProbedEndpoint};
use crate::path::parse_hid_path;
use crate::probe;
use lightloom_hal::hid::{HidBackend, HidEndpointInfo, HidHandle, ReportLengths};
use lightloom_hal::{HalError, HalResult};
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
            // hidapi windows-native: MI_xx from the hardware ids; a non-composite
            // USB device reports 0 (matches SignalRGB, see `path` docs); only
            // non-USB transports keep -1. Fall back to the path parse otherwise.
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

    /// Enumerate, probe (USB parent + `HIDP_CAPS`, read-only) and group
    /// endpoints by physical device. See [`crate::group`] for the identity
    /// rules and [`crate::probe`] for what "read-only" means here.
    pub fn enumerate_grouped(&self) -> HalResult<Vec<DeviceGroup>> {
        let endpoints = self.enumerate()?;
        let probed = endpoints
            .into_iter()
            .map(|info| {
                let mut ep = ProbedEndpoint::from_info(info);
                ep.usb_parent = own_usb_parent(probe::usb_parent(&ep.info.path), ep.info.vid, ep.info.pid);
                ep.caps = probe::read_caps(&ep.info.path).map(|c| {
                    if (c.usage_page, c.usage) != (ep.info.usage_page, ep.info.usage) {
                        tracing::debug!(
                            path = %ep.info.path,
                            caps_page = c.usage_page,
                            caps_usage = c.usage,
                            "HIDP_CAPS usage differs from hidapi enumeration"
                        );
                    }
                    c.lengths
                });
                if let Ok(lengths) = &ep.caps {
                    ep.info.report_lengths = *lengths;
                }
                ep
            })
            .collect();
        Ok(group_endpoints(probed))
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
                    // On Windows each `ColNN` path has its own preparsed data, so the
                    // reconstructed descriptor holds exactly this one TLC.
                    let tlc = desc
                        .find(info.usage_page, info.usage)
                        .or_else(|| desc.collections.first());
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
