// SPDX-License-Identifier: GPL-3.0-or-later
//! The HID backend of a hardware run and the present devices some plugin claims.
//!
//! Raw USB devices (WinUSB on Windows) are enumerated next to the HID
//! collections and joined to them by USB parent: the HID group whose USB
//! parent instance id is the USB device's own gets it as its raw USB side
//! (`hybrid` plugins, `device.setProtocol("hybrid")`). A USB device no HID
//! group claims is a device of its own when a `rawusb` plugin owns its
//! VID:PID, keyed like SignalRGB keys it (`vid:pid:<instance serial>`).

use lightloom_hal::usb::{UsbBackend, UsbDeviceInfo};
use lightloom_hal::{DeviceKey, HidBackend};
use lightloom_hal_windows::path::signalrgb_serial_from_instance;
use lightloom_hal_windows::DeviceGroup;
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::manager::Present;
use lightloom_plugin_host::session::{SessionDevice, UsbTarget};
use lightloom_plugin_host::PluginType;
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
    present_with_usb(groups, Vec::new(), None, catalog, filters)
}

/// The SignalRGB-shaped key of a USB device without HID collections: the
/// serial is its instance id's last segment (as for HID groups), else its
/// USB serial string.
pub fn usb_device_key(info: &UsbDeviceInfo) -> DeviceKey {
    let serial = signalrgb_serial_from_instance(&info.instance_id).unwrap_or_else(|| info.serial.clone());
    DeviceKey::new(info.vid, info.pid, serial)
}

/// [`present`] with the raw USB devices `usb` (opened through `backend`)
/// joined to their HID groups by USB parent, and USB-only devices that a
/// `rawusb` plugin owns added (see the module docs).
pub fn present_with_usb(
    groups: Vec<DeviceGroup>,
    usb: Vec<UsbDeviceInfo>,
    backend: Option<Arc<dyn UsbBackend>>,
    catalog: &Catalog,
    filters: &[String],
) -> Vec<Present> {
    let mut usb: Vec<Option<UsbDeviceInfo>> = usb.into_iter().map(Some).collect();
    let mut target_of = |parent: Option<&str>| -> Option<UsbTarget> {
        let (parent, backend) = (parent?, backend.as_ref()?);
        let slot = usb.iter_mut().find(|u| {
            u.as_ref()
                .is_some_and(|u| !u.instance_id.is_empty() && u.instance_id.eq_ignore_ascii_case(parent))
        })?;
        Some(UsbTarget {
            backend: Arc::clone(backend),
            info: slot.take()?,
        })
    };
    let mut targets = Vec::new();
    for g in groups {
        let key = g.key.to_string();
        // Joined before filtering: a filtered-out group keeps its USB device.
        let target = target_of(g.usb_parent.as_deref());
        if !matches_filter(&key, filters) {
            continue;
        }
        let endpoints: Vec<_> = g.endpoints.into_iter().map(|e| e.info).collect();
        if let Some(plugin) = catalog.lookup_device(g.key.vid, g.key.pid, &endpoints) {
            targets.push(Present {
                device: SessionDevice::hid(key, g.key.vid, g.key.pid, endpoints).with_usb(target),
                plugin: plugin.clone(),
            });
        }
    }
    let Some(backend) = backend else {
        return targets;
    };
    for info in usb.into_iter().flatten() {
        let key = usb_device_key(&info).to_string();
        if !matches_filter(&key, filters) {
            continue;
        }
        let Some(plugin) = catalog
            .lookup(info.vid, info.pid)
            .filter(|p| p.meta.plugin_type == PluginType::RawUsb)
        else {
            continue;
        };
        targets.push(Present {
            device: SessionDevice::usb(
                key,
                UsbTarget {
                    backend: Arc::clone(&backend),
                    info,
                },
            ),
            plugin: plugin.clone(),
        });
    }
    targets
}

/// The raw USB backend of this OS (WinUSB on Windows); `None` where there is none yet.
pub fn usb_backend() -> Option<Arc<dyn UsbBackend>> {
    #[cfg(windows)]
    return Some(Arc::new(lightloom_hal_windows::WinUsbBackend::new()));
    #[cfg(not(windows))]
    None
}

/// The USB devices whose VID:PID is in `wanted` (read-only enumeration; a
/// failure is logged and counts as none).
pub fn usb_devices(backend: &dyn UsbBackend, wanted: &BTreeSet<(u16, u16)>) -> Vec<UsbDeviceInfo> {
    match backend.enumerate() {
        Ok(list) => list
            .into_iter()
            .filter(|d| wanted.contains(&(d.vid, d.pid)))
            .collect(),
        Err(e) => {
            tracing::warn!("enumerating USB devices failed: {e}");
            Vec::new()
        }
    }
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
        let total = groups.len();
        let usb = usb_backend();
        let devices = usb
            .as_deref()
            .map(|b| usb_devices(b, &wanted))
            .unwrap_or_default();
        Ok((present_with_usb(groups, devices, usb, catalog, filters), total))
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
    use lightloom_hal::mock::{MockHid, MockUsb};
    use lightloom_hal::ReportLengths;
    use lightloom_hal_windows::{GroupingSource, ProbedEndpoint};

    fn catalog() -> Catalog {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lightloom-plugin-host/testdata/device");
        Catalog::scan(&[dir], std::time::Duration::from_secs(10))
    }

    #[test]
    fn usb_devices_join_their_hid_group_by_usb_parent_or_stand_alone() {
        let hybrid = MockUsb::device_info(0x1234, 0x9999, "H1", vec![MockUsb::winusb_interface(1, &[0x02])]);
        let rawusb = MockUsb::device_info(
            0x1234,
            0x7701,
            "6&1a2b&0&3",
            vec![MockUsb::winusb_interface(0, &[0x01])],
        );
        let stranger = MockUsb::device_info(0x1234, 0x5555, "X", vec![MockUsb::winusb_interface(0, &[0x01])]);
        let ep = MockHid::endpoint_info(0x1234, 0x9999, "", 0, 0xFF00, 1, 0, ReportLengths::default());
        let group = DeviceGroup {
            key: DeviceKey::new(0x1234, 0x9999, "H1"),
            source: GroupingSource::UsbParent,
            usb_parent: Some(hybrid.instance_id.to_ascii_lowercase()),
            manufacturer: String::new(),
            product: String::new(),
            endpoints: vec![ProbedEndpoint::from_info(ep)],
        };
        let backend: Arc<dyn UsbBackend> = Arc::new(MockUsb::new());
        let present = present_with_usb(
            vec![group],
            vec![stranger, rawusb.clone(), hybrid.clone()],
            Some(backend),
            &catalog(),
            &[],
        );
        let keys: Vec<&str> = present.iter().map(|p| p.device.key.as_str()).collect();
        assert_eq!(keys, vec!["1234:9999:H1", "1234:7701:6-1a2b-0-3"]);
        assert_eq!(present[0].device.endpoints.len(), 1);
        assert_eq!(present[0].device.usb.as_ref().unwrap().info, hybrid);
        assert!(present[1].device.endpoints.is_empty());
        assert_eq!(present[1].device.usb.as_ref().unwrap().info, rawusb);
        assert_eq!(present[1].plugin.meta.plugin_type, PluginType::RawUsb);
        // Without a USB backend only the HID side is there.
        assert!(present_with_usb(Vec::new(), vec![rawusb], None, &catalog(), &[]).is_empty());
    }

    #[test]
    fn filters_are_case_insensitive_key_prefixes() {
        assert!(matches_filter("1462:7c91:A0", &[]));
        assert!(matches_filter("1462:7c91:A0", &["1462:7C91".into()]));
        assert!(!matches_filter("1462:7c91:A0", &["1532".into()]));
    }
}
