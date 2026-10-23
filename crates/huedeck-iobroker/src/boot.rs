// SPDX-License-Identifier: GPL-3.0-or-later
//! Start-up of the broker: check PawnIO, load the pinned `SmbusPIIX4` module
//! on one handle per port, select the ports under the SMBus mutex, and build
//! the [`Server`].
//!
//! Only the AMD FCH (PIIX4) is served for now: the reference machine's B550
//! board has it, and its two ports are `AMD PIIX4 Bus - 0x0b00` (the DIMMs)
//! and `0x0b20` (AUX). Intel (`SmbusI801`) can join the same way.

#[cfg_attr(not(windows), allow(unused_imports))]
use crate::pins::{MODULES_RELEASE, SMBUS_PIIX4};
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
    /// PIIX4 sleep mode: 0 (AlwaysBusy, the module's own default) unless
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

#[cfg(windows)]
pub use win::bootstrap;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::detect;
    use crate::mutex::{GlobalMutex, SmbusMutex};
    use crate::pawnio::{NtStatus, PawnIoDevice, PawnIoModule};
    use crate::piix4::{Piix4Bus, BLOCK_MAX};
    use crate::pins;
    use crate::server::{BrokerBus, HealthInfo, Server};
    use huedeck_protocol::broker::{BusInfo, BusKind, ModuleStatus};
    use std::time::Instant;

    /// The broker as it runs on this machine.
    pub type RealServer = Server<PawnIoModule, SmbusMutex>;

    fn load(blob: &[u8]) -> Result<PawnIoModule, String> {
        let device = PawnIoDevice::open().map_err(|s| format!("PawnIO: {s}"))?;
        device.load(SMBUS_PIIX4.name, blob).map_err(|s| match s {
            NtStatus::NOT_SUPPORTED | NtStatus::NOT_IMPLEMENTED => {
                format!("SmbusPIIX4 refused this chipset ({s}): no AMD FCH SMBus at 0x0B00")
            }
            other => format!("loading SmbusPIIX4: {other}"),
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
            piix4,
        })
    }

    /// Check PawnIO and open the buses. Errors are the text the user sees.
    pub fn bootstrap(opts: &BootOptions) -> Result<RealServer, String> {
        let detection = detect::probe(Some(&opts.modules_dir));
        if let Some(problem) = detection.status.problem() {
            return Err(problem);
        }
        let path = opts.modules_dir.join(SMBUS_PIIX4.file);
        let blob = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        pins::verify_against(&SMBUS_PIIX4, &blob).map_err(|e| format!("{}: {e}", path.display()))?;
        // Fail closed: no mutex, no SMBus.
        let mut mutex = SmbusMutex::create()?;
        let primary = load(&blob)?;
        let aux = load(&blob);
        mutex
            .acquire(1000)
            .map_err(|e| format!("{}: {e}", crate::mutex::SMBUS_MUTEX_NAME))?;
        let mut buses = Vec::new();
        let opened = open_port(primary, 0, opts.sleep_mode);
        match opened {
            Ok(bus) => buses.push(bus),
            Err(e) => {
                mutex.release();
                return Err(e);
            }
        }
        match aux.and_then(|m| open_port(m, 1, opts.sleep_mode)) {
            Ok(bus) => buses.push(bus),
            Err(e) => tracing::warn!("AUX SMBus (0x0b20) not served: {e}"),
        }
        mutex.release();
        let health = HealthInfo {
            pawnio_version: detection.status.version(),
            pawnio_edition: detection.status.edition(),
            ci_options: detection.ci_options,
            sleep_mode: opts.sleep_mode,
            modules: vec![ModuleStatus {
                name: SMBUS_PIIX4.name.into(),
                sha256: SMBUS_PIIX4.sha256.into(),
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
