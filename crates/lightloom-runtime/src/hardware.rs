// SPDX-License-Identifier: GPL-3.0-or-later
//! The HID backend of a hardware run and the present devices some plugin claims.

use lightloom_hal::HidBackend;
use lightloom_hal_windows::DeviceGroup;
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::manager::Present;
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
                    key,
                    vid: g.key.vid,
                    pid: g.key.pid,
                    endpoints,
                },
                plugin: plugin.clone(),
            });
        }
    }
    targets
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
        Ok((present(groups, catalog, filters), total))
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
    fn filters_are_case_insensitive_key_prefixes() {
        assert!(matches_filter("1462:7c91:A0", &[]));
        assert!(matches_filter("1462:7c91:A0", &["1462:7C91".into()]));
        assert!(!matches_filter("1462:7c91:A0", &["1532".into()]));
    }
}
