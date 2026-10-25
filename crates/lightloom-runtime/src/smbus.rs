// SPDX-License-Identifier: GPL-3.0-or-later
//! The SMBus buses of a hardware run and the devices the SMBus plugins find on them.
//!
//! SMBus is on unless turned off (`lightloom --no-smbus`, the daemon's
//! `smbus.enabled = false`). The buses are the NVIDIA GPUs' I2C ports
//! through NvAPI and the AMD GPUs' I2C lines through ADL (`atiadlxx.dll`),
//! both reached from this (unelevated) process, and the chipset
//! SMBus (RAM) served by the privileged Lightloom I/O broker over its named
//! pipe, used only when the broker answers and reports itself healthy (same
//! protocol, official PawnIO 2.2.0 or later, its module loaded). This
//! process never opens PawnIO: when the broker is not there,
//! [`SmbusHardware::notes`] says what is missing (PawnIO not installed, too
//! old, or the broker not registered), from PawnIO's uninstall entry.
//!
//! On Linux every bus (chipset SMBus and the GPUs' I2C ports alike) is a
//! `/dev/i2c-*` adapter opened by this process (`lightloom_hal_linux::i2c`:
//! udev rules and group `i2c`, no helper, no NvAPI or ADL), with the same policy.
//!
//! Discovery runs plugins' `Scan(bus)`, which may write; it is refused while
//! SignalRGB, OpenRGB or a vendor lighting service runs
//! ([`crate::conflicts::smbus_blockers`]), with no bypass. The callers hold
//! [`crate::conflicts::claim_smbus`] (one Lightloom process on the buses) and
//! look for those programs again while the devices run.

use crate::conflicts::smbus_blockers;
use crate::hardware::matches_filter;
use crate::pawnio::BrokerHealth;
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::discover::{scan_buses, ScanBudget, ScanOutcome};
use lightloom_plugin_host::manager::Present;
use lightloom_plugin_host::smbus::BusHandle;

/// Why SMBus discovery did not run.
pub const BLOCKED: &str = "is running: Lightloom never touches SMBus/GPU I2C alongside it. Close it first";

/// The SMBus buses of this machine (kept for the whole run: the NvAPI and ADL workers live here).
pub struct SmbusHardware {
    buses: Vec<BusHandle>,
    nvapi: Option<std::sync::Arc<lightloom_nvapi::NvapiHub>>,
    adl: Option<lightloom_adl::AdlHub>,
    notes: Vec<String>,
    broker: Option<BrokerHealth>,
}

impl std::fmt::Debug for SmbusHardware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmbusHardware")
            .field("buses", &self.buses)
            .field("nvapi", &self.nvapi.is_some())
            .field("adl", &self.adl.is_some())
            .field("notes", &self.notes)
            .field("broker", &self.broker)
            .finish()
    }
}

impl SmbusHardware {
    /// Open the buses: one per NVIDIA GPU (NvAPI), one per AMD GPU (ADL),
    /// then the chipset buses of the I/O broker (Linux: the `/dev/i2c-*`
    /// adapters, GPUs included). Nothing is written; NvAPI and ADL are
    /// initialised and the GPUs enumerated, the broker asked for its bus list
    /// (Linux: the nodes opened, `I2C_FUNCS`).
    pub fn open() -> Self {
        let mut notes = Vec::new();
        let (mut buses, nvapi) = if cfg!(target_os = "linux") {
            // GPU I2C comes from /dev/i2c-* there (chipset_buses).
            (Vec::new(), None)
        } else {
            // The process-wide hub: the sensors share its worker thread.
            match lightloom_nvapi::NvapiHub::shared() {
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
                        "No NVIDIA driver / NvAPI unavailable ({e}): no NVIDIA GPU lighting"
                    ));
                    (Vec::new(), None)
                }
            }
        };
        let (amd, adl) = if cfg!(target_os = "linux") {
            // AMD GPU I2C comes from /dev/i2c-* there too.
            (Vec::new(), None)
        } else {
            amd_gpu_buses(lightloom_adl::AdlHub::start(), &mut notes)
        };
        buses.extend(amd);
        let mut broker = None;
        buses.extend(chipset_buses(&mut notes, &mut broker));
        Self {
            buses,
            nvapi,
            adl,
            notes,
            broker,
        }
    }

    /// Given buses (tests, mock runs).
    pub fn from_buses(buses: Vec<BusHandle>) -> Self {
        Self {
            buses,
            nvapi: None,
            adl: None,
            notes: Vec::new(),
            broker: None,
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

    /// The I/O broker's `Health` answer, when it answered.
    pub fn broker(&self) -> Option<&BrokerHealth> {
        self.broker.as_ref()
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

/// The buses of a started (or failed) ADL hub, one per AMD GPU, kind
/// `AmdGpu`. No AMD driver (`atiadlxx.dll` absent, the usual case on NVIDIA
/// and Intel machines) adds no note, only a debug line; any other failure does.
fn amd_gpu_buses(
    started: Result<lightloom_adl::AdlHub, lightloom_adl::AdlError>,
    notes: &mut Vec<String>,
) -> (Vec<BusHandle>, Option<lightloom_adl::AdlHub>) {
    match started {
        Ok(hub) => {
            let buses: Vec<BusHandle> = hub
                .buses()
                .into_iter()
                .map(|b| BusHandle::new(Box::new(b)))
                .collect();
            if buses.is_empty() {
                tracing::debug!("ADL found no AMD GPU");
                return (buses, None);
            }
            (buses, Some(hub))
        }
        Err(lightloom_adl::AdlError::NotAvailable) => {
            tracing::debug!("no AMD graphics driver (atiadlxx.dll): no AMD GPU buses");
            (Vec::new(), None)
        }
        Err(e) => {
            notes.push(format!("AMD ADL unavailable ({e}): no AMD GPU lighting"));
            (Vec::new(), None)
        }
    }
}

/// The chipset buses of the Lightloom I/O broker; when it does not answer, a
/// note says why (from PawnIO's uninstall entry; the device is never opened).
#[cfg(windows)]
fn chipset_buses(notes: &mut Vec<String>, broker: &mut Option<BrokerHealth>) -> Vec<BusHandle> {
    use lightloom_hal_windows::broker::{broker_buses_after, broker_health, PipeTransport};
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
    let health = match broker_health(&transport) {
        Ok(h) => h,
        Err(e) => {
            notes.push(format!("Lightloom I/O broker: {e}: no RAM lighting"));
            return Vec::new();
        }
    };
    *broker = Some(BrokerHealth::from_report(&health, 0));
    match broker_buses_after(&health, transport) {
        Ok(buses) => {
            if buses.is_empty() {
                notes.push("the Lightloom I/O broker serves no SMBus bus: no RAM lighting".into());
            }
            *broker = Some(BrokerHealth::from_report(&health, buses.len()));
            buses.into_iter().map(|b| BusHandle::new(Box::new(b))).collect()
        }
        Err(e) => {
            notes.push(format!("Lightloom I/O broker: {e}: no RAM lighting"));
            Vec::new()
        }
    }
}

/// Linux: every `/dev/i2c-*` adapter Lightloom uses, chipset and GPU
/// (`lightloom_hal_linux::i2c::open_buses`); notes say which could not be
/// opened (udev rules, group `i2c`, the i2c-dev module).
#[cfg(target_os = "linux")]
fn chipset_buses(notes: &mut Vec<String>, _broker: &mut Option<BrokerHealth>) -> Vec<BusHandle> {
    let (buses, missing) = lightloom_hal_linux::i2c::open_buses();
    notes.extend(missing);
    if buses.is_empty() {
        notes.push("no SMBus / GPU I2C adapter available: no RAM or GPU I2C lighting".into());
    }
    buses.into_iter().map(|b| BusHandle::new(Box::new(b))).collect()
}

/// No SMBus on other systems.
#[cfg(not(any(windows, target_os = "linux")))]
fn chipset_buses(_notes: &mut Vec<String>, _broker: &mut Option<BrokerHealth>) -> Vec<BusHandle> {
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

    fn rx6700xt() -> lightloom_adl::GpuInfo {
        lightloom_adl::GpuInfo {
            name: "AMD Radeon RX 6700 XT".into(),
            pci: PciIds {
                vendor: 0x1002,
                device: 0x73DF,
                sub_vendor: 0x1DA2,
                sub_device: 0xE445,
            },
            bus: 3,
        }
    }

    #[test]
    fn a_missing_amd_driver_adds_no_bus_and_no_note() {
        use lightloom_adl::AdlError;
        let mut notes = Vec::new();
        let (buses, hub) = amd_gpu_buses(Err(AdlError::NotAvailable), &mut notes);
        assert!(buses.is_empty() && hub.is_none() && notes.is_empty());
        let (buses, _) = amd_gpu_buses(
            Err(AdlError::MissingFunction("ADL2_Display_WriteAndReadI2C")),
            &mut notes,
        );
        assert!(buses.is_empty());
        assert_eq!(notes.len(), 1, "a broken driver is worth a note: {notes:?}");
        // The real loader on this machine: with no atiadlxx.dll, nothing at all.
        if !lightloom_adl::is_available() {
            let mut notes = Vec::new();
            let (buses, hub) = amd_gpu_buses(lightloom_adl::AdlHub::start(), &mut notes);
            assert!(buses.is_empty() && hub.is_none() && notes.is_empty());
        }
    }

    #[test]
    fn amd_gpu_buses_are_scanned_by_amd_gpu_plugins() {
        let fake = lightloom_adl::FakeAdlI2c::new(vec![rx6700xt()]).add_device(0, 0x28, []);
        let log = fake.log();
        let hub = lightloom_adl::AdlHub::with_transport(fake).unwrap();
        let mut notes = Vec::new();
        let (buses, hub) = amd_gpu_buses(Ok(hub), &mut notes);
        assert!(hub.is_some() && notes.is_empty());
        assert_eq!(buses.len(), 1);
        assert_eq!(buses[0].descriptor().kind, SmbusBusKind::AmdGpu);
        let hw = SmbusHardware::from_buses(buses);
        let catalog = Catalog::scan(
            &[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")],
            Duration::from_secs(10),
        );
        let (present, outcomes) = hw.scan(&catalog, &[], &ScanBudget::default());
        assert_eq!(present.len(), 1, "{outcomes:?}");
        assert_eq!(present[0].device.key, "smbus:1002:73df:1da2:e445@pci03:28");
        let seen = log.lock().unwrap();
        assert!(
            seen.iter().all(|t| t.addr == 0x28 && !t.plan.write),
            "the scan only read 0x28: {seen:?}"
        );
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
