// SPDX-License-Identifier: GPL-3.0-or-later
//! The HID backend of a hardware run and the present devices some plugin claims.

use huedeck_hal::HidBackend;
use huedeck_plugin_host::catalog::Catalog;
use huedeck_plugin_host::manager::Present;
#[cfg(windows)]
use huedeck_plugin_host::session::SessionDevice;
use std::sync::Arc;

/// `HUEDECK_HID_BACKEND=hidapi` selects the hidapi backend instead of the native one.
pub fn use_hidapi() -> bool {
    std::env::var("HUEDECK_HID_BACKEND").is_ok_and(|v| v.eq_ignore_ascii_case("hidapi"))
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

/// The HID backend of a hardware run, kept to enumerate again after hotplug events.
#[cfg(windows)]
pub enum Hardware {
    /// `HidD_*` / `CreateFile` backend (default).
    Native(Arc<huedeck_hal_windows::NativeHidBackend>),
    /// hidapi backend (`HUEDECK_HID_BACKEND=hidapi`).
    Hidapi(Arc<huedeck_hal_windows::WindowsHidBackend>),
}

#[cfg(windows)]
impl Hardware {
    /// The backend [`use_hidapi`] selects.
    pub fn open() -> anyhow::Result<Self> {
        use anyhow::Context;
        Ok(if use_hidapi() {
            Self::Hidapi(Arc::new(
                huedeck_hal_windows::WindowsHidBackend::new().context("initialising hidapi")?,
            ))
        } else {
            Self::Native(Arc::new(huedeck_hal_windows::NativeHidBackend::new()))
        })
    }

    /// The backend as the plugin host sees it.
    pub fn backend(&self) -> Arc<dyn HidBackend> {
        match self {
            Self::Native(b) => b.clone(),
            Self::Hidapi(b) => b.clone(),
        }
    }

    /// The present devices some plugin claims (enumerating and probing only
    /// those), after `filters`, each with its plugin; plus how many matched
    /// before filtering.
    pub fn targets(&self, catalog: &Catalog, filters: &[String]) -> anyhow::Result<(Vec<Present>, usize)> {
        use anyhow::Context;
        let wanted: std::collections::BTreeSet<(u16, u16)> =
            catalog.entries().iter().flat_map(|e| e.meta.keys()).collect();
        let groups = match self {
            Self::Native(b) => b.enumerate_grouped_for(&wanted),
            Self::Hidapi(b) => b.enumerate_grouped_for(&wanted),
        }
        .context("enumerating HID devices")?;
        let total = groups.len();
        let mut targets = Vec::new();
        for g in groups {
            let key = g.key.to_string();
            if !matches_filter(&key, filters) {
                continue;
            }
            if let Some(plugin) = catalog.lookup(g.key.vid, g.key.pid) {
                targets.push(Present {
                    device: SessionDevice {
                        key,
                        vid: g.key.vid,
                        pid: g.key.pid,
                        endpoints: g.endpoints.into_iter().map(|e| e.info).collect(),
                    },
                    plugin: plugin.clone(),
                });
            }
        }
        Ok((targets, total))
    }
}

/// No hardware backend on this platform yet.
#[cfg(not(windows))]
pub enum Hardware {}

#[cfg(not(windows))]
impl Hardware {
    /// Always an error: the Linux backend arrives with milestone M7.
    pub fn open() -> anyhow::Result<Self> {
        anyhow::bail!(
            "driving hardware is only implemented on Windows so far (Linux backend: milestone M7); \
             try `huedeck bench --mock VID:PID`"
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

/// Every connected HID device (read-only enumeration): key, VID, PID.
#[cfg(windows)]
pub fn connected_devices() -> anyhow::Result<Vec<(String, u16, u16)>> {
    let groups = huedeck_hal_windows::NativeHidBackend::new().enumerate_grouped()?;
    Ok(groups
        .into_iter()
        .map(|g| (g.key.to_string(), g.key.vid, g.key.pid))
        .collect())
}

/// Every connected HID device: not implemented on this OS yet.
#[cfg(not(windows))]
pub fn connected_devices() -> anyhow::Result<Vec<(String, u16, u16)>> {
    anyhow::bail!("HID enumeration is only implemented on Windows so far")
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
