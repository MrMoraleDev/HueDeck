// SPDX-License-Identifier: GPL-3.0-or-later
//! The SMBus buses of a hardware run and the devices the SMBus plugins find on them.
//!
//! SMBus is on unless turned off (`lightloom --no-smbus`, the daemon's
//! `smbus.enabled = false`). The buses are the NVIDIA GPUs' I2C ports
//! through NvAPI, reached from this (unelevated) process, and the chipset
//! SMBus (RAM) served by the privileged Lightloom I/O broker over its named
//! pipe, used only when the broker answers and reports itself healthy (same
//! protocol, official PawnIO 2.2.0 or later, its module loaded). This
//! process never opens PawnIO: when the broker is not there,
//! [`SmbusHardware::notes`] says what is missing (PawnIO not installed, too
//! old, or the broker not registered), from PawnIO's uninstall entry.
//!
//! Discovery runs plugins' `Scan(bus)`, which may write; it is refused while
//! SignalRGB, OpenRGB or a vendor lighting service runs
//! ([`crate::conflicts::smbus_blockers`]), with no bypass. The callers hold
//! [`crate::conflicts::claim_smbus`] (one Lightloom process on the buses) and
//! look for those programs again while the devices run.

use crate::conflicts::smbus_blockers;
use crate::hardware::matches_filter;
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::discover::{scan_buses, ScanBudget, ScanOutcome};
use lightloom_plugin_host::manager::Present;
use lightloom_plugin_host::smbus::BusHandle;

/// Why SMBus discovery did not run.
pub const BLOCKED: &str = "is running: Lightloom never touches SMBus/GPU I2C alongside it. Close it first";

/// The SMBus buses of this machine (kept for the whole run: the NvAPI worker lives here).
pub struct SmbusHardware {
    buses: Vec<BusHandle>,
    nvapi: Option<lightloom_nvapi::NvapiHub>,
    notes: Vec<String>,
}

impl std::fmt::Debug for SmbusHardware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmbusHardware")
            .field("buses", &self.buses)
            .field("nvapi", &self.nvapi.is_some())
            .field("notes", &self.notes)
            .finish()
    }
}

impl SmbusHardware {
    /// Open the buses: one per NVIDIA GPU (NvAPI), then the chipset buses of
    /// the I/O broker. Nothing is written; NvAPI is initialised and the GPUs
    /// enumerated, the broker asked for its bus list.
    pub fn open() -> Self {
        let mut notes = Vec::new();
        let (mut buses, nvapi) = match lightloom_nvapi::NvapiHub::start() {
            Ok(hub) => {
                let buses: Vec<BusHandle> = hub
                    .buses()
                    .into_iter()
                    .map(|b| BusHandle::new(Box::new(b)))
                    .collect();
                if buses.is_empty() {
                    notes.push("NvAPI found no NVIDIA GPU".to_string());
                }
                (buses, Some(hub))
            }
            Err(e) => {
                notes.push(format!(
                    "No NVIDIA driver / NvAPI unavailable ({e}): no GPU lighting"
                ));
                (Vec::new(), None)
            }
        };
        buses.extend(chipset_buses(&mut notes));
        Self { buses, nvapi, notes }
    }

    /// Given buses (tests, mock runs).
    pub fn from_buses(buses: Vec<BusHandle>) -> Self {
        Self {
            buses,
            nvapi: None,
            notes: Vec::new(),
        }
    }

    /// The buses.
    pub fn buses(&self) -> &[BusHandle] {
        &self.buses
    }

    /// What is missing and why (shown to the user).
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Scan every bus with the SMBus plugins of `catalog` and return the
    /// devices found (after `filters`) and one outcome per plugin and bus.
    /// Fails while a program of [`smbus_blockers`] runs.
    pub fn targets(
        &self,
        catalog: &Catalog,
        filters: &[String],
        budget: &ScanBudget,
    ) -> anyhow::Result<(Vec<Present>, Vec<ScanOutcome>)> {
        let blockers = smbus_blockers()?;
        if !blockers.is_empty() {
            anyhow::bail!("{} {BLOCKED}", blockers.join(", "));
        }
        Ok(self.scan(catalog, filters, budget))
    }

    /// [`SmbusHardware::targets`] without the conflict check (mock buses only).
    pub fn scan(
        &self,
        catalog: &Catalog,
        filters: &[String],
        budget: &ScanBudget,
    ) -> (Vec<Present>, Vec<ScanOutcome>) {
        let (present, outcomes) = scan_buses(catalog, &self.buses, budget);
        for o in &outcomes {
            match &o.error {
                Some(e) => tracing::warn!(bus = %o.bus, plugin = %o.plugin, "SMBus scan: {e}"),
                None => tracing::info!(bus = %o.bus, plugin = %o.plugin, found = ?o.addrs, "SMBus scan"),
            }
        }
        let present = present
            .into_iter()
            .filter(|p| matches_filter(&p.device.key, filters))
            .collect();
        (present, outcomes)
    }
}

/// The chipset buses of the Lightloom I/O broker; when it does not answer, a
/// note says why (from PawnIO's uninstall entry; the device is never opened).
#[cfg(windows)]
fn chipset_buses(notes: &mut Vec<String>) -> Vec<BusHandle> {
    use lightloom_hal_windows::broker::{broker_buses, PipeTransport};
    use lightloom_hal_windows::pawnio::{message, pawnio_presence};
    let transport = match PipeTransport::shared() {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            notes.extend(message(&pawnio_presence(), false));
            return Vec::new();
        }
        Err(e) => {
            notes.push(format!(
                "Lightloom I/O broker not reachable ({e}): no RAM lighting (another Lightloom process may be using it)"
            ));
            return Vec::new();
        }
    };
    match broker_buses(transport) {
        Ok(buses) => {
            if buses.is_empty() {
                notes.push("the Lightloom I/O broker serves no SMBus bus: no RAM lighting".into());
            }
            buses.into_iter().map(|b| BusHandle::new(Box::new(b))).collect()
        }
        Err(e) => {
            notes.push(format!("Lightloom I/O broker: {e}: no RAM lighting"));
            Vec::new()
        }
    }
}

/// No chipset SMBus outside Windows yet (Linux: `/dev/i2c-*` later).
#[cfg(not(windows))]
fn chipset_buses(_notes: &mut Vec<String>) -> Vec<BusHandle> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::mock::{MockSmbus, TransactionRecorder};
    use lightloom_hal::{PciIds, SmbusBusKind};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn mock_buses_are_scanned_and_filtered() {
        let mut bus = MockSmbus::new(
            "NVIDIA GeForce RTX 3060",
            SmbusBusKind::Nvidia,
            TransactionRecorder::new(),
        )
        .with_pci(PciIds {
            vendor: 0x10DE,
            device: 0x2504,
            sub_vendor: 0x1462,
            sub_device: 0x3903,
        })
        .with_instance("pci01");
        bus.add_device(0x68, []);
        let hw = SmbusHardware::from_buses(vec![BusHandle::new(Box::new(bus))]);
        let catalog = Catalog::scan(
            &[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")],
            Duration::from_secs(10),
        );
        let (present, outcomes) = hw.scan(&catalog, &[], &ScanBudget::default());
        assert_eq!(present.len(), 1, "{outcomes:?}");
        assert_eq!(present[0].device.key, "smbus:10de:2504:1462:3903@pci01:68");
        let (filtered, _) = hw.scan(&catalog, &["1462:".into()], &ScanBudget::default());
        assert!(filtered.is_empty());
        let (kept, _) = hw.scan(&catalog, &["smbus:10de".into()], &ScanBudget::default());
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn ram_and_gpu_buses_scan_together() {
        let rec = TransactionRecorder::new();
        let mut gpu = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec.clone())
            .with_pci(PciIds {
                vendor: 0x10DE,
                device: 0x2504,
                sub_vendor: 0x1462,
                sub_device: 0x3903,
            })
            .with_instance("pci01");
        gpu.add_device(0x68, []);
        let mut ram = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec)
            .with_pci(PciIds {
                vendor: 0x1022,
                device: 0x790B,
                sub_vendor: 0x1022,
                sub_device: 0x790B,
            })
            .with_instance("0b00");
        // Two Crucial Ballistix DDR4 modules: their SPDs name Crucial
        // Technology (JEP106 bank 6, 0x9B), which the host's SPD check needs.
        let mut spd = vec![0u8; 512];
        spd[2] = 0x0C;
        spd[320] = 0x85;
        spd[321] = 0x9B;
        ram.add_ddr4_spd(0x50, &spd);
        ram.add_ddr4_spd(0x51, &spd);
        for addr in [0x39, 0x3A] {
            let ident = (0..16u8).map(|i| (0xA0 + i, i));
            let micron = b"Micron\0"
                .iter()
                .enumerate()
                .map(|(i, b)| (0x1025 + i as u16, *b));
            ram.add_indexed_device(addr, ident, micron);
        }
        let hw =
            SmbusHardware::from_buses(vec![BusHandle::new(Box::new(gpu)), BusHandle::new(Box::new(ram))]);
        let catalog = Catalog::scan(
            &[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")],
            Duration::from_secs(10),
        );
        let (present, outcomes) = hw.scan(&catalog, &[], &ScanBudget::default());
        let keys: Vec<&str> = present.iter().map(|p| p.device.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "smbus:10de:2504:1462:3903@pci01:68",
                "smbus:1022:790b:1022:790b@0b00:39",
                "smbus:1022:790b:1022:790b@0b00:3a"
            ],
            "{outcomes:?}"
        );
    }
}
