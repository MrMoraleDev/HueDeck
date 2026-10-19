// SPDX-License-Identifier: GPL-3.0-or-later
//! Other software holding the devices Lightloom drives: reported, never terminated.

use lightloom_plugin_host::manager::Present;

/// RGB software that holds the devices Lightloom drives.
pub const KNOWN_CONFLICTS: &[&str] = &["SignalRgb.exe", "OpenRGB.exe"];

/// Vendor lighting services that drive the same GPU and DIMM RGB controllers
/// over SMBus / GPU I2C (SignalRGB's conflict table for its RAM and GPU
/// plugins, docs/host-api/smbus.md): MSI Mystic Light (`LEDKeeper*`,
/// `LightKeeperService*`, `Mystic_Light_Service`), ASUS Aura (Armoury Crate's
/// `LightingService`), Gigabyte RGB Fusion. `ControlCenter.exe` (Gigabyte)
/// is left out: Elgato's Control Center has the same name.
pub const SMBUS_VENDOR_SERVICES: &[&str] = &[
    "LEDKeeper.exe",
    "LEDKeeper2.exe",
    "LightKeeperService.exe",
    "LightKeeperService2.exe",
    "Mystic_Light_Service.exe",
    "LightingService.exe",
    "RGBFusion.exe",
];

/// The running processes among [`KNOWN_CONFLICTS`], `extra` and the
/// `ConflictingProcesses` of the targets' plugins, sorted, without duplicates.
#[cfg(windows)]
pub fn running_conflicts(targets: &[Present], extra: &[&str]) -> anyhow::Result<Vec<String>> {
    let running = lightloom_hal_windows::system::running_process_names().map_err(anyhow::Error::msg)?;
    let mut wanted: Vec<String> = KNOWN_CONFLICTS
        .iter()
        .chain(extra)
        .map(|s| s.to_string())
        .collect();
    for t in targets {
        wanted.extend(t.plugin.meta.conflicting_processes.iter().cloned());
    }
    let mut found: Vec<String> = running
        .into_iter()
        .filter(|r| wanted.iter().any(|w| w.eq_ignore_ascii_case(r)))
        .collect();
    found.sort_by_key(|s| s.to_ascii_lowercase());
    found.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(found)
}

/// No process list on this OS yet: nothing found.
#[cfg(not(windows))]
pub fn running_conflicts(_: &[Present], _: &[&str]) -> anyhow::Result<Vec<String>> {
    Ok(Vec::new())
}

/// The running programs that forbid SMBus and GPU I2C traffic: SignalRGB and
/// OpenRGB drive the same buses (and their `Scan()` writes remap devices),
/// and the vendor services of [`SMBUS_VENDOR_SERVICES`] drive the same
/// controllers, so Lightloom never touches them alongside. Unlike HID
/// conflicts this cannot be bypassed (`--ignore-conflicts` does not apply).
/// Checked before every scan and again every [`SMBUS_RECHECK`] while SMBus
/// devices are driven.
#[cfg(windows)]
pub fn smbus_blockers() -> anyhow::Result<Vec<String>> {
    let running = lightloom_hal_windows::system::running_process_names().map_err(anyhow::Error::msg)?;
    Ok(blockers_among(
        &running.iter().map(String::as_str).collect::<Vec<_>>(),
    ))
}

/// No process list on this OS yet: nothing found.
#[cfg(not(windows))]
pub fn smbus_blockers() -> anyhow::Result<Vec<String>> {
    Ok(Vec::new())
}

/// Held while this process drives SMBus / GPU I2C (see [`claim_smbus`]).
#[derive(Debug)]
pub struct SmbusClaim {
    #[cfg(windows)]
    _owner: lightloom_hal_windows::owner::SmbusOwner,
}

/// Become the one Lightloom process that drives SMBus and GPU I2C on this
/// machine: `lightloomd` and `lightloom … --smbus` never drive the buses side by
/// side. Fails while another Lightloom process holds the claim.
#[cfg(windows)]
pub fn claim_smbus() -> anyhow::Result<SmbusClaim> {
    match lightloom_hal_windows::owner::SmbusOwner::try_acquire().map_err(anyhow::Error::msg)? {
        Some(owner) => Ok(SmbusClaim { _owner: owner }),
        None => anyhow::bail!(
            "another Lightloom process (lightloomd with SMBus on, or a `lightloom --smbus` run) drives SMBus and GPU I2C:              stop it first"
        ),
    }
}

/// Nothing to share the buses with outside Windows yet.
#[cfg(not(windows))]
pub fn claim_smbus() -> anyhow::Result<SmbusClaim> {
    Ok(SmbusClaim {})
}

/// How often [`smbus_blockers`] runs again while SMBus devices are driven.
pub const SMBUS_RECHECK: std::time::Duration = std::time::Duration::from_secs(1);

/// The names in `running` that are [`KNOWN_CONFLICTS`] or
/// [`SMBUS_VENDOR_SERVICES`] (case-insensitive), sorted, once each.
pub fn blockers_among(running: &[&str]) -> Vec<String> {
    let mut found: Vec<String> = running
        .iter()
        .filter(|r| {
            KNOWN_CONFLICTS
                .iter()
                .chain(SMBUS_VENDOR_SERVICES)
                .any(|k| k.eq_ignore_ascii_case(r))
        })
        .map(|r| r.to_string())
        .collect();
    found.sort_by_key(|s| s.to_ascii_lowercase());
    found.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockers_are_case_insensitive() {
        assert_eq!(
            blockers_among(&["explorer.exe", "signalrgb.exe", "OpenRGB.exe", "SIGNALRGB.EXE"]),
            vec!["OpenRGB.exe".to_string(), "signalrgb.exe".to_string()]
        );
        assert!(blockers_among(&["lightloomd.exe"]).is_empty());
    }

    #[test]
    fn vendor_lighting_services_block_smbus() {
        assert_eq!(
            blockers_among(&["LEDKeeper2.exe", "lightingservice.exe", "ControlCenter.exe"]),
            vec!["LEDKeeper2.exe".to_string(), "lightingservice.exe".to_string()]
        );
    }

    #[test]
    fn one_smbus_owner_per_machine() {
        let Ok(first) = claim_smbus() else {
            // A Lightloom process on this machine drives SMBus right now.
            return;
        };
        let err = claim_smbus().unwrap_err().to_string();
        assert!(err.contains("another Lightloom process"), "{err}");
        drop(first);
        assert!(claim_smbus().is_ok());
    }
}
