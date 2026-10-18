// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Other software holding the devices Lightloom drives: reported, never terminated.

use lightloom_plugin_host::manager::Present;

/// RGB software that holds the devices Lightloom drives.
pub const KNOWN_CONFLICTS: &[&str] = &["SignalRgb.exe", "OpenRGB.exe"];

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
