// SPDX-License-Identifier: GPL-3.0-or-later
//! SMBus discovery: which devices sit on which bus, found by the plugins.
//!
//! For every bus, every SMBus plugin offered it ([`Catalog::smbus_candidates`])
//! runs `Scan(bus)` in its own runtime, in scan mode, behind a [`GuardedBus`]
//! limited to its `SmbusAddresses()` window. The addresses it returns join
//! the bus allowlist (the only addresses device sessions may use) and each
//! becomes one device, keyed `smbus:<bus key>:<addr>`. When two plugins claim
//! the same address on one bus, the first one wins and the claim of the
//! other is reported.

use crate::catalog::Catalog;
use crate::device::{panic_text, WriteMode};
use crate::manager::Present;
use crate::runtime::{run_isolated, PluginRuntime, RuntimeOptions};
use crate::session::SessionDevice;
use crate::smbus::{BusHandle, GuardedBus, Phase};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// How [`scan_buses`] runs each `Scan(bus)`.
#[derive(Clone, Copy, Debug)]
pub struct ScanBudget {
    /// Parse + evaluate, then `Scan(bus)`, each.
    pub per_plugin: Duration,
    /// Dry-run: writes in `Scan()` are recorded, not sent (reads still are).
    pub mode: WriteMode,
}

impl Default for ScanBudget {
    fn default() -> Self {
        Self {
            per_plugin: Duration::from_secs(5),
            mode: WriteMode::Live,
        }
    }
}

/// What one plugin's `Scan(bus)` found.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ScanOutcome {
    /// Bus name.
    pub bus: String,
    /// Plugin `Name()`.
    pub plugin: String,
    /// Addresses it returned (inside its scan window).
    pub addrs: Vec<u8>,
    /// Time the plugin took (load and scan).
    pub took: Duration,
    /// Why it found nothing, if it failed; a skipped claim is noted here too.
    pub error: Option<String>,
}

/// Run the scan of every candidate plugin on every bus; the devices found and
/// one outcome per plugin and bus.
pub fn scan_buses(
    catalog: &Catalog,
    buses: &[BusHandle],
    opts: &ScanBudget,
) -> (Vec<Present>, Vec<ScanOutcome>) {
    let mut present = Vec::new();
    let mut outcomes = Vec::new();
    for bus in buses {
        let mut claimed: BTreeSet<u8> = BTreeSet::new();
        for entry in catalog.smbus_candidates(bus.descriptor()) {
            let started = Instant::now();
            let window: BTreeSet<u8> = entry.meta.smbus_addresses.iter().copied().collect();
            let mut guard = GuardedBus::new(
                bus.clone(),
                Phase::Scan {
                    window: window.clone(),
                },
                opts.mode,
            );
            guard.deny_registers(entry.meta.smbus_denied_registers.iter().copied());
            let guard = guard.shared();
            let ropts = RuntimeOptions {
                plugin_root: Some(entry.root.clone()),
                smbus: Some(guard),
                module_stubs: false,
                ..RuntimeOptions::default()
            };
            let budget = opts.per_plugin;
            let path = entry.meta.source_path.clone();
            let result = run_isolated(move || -> Result<Vec<u8>, String> {
                let rt = PluginRuntime::new(&path, &ropts).map_err(|e| e.to_string())?;
                if let Some(e) = rt.load(budget).error {
                    return Err(format!("load: {e}"));
                }
                rt.call_scan(&window, budget).map_err(|e| format!("Scan: {e}"))
            });
            let mut outcome = ScanOutcome {
                bus: bus.descriptor().name.clone(),
                plugin: entry.meta.name.clone(),
                addrs: Vec::new(),
                took: started.elapsed(),
                error: None,
            };
            match result {
                Ok(Ok(addrs)) => {
                    let mut skipped = Vec::new();
                    for a in &addrs {
                        if !claimed.insert(*a) {
                            skipped.push(format!("0x{a:02x}"));
                            continue;
                        }
                        bus.allow(*a);
                        present.push(Present {
                            device: SessionDevice::smbus(bus.clone(), *a),
                            plugin: entry.clone(),
                        });
                    }
                    if !skipped.is_empty() {
                        let text = format!("already claimed by an earlier plugin: {}", skipped.join(", "));
                        tracing::warn!(bus = %outcome.bus, plugin = %outcome.plugin, "{text}");
                        outcome.error = Some(text);
                    }
                    outcome.addrs = addrs;
                }
                Ok(Err(e)) => outcome.error = Some(e),
                Err(panic) => {
                    outcome.error = Some(format!("plugin thread panicked: {}", panic_text(panic.as_ref())))
                }
            }
            outcomes.push(outcome);
        }
    }
    (present, outcomes)
}
