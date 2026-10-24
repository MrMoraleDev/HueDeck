// SPDX-License-Identifier: GPL-3.0-or-later
//! Start-up of the broker: check PawnIO, find the chipset SMBus module that
//! drives this board, open its buses under the SMBus mutex, and build the
//! [`Server`](crate::server::Server).
//!
//! The pinned chipset modules are tried in [`CHIPSET_MODULES`] order
//! ([`first_chipset`]); each one checks the PCI ids itself and refuses a
//! chipset it does not drive with `STATUS_NOT_SUPPORTED`:
//!
//! * `SmbusPIIX4` (AMD FCH, 1022:790B): one handle per port, `AMD PIIX4 Bus -
//!   0x0b00` (the DIMMs) and `0x0b20` (AUX), as on the reference B550 board;
//! * `SmbusI801` (Intel PCH, 8086 class 0x0C05 at 00:1F.4 and the other
//!   slots it knows): one bus, `Intel ICH8 Bus - 0x<io base>` ([`crate::i801`]).
//!
//! A module whose blob is not installed (a setup from before `SmbusI801` was
//! pinned) is skipped like a refused chipset; a blob that fails its pin, or a
//! load failure of any other kind, stops the broker (fail closed).

use crate::pins::{Pin, MODULES_RELEASE};
#[cfg(windows)]
use crate::pins::{CHIPSET_MODULES, SMBUS_I801, SMBUS_PIIX4};
use huedeck_protocol::broker::{BusInfo, BusKind};
use std::path::PathBuf;

/// Where `huedeck-setup --smbus` installs the pinned modules.
pub fn default_modules_dir() -> PathBuf {
    program_files()
        .join("HueDeck")
        .join("pawnio")
        .join(MODULES_RELEASE)
}

/// The 64-bit Program Files folder, from `ProgramFilesDir` under
/// `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion` (what the known folder
/// `FOLDERID_ProgramFiles` resolves to), never from the `ProgramFiles`
/// environment variable, which the user controls. `C:\Program Files` when
/// the value cannot be read.
pub fn program_files() -> PathBuf {
    #[cfg(windows)]
    if let Some(dir) = registry::program_files_dir() {
        return dir;
    }
    PathBuf::from(r"C:\Program Files")
}

#[cfg(windows)]
mod registry {
    use std::path::PathBuf;
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    pub fn program_files_dir() -> Option<PathBuf> {
        let mut buf = [0u16; 520];
        let mut size = std::mem::size_of_val(&buf) as u32;
        // SAFETY: constant NUL-terminated key and value names; `buf` is
        // writable for `size` bytes and `size` is a valid in/out pointer.
        let err = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!(r"SOFTWARE\Microsoft\Windows\CurrentVersion"),
                w!("ProgramFilesDir"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if err.is_err() {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let dir = String::from_utf16(&buf[..len]).ok()?;
        (!dir.is_empty()).then(|| PathBuf::from(dir))
    }
}

/// `ioctl_set_sleep_mode` 0: poll with `KeStallExecutionProcessor` (no timer tick per transfer).
pub const SLEEP_MODE_ALWAYS_BUSY: u8 = 0;

/// How the broker starts.
#[derive(Clone, Debug)]
pub struct BootOptions {
    /// Directory of the pinned module blobs.
    pub modules_dir: PathBuf,
    /// Refuse every write with `DryRun`.
    pub dry_run: bool,
    /// Module sleep mode (PIIX4 and I801 alike): 0 (AlwaysBusy, the modules' own default) unless
    /// told otherwise. ShortBusy (1) and AlwaysSleep (2) wait with
    /// `KeDelayExecutionThread`, which on Windows lasts at least one timer
    /// tick (15.6 ms unless some program raised the timer resolution) per
    /// transfer. On the test machine the Crucial `Initialize()` (~180
    /// transfers) took about 3 s with ShortBusy, ~16 ms per transfer, the
    /// timer tick (the pipe's 1 ms sleep under a lease, see `pipe`, cost a
    /// tick too). AlwaysBusy polls with
    /// `KeStallExecutionProcessor` instead, bounded by the module's 64 ms
    /// timeout: each transfer costs its bus time (0.5-1.3 ms at 93.75 kHz),
    /// spent on the broker's thread (research §8 R1, docs/adr/0004).
    pub sleep_mode: u8,
}

impl Default for BootOptions {
    fn default() -> Self {
        Self {
            modules_dir: default_modules_dir(),
            dry_run: false,
            sleep_mode: SLEEP_MODE_ALWAYS_BUSY,
        }
    }
}

/// `AMD PIIX4 Bus - 0x0b00` / `0x0b20`.
pub fn piix4_bus_name(port: u8) -> String {
    format!(
        "AMD PIIX4 Bus - 0x{:04x}",
        crate::piix4::PORT_BASES[usize::from(port.min(1))]
    )
}

/// Why a chipset module was not loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadFailure {
    /// The module refused this chipset, or its blob is not installed: try the next.
    NotThisChipset(String),
    /// Anything else (a blob failing its pin, PawnIO refusing the load): stop.
    Failed(String),
}

/// The first module of `pins` that `load` loads. Modules that are not for
/// this chipset are skipped; any other failure is returned at once.
pub fn first_chipset<M>(
    pins: &'static [Pin],
    mut load: impl FnMut(&'static Pin) -> Result<M, LoadFailure>,
) -> Result<(&'static Pin, M), String> {
    let mut skipped = Vec::new();
    for pin in pins {
        match load(pin) {
            Ok(module) => return Ok((pin, module)),
            Err(LoadFailure::NotThisChipset(why)) => {
                tracing::info!("{}: {why}", pin.name);
                skipped.push(format!("{}: {why}", pin.name));
            }
            Err(LoadFailure::Failed(why)) => return Err(why),
        }
    }
    Err(format!(
        "no supported chipset SMBus (AMD FCH or Intel PCH) on this machine ({})",
        skipped.join("; ")
    ))
}

/// What `ListBuses` reports for the Intel SMBus `SmbusI801` identified.
pub fn i801_bus_info(identity: &crate::piix4::Identity) -> BusInfo {
    BusInfo {
        id: 0,
        name: crate::i801::bus_name(identity),
        kind: BusKind::Intel,
        vendor: identity.vendor,
        device: identity.device,
        sub_vendor: identity.sub_vendor,
        sub_device: identity.sub_device,
        max_block_len: crate::piix4::BLOCK_MAX as u8,
    }
}

#[cfg(windows)]
pub use win::bootstrap;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::detect;
    use crate::mutex::{GlobalMutex, SmbusMutex};
    use crate::pawnio::{NtStatus, PawnIoDevice, PawnIoModule};
    use crate::i801::{self, I801Bus};
    use crate::piix4::{Piix4Bus, BLOCK_MAX};
    use crate::pins;
    use crate::server::{BrokerBus, HealthInfo, Server};
    use huedeck_protocol::broker::ModuleStatus;
    use std::time::Instant;

    /// The broker as it runs on this machine.
    pub type RealServer = Server<PawnIoModule, SmbusMutex>;

    /// The blob of `pin` from `dir`, checked against the pin.
    fn read_pinned(dir: &std::path::Path, pin: &Pin) -> Result<Vec<u8>, LoadFailure> {
        let path = dir.join(pin.file);
        let blob = std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                LoadFailure::NotThisChipset(format!("{} is not installed", path.display()))
            }
            _ => LoadFailure::Failed(format!("{}: {e}", path.display())),
        })?;
        pins::verify_against(pin, &blob)
            .map_err(|e| LoadFailure::Failed(format!("{}: {e}", path.display())))?;
        Ok(blob)
    }

    fn load(pin: &Pin, blob: &[u8]) -> Result<PawnIoModule, LoadFailure> {
        let device = PawnIoDevice::open().map_err(|s| LoadFailure::Failed(format!("PawnIO: {s}")))?;
        device.load(pin.name, blob).map_err(|s| match s {
            NtStatus::NOT_SUPPORTED | NtStatus::NOT_IMPLEMENTED => {
                LoadFailure::NotThisChipset(format!("refused this chipset ({s})"))
            }
            other => LoadFailure::Failed(format!("loading {}: {other}", pin.name)),
        })
    }

    /// The Intel SMBus: the SPD write protection first, then the identity
    /// and the sleep mode. Caller holds the SMBus mutex.
    fn open_i801(module: PawnIoModule, sleep_mode: u8) -> Result<BrokerBus<PawnIoModule>, String> {
        let mut bus = I801Bus::new(module);
        match bus.write_protection() {
            Ok(true) => tracing::info!("Intel SMBus: SPD write protection (SPD_WD) is on"),
            Ok(false) => tracing::warn!(
                target: "huedeck::audit",
                "Intel SMBus: SPD write protection (SPD_WD) is off; only the broker's SPD write denial applies"
            ),
            Err(e) => tracing::warn!("Intel SMBus: SPD write protection unknown: {e}"),
        }
        let identity = bus
            .identity()
            .map_err(|e| format!("Intel SMBus identity: {e}"))?;
        i801::check_identity(&identity)?;
        match i801::known_device(identity.vendor, identity.device) {
            Some(chipset) => tracing::info!(?identity, chipset, "Intel SMBus ready"),
            None => tracing::warn!(
                ?identity,
                "Intel SMBus {:04x} is not a known PCH SMBus function; served, as SmbusI801 accepted it",
                identity.device
            ),
        }
        bus.set_sleep_mode(sleep_mode)
            .map_err(|e| format!("Intel SMBus sleep mode: {e}"))?;
        Ok(BrokerBus {
            info: i801_bus_info(&identity),
            chip: bus.into(),
        })
    }

    /// One port: select it (then put the mux back as found), read its
    /// identity, set the sleep mode. Caller holds the SMBus mutex.
    fn open_port(module: PawnIoModule, port: u8, sleep_mode: u8) -> Result<BrokerBus<PawnIoModule>, String> {
        let mut piix4 = Piix4Bus::new(module, port).map_err(|e| e.to_string())?;
        let previous = piix4.select_port().map_err(|e| format!("port {port}: {e}"))?;
        let identity = piix4
            .identity()
            .map_err(|e| format!("port {port} identity: {e}"))?;
        piix4
            .set_sleep_mode(sleep_mode)
            .map_err(|e| format!("port {port} sleep mode: {e}"))?;
        piix4
            .restore_port(previous)
            .map_err(|e| format!("port {port}: {e}"))?;
        tracing::info!(port, ?identity, "PIIX4 port ready");
        Ok(BrokerBus {
            info: BusInfo {
                id: port,
                name: piix4_bus_name(port),
                kind: BusKind::Amd,
                vendor: identity.vendor,
                device: identity.device,
                sub_vendor: identity.sub_vendor,
                sub_device: identity.sub_device,
                max_block_len: BLOCK_MAX as u8,
            },
            chip: piix4.into(),
        })
    }

    /// Check PawnIO and open the buses. Errors are the text the user sees.
    pub fn bootstrap(opts: &BootOptions) -> Result<RealServer, String> {
        let detection = detect::probe(Some(&opts.modules_dir));
        if let Some(problem) = detection.status.problem() {
            return Err(problem);
        }
        // Fail closed: no mutex, no SMBus.
        let mut mutex = SmbusMutex::create()?;
        let (pin, (blob, first)) = first_chipset(CHIPSET_MODULES, |pin| {
            let blob = read_pinned(&opts.modules_dir, pin)?;
            let module = load(pin, &blob)?;
            Ok((blob, module))
        })?;
        // The PIIX4's AUX port gets a handle of its own.
        let aux = (pin.name == SMBUS_PIIX4.name).then(|| {
            load(pin, &blob).map_err(|e| match e {
                LoadFailure::NotThisChipset(s) | LoadFailure::Failed(s) => s,
            })
        });
        mutex
            .acquire(1000)
            .map_err(|e| format!("{}: {e}", crate::mutex::SMBUS_MUTEX_NAME))?;
        let mut buses = Vec::new();
        let opened = if pin.name == SMBUS_I801.name {
            open_i801(first, opts.sleep_mode)
        } else {
            open_port(first, 0, opts.sleep_mode)
        };
        match opened {
            Ok(bus) => buses.push(bus),
            Err(e) => {
                mutex.release();
                return Err(e);
            }
        }
        if let Some(aux) = aux {
            match aux.and_then(|m| open_port(m, 1, opts.sleep_mode)) {
                Ok(bus) => buses.push(bus),
                Err(e) => tracing::warn!("AUX SMBus (0x0b20) not served: {e}"),
            }
        }
        mutex.release();
        let health = HealthInfo {
            pawnio_version: detection.status.version(),
            pawnio_edition: detection.status.edition(),
            ci_options: detection.ci_options,
            sleep_mode: opts.sleep_mode,
            modules: vec![ModuleStatus {
                name: pin.name.into(),
                sha256: pin.sha256.into(),
                loaded: true,
            }],
        };
        Ok(Server::new(buses, mutex, health, opts.dry_run, Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_names_match_signalrgb_and_openrgb() {
        assert_eq!(piix4_bus_name(0), "AMD PIIX4 Bus - 0x0b00");
        assert_eq!(piix4_bus_name(1), "AMD PIIX4 Bus - 0x0b20");
        assert!(default_modules_dir().ends_with(r"HueDeck\pawnio\0.2.11") || !cfg!(windows));
    }

    #[test]
    fn chipset_modules_are_tried_in_order() {
        use crate::pins::CHIPSET_MODULES;
        let mut tried = Vec::new();
        let (pin, module) = first_chipset(CHIPSET_MODULES, |pin| {
            tried.push(pin.name);
            match pin.name {
                "SmbusPIIX4" => Err(LoadFailure::NotThisChipset("refused this chipset".into())),
                _ => Ok(7),
            }
        })
        .unwrap();
        assert_eq!((pin.name, module), ("SmbusI801", 7));
        assert_eq!(tried, ["SmbusPIIX4", "SmbusI801"]);
        // An AMD board never loads SmbusI801.
        let mut tried = Vec::new();
        let (pin, ()) = first_chipset(CHIPSET_MODULES, |pin| {
            tried.push(pin.name);
            Ok(())
        })
        .unwrap();
        assert_eq!((pin.name, tried), ("SmbusPIIX4", vec!["SmbusPIIX4"]));
    }

    #[test]
    fn chipset_failures_stop_or_explain() {
        use crate::pins::CHIPSET_MODULES;
        let err = first_chipset(CHIPSET_MODULES, |pin| {
            Err::<(), _>(LoadFailure::NotThisChipset(format!("{} is not installed", pin.file)))
        })
        .unwrap_err();
        assert!(err.contains("no supported chipset SMBus"), "{err}");
        assert!(err.contains("SmbusPIIX4.bin is not installed") && err.contains("SmbusI801.bin"));
        // A tampered PIIX4 blob stops the broker: the I801 is not tried.
        let mut tried = 0;
        let err = first_chipset(CHIPSET_MODULES, |_| {
            tried += 1;
            Err::<(), _>(LoadFailure::Failed("SHA-256 does not match the pin".into()))
        })
        .unwrap_err();
        assert_eq!((tried, err.as_str()), (1, "SHA-256 does not match the pin"));
    }

    #[test]
    fn i801_bus_info_is_an_intel_chipset_bus() {
        let identity = crate::piix4::decode_identity(&[
            u64::from_le_bytes(*b"i801\0\0\0\0"),
            0xEFA0,
            0x7D25_1462_7A23_8086,
        ]);
        let info = i801_bus_info(&identity);
        assert_eq!(info.name, "Intel ICH8 Bus - 0xefa0");
        assert_eq!(info.kind, BusKind::Intel);
        assert!(info.kind.is_chipset() && info.kind.is_dimm_bus());
        assert_eq!(
            (info.vendor, info.device, info.max_block_len),
            (0x8086, 0x7A23, 32)
        );
    }

    #[test]
    fn piix4_polls_without_a_timer_tick_per_transfer() {
        assert_eq!(BootOptions::default().sleep_mode, SLEEP_MODE_ALWAYS_BUSY);
        assert_eq!(SLEEP_MODE_ALWAYS_BUSY, 0);
    }

    #[test]
    #[cfg(windows)]
    fn program_files_comes_from_the_registry() {
        let dir = program_files();
        assert!(dir.is_absolute() && dir.is_dir(), "{}", dir.display());
        assert!(!dir.to_string_lossy().contains("(x86)"), "the 64-bit folder");
    }
}
