// SPDX-License-Identifier: GPL-3.0-or-later
//! hidapi-backed HID backend for Windows.
//!
//! hidapi's Windows backend already exposes one entry per top-level
//! collection (each is its own device-interface path), so enumeration maps
//! 1:1 onto [`HidEndpointInfo`]. [`HidBackend::enumerate`] leaves report
//! lengths at zero; [`WindowsHidBackend::enumerate_grouped`] fills them from
//! `HIDP_CAPS` through a zero-access handle (see [`crate::probe`]), and
//! [`HidBackend::open`] reads them from `HIDP_CAPS` too (falling back to the
//! reconstructed report descriptor) and lists the declared feature reports.

use crate::group::{group_endpoints, own_usb_parent, DeviceGroup, ProbedEndpoint};
use crate::path::parse_hid_path;
use crate::probe;
use lightloom_hal::hid::{HidBackend, HidEndpointInfo, HidHandle, ReportLengths, ReportSpec};
use lightloom_hal::{HalError, HalResult};
use std::collections::BTreeSet;
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

/// `info`'s top-level collection from hidapi's reconstructed report descriptor.
fn descriptor_tlc(
    device: &hidapi::HidDevice,
    info: &HidEndpointInfo,
) -> Option<lightloom_hid_desc::TopLevelCollection> {
    let mut buf = vec![0u8; MAX_REPORT_DESCRIPTOR];
    let n = device
        .get_report_descriptor(&mut buf)
        .map_err(|e| tracing::debug!(path = %info.path, error = %e, "report descriptor unavailable"))
        .ok()?;
    let desc = lightloom_hid_desc::parse(&buf[..n])
        .map_err(|e| tracing::debug!(path = %info.path, error = %e, "report descriptor not parsed"))
        .ok()?;
    // On Windows each `ColNN` path has its own preparsed data, so the
    // reconstructed descriptor holds exactly this one TLC.
    desc.find(info.usage_page, info.usage)
        .or_else(|| desc.collections.first())
        .cloned()
}

/// Feature reports a collection declares, ascending by id.
fn feature_reports(tlc: &lightloom_hid_desc::TopLevelCollection) -> Vec<ReportSpec> {
    let mut out: Vec<ReportSpec> = tlc
        .reports
        .iter()
        .filter(|r| r.kind == lightloom_hid_desc::ReportKind::Feature)
        .map(|r| ReportSpec {
            id: r.id,
            len: r.byte_length(),
        })
        .collect();
    out.sort_by_key(|r| r.id);
    out.dedup_by_key(|r| r.id);
    out
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
    /// Initialise hidapi (hidapi 2.6 enumerates every HID interface here).
    pub fn new() -> HalResult<Self> {
        let started = std::time::Instant::now();
        let api = hidapi::HidApi::new().map_err(map_err)?;
        tracing::debug!(
            elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
            "hidapi initialised"
        );
        Ok(Self { api: Mutex::new(api) })
    }

    fn info_from(d: &hidapi::DeviceInfo) -> HidEndpointInfo {
        let path = d.path().to_string_lossy().into_owned();
        let parsed = parse_hid_path(&path);
        // hidapi windows-native: MI_xx from the hardware ids, 0 for a non-composite
        // USB device (as SignalRGB reports). It also reports 0 for transports it
        // mistakes for USB (see `probe::hid_bus`), so its value is kept for USB
        // collections only; otherwise `MI_xx` from the path or -1.
        let interface_number = match (probe::hid_bus(&path), d.interface_number()) {
            (probe::HidBus::Usb, n) if n >= 0 => n,
            _ => parsed.interface_number,
        };
        HidEndpointInfo {
            vid: d.vendor_id(),
            pid: d.product_id(),
            serial: d.serial_number().unwrap_or("").to_string(),
            manufacturer: d.manufacturer_string().unwrap_or("").to_string(),
            product: d.product_string().unwrap_or("").to_string(),
            path,
            interface_number,
            usage_page: d.usage_page(),
            usage: d.usage(),
            collection: parsed.collection,
            report_lengths: ReportLengths::default(),
            feature_reports: Vec::new(),
        }
    }

    /// Enumerate, probe (USB parent + `HIDP_CAPS`, read-only) and group
    /// endpoints by physical device. See [`crate::group`] for the identity
    /// rules and [`crate::probe`] for what "read-only" means here.
    pub fn enumerate_grouped(&self) -> HalResult<Vec<DeviceGroup>> {
        Ok(Self::probe_and_group(self.enumerate()?))
    }

    /// [`WindowsHidBackend::enumerate_grouped`] restricted to `wanted` `(vid, pid)`
    /// pairs (the devices a host has plugins for). The present pairs are read
    /// from the interface paths first ([`probe::present_hid`], nothing opened);
    /// hidapi then reads strings and usages, and the probes run, only for
    /// matching collections. Falls back to a full enumeration when a path's
    /// VID/PID cannot be parsed.
    pub fn enumerate_grouped_for(&self, wanted: &BTreeSet<(u16, u16)>) -> HalResult<Vec<DeviceGroup>> {
        let started = std::time::Instant::now();
        let present = match probe::present_hid() {
            Ok(p) if p.unparsed == 0 => p,
            other => {
                tracing::debug!(?other, "falling back to a full HID enumeration");
                let mut groups = self.enumerate_grouped()?;
                groups.retain(|g| wanted.contains(&(g.key.vid, g.key.pid)));
                return Ok(groups);
            }
        };
        let endpoints = {
            let mut api = self.api.lock().unwrap_or_else(|e| e.into_inner());
            api.reset_devices().map_err(map_err)?;
            for (vid, pid) in present.keys.intersection(wanted) {
                api.add_devices(*vid, *pid).map_err(map_err)?;
            }
            api.device_list().map(Self::info_from).collect::<Vec<_>>()
        };
        let listed = started.elapsed();
        let groups = Self::probe_and_group(endpoints);
        tracing::debug!(
            list_ms = listed.as_secs_f64() * 1000.0,
            total_ms = started.elapsed().as_secs_f64() * 1000.0,
            groups = groups.len(),
            "filtered HID enumeration"
        );
        Ok(groups)
    }

    fn probe_and_group(endpoints: Vec<HidEndpointInfo>) -> Vec<DeviceGroup> {
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
        group_endpoints(probed)
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

        // Report lengths: HIDP_CAPS through a zero-access handle (exactly what Windows
        // enforces for HidD_Get/SetFeature and WriteFile), else the reconstructed
        // descriptor, which also lists the feature reports by id.
        let tlc = descriptor_tlc(&device, &info);
        let tlc_numbered = tlc.as_ref().is_none_or(|t| t.uses_report_ids());
        info.feature_reports = tlc.as_ref().map(feature_reports).unwrap_or_default();
        match probe::read_caps(path) {
            Ok(caps) => info.report_lengths = caps.lengths,
            Err(e) => {
                tracing::debug!(path, error = %e, "HIDP_CAPS unavailable, using the report descriptor");
                info.report_lengths = tlc
                    .map(|t| ReportLengths {
                        input: t.input_len,
                        output: t.output_len,
                        feature: t.feature_len,
                    })
                    .unwrap_or_default();
            }
        }
        device.set_blocking_mode(true).map_err(map_err)?;
        tracing::debug!(
            path,
            report_lengths = ?info.report_lengths,
            feature_reports = ?info.feature_reports,
            "opened HID endpoint"
        );
        // hidapi strips the leading 0x00 of unnumbered input reports; the HAL contract keeps it.
        let numbered = tlc_numbered;
        Ok(Box::new(WindowsHidHandle {
            device,
            info,
            numbered,
        }))
    }
}

/// An open HID collection.
pub struct WindowsHidHandle {
    device: hidapi::HidDevice,
    info: HidEndpointInfo,
    /// Whether the collection uses report ids (unknown counts as numbered).
    numbered: bool,
}

impl HidHandle for WindowsHidHandle {
    fn info(&self) -> &HidEndpointInfo {
        &self.info
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        // hidapi returns the padded length, or 0 when WriteFile completed synchronously.
        self.device.write(data).map_err(map_err)?;
        Ok(data.len())
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        if self.numbered || buf.is_empty() {
            return self.device.read_timeout(buf, timeout_ms).map_err(map_err);
        }
        // Unnumbered collection: hidapi stripped the 0x00 report id; put it back.
        let n = self
            .device
            .read_timeout(&mut buf[1..], timeout_ms)
            .map_err(map_err)?;
        if n == 0 {
            return Ok(0);
        }
        buf[0] = 0;
        Ok(n + 1)
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
