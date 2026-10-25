// SPDX-License-Identifier: GPL-3.0-or-later
//! The HID backend of a hardware run and the present devices some plugin claims.
//!
//! Serial devices (`Type() == "serial"` plugins) come from the OS list of
//! serial ports ([`serial_targets`]): a port whose USB parent's VID:PID a
//! serial plugin owns becomes a device with all the ports of that USB device.
//! Ports are listed, never opened or written to, until a session connects.

use lightloom_hal::{HidBackend, SerialBackend};
use lightloom_hal_windows::DeviceGroup;
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::manager::Present;
use lightloom_plugin_host::serial::{group_ports, SerialTarget};
use lightloom_plugin_host::session::SessionDevice;
use std::collections::BTreeSet;
use std::sync::Arc;

/// `LIGHTLOOM_HID_BACKEND=hidapi` selects the hidapi backend instead of the native one.
pub fn use_hidapi() -> bool {
    std::env::var("LIGHTLOOM_HID_BACKEND").is_ok_and(|v| v.eq_ignore_ascii_case("hidapi"))
}

/// Whether the device `key` passes the `--device` style `filters` (key prefixes,
/// case-insensitive; no filter passes everything).
pub fn matches_filter(key: &str, filters: &[String]) -> bool {
    filters.is_empty()
        || filters.iter().any(|f| {
            key.to_ascii_lowercase()
                .starts_with(&f.trim().to_ascii_lowercase())
        })
}

/// `(vid, pid)` pairs and `(usage_page, usage)` pairs the catalog's plugins claim.
pub fn claims(catalog: &Catalog) -> (BTreeSet<(u16, u16)>, BTreeSet<(u16, u16)>) {
    let wanted = catalog.entries().iter().flat_map(|e| e.meta.keys()).collect();
    let usages = catalog
        .entries()
        .iter()
        .flat_map(|e| e.meta.hid_usages.iter().map(|[page, usage]| (*page, *usage)))
        .collect();
    (wanted, usages)
}

/// The groups some plugin drives, after `filters`, each with its plugin.
pub fn present(groups: Vec<DeviceGroup>, catalog: &Catalog, filters: &[String]) -> Vec<Present> {
    let mut targets = Vec::new();
    for g in groups {
        let key = g.key.to_string();
        if !matches_filter(&key, filters) {
            continue;
        }
        let endpoints: Vec<_> = g.endpoints.into_iter().map(|e| e.info).collect();
        if let Some(plugin) = catalog.lookup_device(g.key.vid, g.key.pid, &endpoints) {
            targets.push(Present {
                device: SessionDevice {
                    smbus: None,
                    lamparray: None,
                    serial: None,
                    key,
                    vid: g.key.vid,
                    pid: g.key.pid,
                    endpoints,
                    network: None,
                },
                plugin: plugin.clone(),
            });
        }
    }
    targets
}

/// The serial devices some serial plugin owns among `backend`'s ports,
/// after `filters`, each with its plugin.
pub fn serial_targets(
    backend: Arc<dyn SerialBackend>,
    catalog: &Catalog,
    filters: &[String],
) -> lightloom_hal::HalResult<Vec<Present>> {
    if !catalog.has_serial_plugins() {
        return Ok(Vec::new());
    }
    let mut targets = Vec::new();
    for ((vid, pid, _), ports) in group_ports(backend.available_ports()?) {
        let Some(plugin) = catalog.lookup_serial(vid, pid) else {
            continue;
        };
        let device = SessionDevice::serial(
            vid,
            pid,
            SerialTarget {
                backend: Arc::clone(&backend),
                ports,
            },
        );
        if matches_filter(&device.key, filters) {
            targets.push(Present {
                device,
                plugin: plugin.clone(),
            });
        }
    }
    Ok(targets)
}

/// The serial backend of this OS (stateless; nothing is opened).
#[cfg(windows)]
pub fn serial_backend() -> Option<Arc<dyn SerialBackend>> {
    Some(Arc::new(lightloom_hal_windows::WindowsSerialBackend::new()))
}

/// The serial backend of this OS (sysfs; nothing is opened).
#[cfg(target_os = "linux")]
pub fn serial_backend() -> Option<Arc<dyn SerialBackend>> {
    Some(Arc::new(lightloom_hal_linux::LinuxSerialBackend::new()))
}

/// No serial backend on this platform.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn serial_backend() -> Option<Arc<dyn SerialBackend>> {
    None
}

/// The HID backend of a hardware run, kept to enumerate again after hotplug events.
#[cfg(windows)]
pub enum Hardware {
    /// `HidD_*` / `CreateFile` backend (default).
    Native(Arc<lightloom_hal_windows::NativeHidBackend>),
    /// hidapi backend (`LIGHTLOOM_HID_BACKEND=hidapi`).
    Hidapi(Arc<lightloom_hal_windows::WindowsHidBackend>),
}

/// The HID backend of a hardware run (hidraw), kept to enumerate again after hotplug events.
#[cfg(target_os = "linux")]
pub enum Hardware {
    /// `/dev/hidraw*` backend.
    Hidraw(Arc<lightloom_hal_linux::LinuxHidBackend>),
}

#[cfg(any(windows, target_os = "linux"))]
impl Hardware {
    /// The backend of this OS ([`use_hidapi`] selects between the two on Windows).
    #[cfg(windows)]
    pub fn open() -> anyhow::Result<Self> {
        use anyhow::Context;
        Ok(if use_hidapi() {
            Self::Hidapi(Arc::new(
                lightloom_hal_windows::WindowsHidBackend::new().context("initialising hidapi")?,
            ))
        } else {
            Self::Native(Arc::new(lightloom_hal_windows::NativeHidBackend::new()))
        })
    }

    /// The hidraw backend (nothing is opened until a session starts).
    #[cfg(target_os = "linux")]
    pub fn open() -> anyhow::Result<Self> {
        Ok(Self::Hidraw(Arc::new(lightloom_hal_linux::LinuxHidBackend::new())))
    }

    /// The backend as the plugin host sees it.
    pub fn backend(&self) -> Arc<dyn HidBackend> {
        match self {
            #[cfg(windows)]
            Self::Native(b) => b.clone(),
            #[cfg(windows)]
            Self::Hidapi(b) => b.clone(),
            #[cfg(target_os = "linux")]
            Self::Hidraw(b) => b.clone(),
        }
    }

    /// The present devices some plugin claims (enumerating and probing only
    /// those), after `filters`, each with its plugin; plus how many matched
    /// before filtering.
    pub fn targets(&self, catalog: &Catalog, filters: &[String]) -> anyhow::Result<(Vec<Present>, usize)> {
        use anyhow::Context;
        let (wanted, usages) = claims(catalog);
        let groups = match self {
            #[cfg(windows)]
            Self::Native(b) => b.enumerate_grouped_matching(&wanted, &usages),
            #[cfg(windows)]
            Self::Hidapi(b) => b.enumerate_grouped_matching(&wanted, &usages),
            #[cfg(target_os = "linux")]
            Self::Hidraw(b) => b.enumerate_grouped_matching(&wanted, &usages),
        }
        .context("enumerating HID devices")?;
        let mut total = groups.len();
        let mut targets = present(groups, catalog, filters);
        if let Some(serial) = serial_backend() {
            // A failed port listing must not cost the HID devices.
            match serial_targets(serial, catalog, &[]) {
                Ok(found) => {
                    total += found.len();
                    targets.extend(
                        found
                            .into_iter()
                            .filter(|p| matches_filter(&p.device.key, filters)),
                    );
                }
                Err(e) => tracing::warn!(error = %e, "listing serial ports failed"),
            }
        }
        Ok((targets, total))
    }
}

/// No hardware backend on this platform.
#[cfg(not(any(windows, target_os = "linux")))]
pub enum Hardware {}

#[cfg(not(any(windows, target_os = "linux")))]
impl Hardware {
    /// Always an error: Lightloom drives hardware on Windows and Linux only.
    pub fn open() -> anyhow::Result<Self> {
        anyhow::bail!(
            "driving hardware is implemented on Windows and Linux only; try `lightloom bench --mock VID:PID`"
        )
    }

    /// Unreachable.
    pub fn backend(&self) -> Arc<dyn HidBackend> {
        match *self {}
    }

    /// Unreachable.
    pub fn targets(&self, _: &Catalog, _: &[String]) -> anyhow::Result<(Vec<Present>, usize)> {
        match *self {}
    }
}

/// Every connected HID device grouped by physical device (read-only
/// enumeration; `LIGHTLOOM_HID_BACKEND=hidapi` selects hidapi on Windows).
#[cfg(windows)]
pub fn connected_groups() -> anyhow::Result<Vec<DeviceGroup>> {
    if use_hidapi() {
        return Ok(lightloom_hal_windows::WindowsHidBackend::new()?.enumerate_grouped()?);
    }
    Ok(lightloom_hal_windows::NativeHidBackend::new().enumerate_grouped()?)
}

/// Every connected HID device grouped by physical device (sysfs only, nothing opened).
#[cfg(target_os = "linux")]
pub fn connected_groups() -> anyhow::Result<Vec<DeviceGroup>> {
    Ok(lightloom_hal_linux::LinuxHidBackend::new().enumerate_grouped()?)
}

/// Every connected HID device: not implemented on this OS.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn connected_groups() -> anyhow::Result<Vec<DeviceGroup>> {
    anyhow::bail!("HID enumeration is implemented on Windows and Linux only")
}

/// Every connected HID device (read-only enumeration): key, VID, PID.
pub fn connected_devices() -> anyhow::Result<Vec<(String, u16, u16)>> {
    #[cfg(windows)]
    let groups = lightloom_hal_windows::NativeHidBackend::new().enumerate_grouped()?;
    #[cfg(not(windows))]
    let groups = connected_groups()?;
    Ok(groups
        .into_iter()
        .map(|g| (g.key.to_string(), g.key.vid, g.key.pid))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_ports_of_claimed_usb_devices_become_devices() {
        use lightloom_hal::mock::MockSerial;
        let dir = std::env::temp_dir().join(format!("lightloom-serial-targets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("strip.js"),
            "export function Name() { return 'Strip'; }\n\
             export function Type() { return 'serial'; }\n\
             export function VendorId() { return 0x16D5; }\n\
             export function ProductId() { return 0x2A08; }\n\
             export function Initialize() {}\n\
             export function Render() {}\n",
        )
        .unwrap();
        let catalog = Catalog::scan(std::slice::from_ref(&dir), std::time::Duration::from_secs(10));
        let mock = MockSerial::new()
            .add_port(MockSerial::usb_port("COM3", 0x16D5, 0x2A08, "N8", Some(0)))
            .add_port(MockSerial::usb_port("COM4", 0x16D5, 0x2A08, "N8", Some(2)))
            .add_port(MockSerial::usb_port("COM5", 0x0403, 0x6001, "FTDI", Some(0)));
        let backend: Arc<dyn SerialBackend> = Arc::new(mock);
        let found = serial_targets(Arc::clone(&backend), &catalog, &[]).unwrap();
        assert_eq!(found.len(), 1, "the unclaimed FTDI port is no device");
        let d = &found[0].device;
        assert_eq!(d.key, "16d5:2a08:N8");
        assert_eq!(d.serial.as_ref().unwrap().ports.len(), 2);
        assert!(serial_targets(backend, &catalog, &["1532".into()])
            .unwrap()
            .is_empty());
        // The serial plugin does not drive a HID device with its VID:PID.
        assert!(catalog.lookup_device(0x16D5, 0x2A08, &[]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn filters_are_case_insensitive_key_prefixes() {
        assert!(matches_filter("1462:7c91:A0", &[]));
        assert!(matches_filter("1462:7c91:A0", &["1462:7C91".into()]));
        assert!(!matches_filter("1462:7c91:A0", &["1532".into()]));
    }
}
