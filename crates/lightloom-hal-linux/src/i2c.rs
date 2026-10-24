// SPDX-License-Identifier: GPL-3.0-or-later
//! SMBus / I2C through `/dev/i2c-*` (i2c-dev): adapter discovery from sysfs
//! and the [`SmbusBus`] of each adapter Lightloom uses.
//!
//! **Discovery** (docs/research/m7-linux-backend.md §3.1-§3.2) reads
//! `/sys/class/i2c-dev/i2c-N/device` (the adapter), its `name`, and the
//! first ancestor with `vendor` and `device` (the PCI function). The name
//! decides ([`classify`]):
//!
//! * `SMBus PIIX4 adapter port 0 at 0b00` and the aux `port 1 at 0b20` (AMD
//!   FCH): [`SmbusBusKind::Amd`], named `AMD PIIX4 Bus - 0x0b00` / `0x0b20`
//!   and keyed `…@0b00` / `…@0b20` as on Windows (same plugin device keys);
//!   ports 2-4 are left out (other parts on the board, as SignalRGB);
//! * `SMBus I801 adapter at efa0` (≤ 6.11), `SMBus I801 adapter at efa0` /
//!   `SMBus I801 IDF adapter at …` (6.12-6.14) and `SMBus I801 adapter at
//!   0000:00:1f.4` (6.15+; the I/O base then comes from BAR 4 of the PCI
//!   function): [`SmbusBusKind::Intel`]; the IDF adapters are left out;
//! * `NVIDIA i2c adapter 1 at 1:00.0` (NVIDIA driver): [`SmbusBusKind::Nvidia`],
//!   port 1 only (the RGB port, as through NvAPI), keyed `…@pci01` as NvAPI;
//! * `AMDGPU DM i2c OEM bus` (6.15+, the board's RGB bus) and `AMDGPU i2c
//!   bit bus OEM 0x97`: [`SmbusBusKind::AmdGpu`], keyed `…@pci03-oem` /
//!   `…@pci03-oem97`. The DDC (`AMDGPU DM i2c hw bus N`) and DP AUX
//!   (`AMDGPU DM aux hw bus N`) buses go to the monitors and are left out;
//! * everything else (AMD ASF, which cannot read; `i2c-nvidia-gpu`, the
//!   Type-C controller; DDC, touchpads, the EC) is ignored.
//!
//! **Policy** ([`I2cSmbus`]) is the Windows one, enforced here as well as in
//! the plugin host: addresses 0x03..=0x77; writes and Quick commands
//! refused by [`write_denied`] (SPD on every bus; write-protect, PMIC and the
//! EEPROM area on DIMM buses); on DIMM buses writes only at the I/O broker's
//! [`BROKER_WRITE_ALLOWLIST`] (the first-party RAM plugins' addresses), the
//! broker's own rule; blocks of 1..=32 bytes; chipset buses without block
//! reads or register-less transfers (SignalRGB and broker parity), GPU
//! buses with them when the adapter does plain I2C. Each operation is also
//! checked against the adapter's `I2C_FUNCS` mask, because i2c-dev sends an
//! `I2C_SMBUS` request without looking at it (and PIIX4 logs every refusal).
//! `I2C_SLAVE` is used, never `I2C_SLAVE_FORCE`: an address a kernel driver
//! owns answers `Busy`.
//!
//! **SPD** ([`SmbusBus::spd_read`], the host's SPD check of RAM plugins):
//! when the kernel has an EEPROM driver on the SPD (`ee1004`, instantiated
//! by `i2c-piix4` / `i2c-i801` since 6.11) the bytes come from its read-only
//! `eeprom` file and nothing is sent; otherwise the EE1004 page select (a
//! Send Byte to 0x36 / 0x37) is a step inside `spd_read`, page 0 is selected
//! again before it returns, and no plugin-reachable operation can send it
//! (0x30-0x37 stay denied to every write), as in the broker.
//!
//! Failures reach plugins as `-1` (chipset parity). Access is serialised by
//! the plugin host's per-bus queue and by the kernel's adapter lock per
//! transfer; `TransactionLock` needs nothing more (no other program on Linux
//! honours a cross-process SMBus mutex).
//!
//! Pure except the `cfg(target_os = "linux")` device and opener at the end,
//! so the parsing and the policy are tested on every OS ([`I2cDev`] fakes).

use crate::sysfs::Files;
use lightloom_hal::smbus::{
    validate_address, validate_spd_read, write_denied, PciIds, SmbusBus, SmbusBusDescriptor, SmbusBusKind,
    SMBUS_BLOCK_MAX, SPD_PAGE_LEN, SPD_SET_PAGE,
};
use lightloom_hal::{HalError, HalResult};
use lightloom_protocol::broker::BROKER_WRITE_ALLOWLIST;
use std::io;
use std::path::{Path, PathBuf};

/// `I2C_FUNC_*` bits of `linux/i2c.h`.
pub mod func {
    /// Plain I2C messages (`I2C_RDWR`).
    pub const I2C: u32 = 0x0000_0001;
    /// Quick command.
    pub const SMBUS_QUICK: u32 = 0x0001_0000;
    /// Receive Byte.
    pub const SMBUS_READ_BYTE: u32 = 0x0002_0000;
    /// Send Byte.
    pub const SMBUS_WRITE_BYTE: u32 = 0x0004_0000;
    /// Read Byte (with command).
    pub const SMBUS_READ_BYTE_DATA: u32 = 0x0008_0000;
    /// Write Byte (with command).
    pub const SMBUS_WRITE_BYTE_DATA: u32 = 0x0010_0000;
    /// Read Word.
    pub const SMBUS_READ_WORD_DATA: u32 = 0x0020_0000;
    /// Write Word.
    pub const SMBUS_WRITE_WORD_DATA: u32 = 0x0040_0000;
    /// Block Write (count prefixed).
    pub const SMBUS_WRITE_BLOCK_DATA: u32 = 0x0200_0000;
    /// I2C block read (command, then N bytes; no count).
    pub const SMBUS_READ_I2C_BLOCK: u32 = 0x0400_0000;
}

/// `I2C_SMBUS_*` transfer sizes.
mod size {
    pub const QUICK: u32 = 0;
    pub const BYTE: u32 = 1;
    pub const BYTE_DATA: u32 = 2;
    pub const WORD_DATA: u32 = 3;
    pub const BLOCK_DATA: u32 = 5;
    pub const I2C_BLOCK_DATA: u32 = 8;
}

/// `I2C_SMBUS_READ`.
const READ: u8 = 1;
/// `I2C_SMBUS_WRITE`.
const WRITE: u8 = 0;

/// `union i2c_smbus_data`: `byte` is `block[0]`, `word` `block[0..2]`
/// (native order), a block `block[0]` (length) then the data.
#[repr(C, align(2))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmbusData {
    /// The union's bytes.
    pub block: [u8; SMBUS_BLOCK_MAX + 2],
}

impl Default for SmbusData {
    fn default() -> Self {
        Self {
            block: [0; SMBUS_BLOCK_MAX + 2],
        }
    }
}

impl SmbusData {
    fn word(&self) -> u16 {
        u16::from_ne_bytes([self.block[0], self.block[1]])
    }
}

/// What an adapter's sysfs `name` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterClass {
    /// `SMBus PIIX4 adapter[ port N] at XXXX`.
    Piix4 {
        /// The port, when the name has one.
        port: Option<u8>,
        /// The I/O base.
        base: u16,
    },
    /// `SMBus I801[ IDF] adapter at X`.
    I801 {
        /// The I/O base, when the name carries it (before 6.15).
        base: Option<u16>,
        /// The IDF variant.
        idf: bool,
    },
    /// `NVIDIA i2c adapter N at B:SS.F`.
    Nvidia {
        /// The port.
        port: u8,
    },
    /// An AMDGPU OEM bus; the instance suffix (`oem`, `oem97`).
    AmdGpu(String),
    /// Any other adapter.
    Other,
}

fn hex_u16(s: &str) -> Option<u16> {
    let s = s.trim();
    let s = s.strip_prefix("0x").unwrap_or(s);
    (!s.is_empty() && s.len() <= 4)
        .then(|| u16::from_str_radix(s, 16).ok())
        .flatten()
}

/// Classify an adapter by its sysfs `name` (trailing whitespace ignored).
pub fn classify(name: &str) -> AdapterClass {
    let name = name.trim();
    if let Some(rest) = name.strip_prefix("SMBus PIIX4 adapter") {
        let (port, at) = match rest.strip_prefix(" port ") {
            Some(r) => match r.split_once(' ') {
                Some((p, at)) => (p.parse().ok(), at),
                None => return AdapterClass::Other,
            },
            None => (None, rest.trim_start()),
        };
        if rest.starts_with(" port ") && port.is_none() {
            return AdapterClass::Other;
        }
        return match at.strip_prefix("at ").and_then(hex_u16) {
            Some(base) => AdapterClass::Piix4 { port, base },
            None => AdapterClass::Other,
        };
    }
    if let Some(rest) = name.strip_prefix("SMBus I801") {
        let idf = rest.starts_with(" IDF");
        let Some((_, at)) = rest.split_once(" adapter at ") else {
            return AdapterClass::Other;
        };
        return AdapterClass::I801 { base: hex_u16(at), idf };
    }
    if let Some(rest) = name.strip_prefix("NVIDIA i2c adapter ") {
        return match rest.split_once(' ').map(|(p, _)| p.parse()) {
            Some(Ok(port)) => AdapterClass::Nvidia { port },
            _ => AdapterClass::Other,
        };
    }
    if name == "AMDGPU DM i2c OEM bus" {
        return AdapterClass::AmdGpu("oem".into());
    }
    if name == "AMDGPU i2c bit bus OEM 0x97" {
        return AdapterClass::AmdGpu("oem97".into());
    }
    AdapterClass::Other
}

/// One i2c-dev adapter as sysfs describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I2cAdapter {
    /// `N` of `/dev/i2c-N` (dynamic: it may change across boots).
    pub number: u32,
    /// `/dev/i2c-N`.
    pub devnode: String,
    /// The adapter directory (resolved), where its clients (`N-00aa`) live.
    pub dir: PathBuf,
    /// The kernel's adapter name.
    pub name: String,
    /// What the name says.
    pub class: AdapterClass,
    /// The PCI function above it, when there is one.
    pub pci: Option<PciFunction>,
}

/// The PCI function that owns an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PciFunction {
    /// Its sysfs directory (named after the address, `0000:01:00.0`).
    pub dir: PathBuf,
    /// Vendor, device, subsystem ids.
    pub ids: PciIds,
}

impl PciFunction {
    /// The PCI bus number from the directory name (`0000:01:00.0` → 1).
    pub fn bus(&self) -> Option<u8> {
        let name = self.dir.file_name()?.to_str()?;
        let mut parts = name.split(':');
        let (_domain, bus) = (parts.next()?, parts.next()?);
        u8::from_str_radix(bus, 16).ok()
    }
}

fn text(fs: &impl Files, path: &Path) -> Option<String> {
    Some(String::from_utf8_lossy(&fs.read(path)?).trim().to_string())
}

/// The first ancestor of `dir` with `vendor` and `device` (§3.2 step 4).
pub fn read_pci_parent(fs: &impl Files, dir: &Path) -> Option<PciFunction> {
    for d in dir.ancestors().skip(1) {
        let (Some(vendor), Some(device)) = (
            text(fs, &d.join("vendor")).and_then(|s| hex_u16(&s)),
            text(fs, &d.join("device")).and_then(|s| hex_u16(&s)),
        ) else {
            continue;
        };
        let sub = |n: &str| text(fs, &d.join(n)).and_then(|s| hex_u16(&s)).unwrap_or(0);
        return Some(PciFunction {
            dir: d.to_path_buf(),
            ids: PciIds {
                vendor,
                device,
                sub_vendor: sub("subsystem_vendor"),
                sub_device: sub("subsystem_device"),
            },
        });
    }
    None
}

/// The start of an I/O BAR of a PCI function (`resource`: one
/// `start end flags` line per BAR; i801 has its SMBus base in BAR 4).
pub fn read_io_bar(fs: &impl Files, pci_dir: &Path, bar: usize) -> Option<u16> {
    let resource = text(fs, &pci_dir.join("resource"))?;
    let line = resource.lines().nth(bar)?;
    let start = line.split_whitespace().next()?;
    let start = u64::from_str_radix(start.trim_start_matches("0x"), 16).ok()?;
    (start != 0 && start <= 0xFFFF).then_some(start as u16)
}

/// Every adapter under `class_dir` (normally `/sys/class/i2c-dev`), by number.
pub fn discover(fs: &impl Files, class_dir: &Path) -> Vec<I2cAdapter> {
    let mut out = Vec::new();
    for entry in fs.list(class_dir) {
        let Some(number) = entry.strip_prefix("i2c-").and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let class_entry = class_dir.join(&entry);
        let Some(dir) = fs.resolve(&class_entry.join("device")) else {
            continue;
        };
        // i2c-dev's own `name` is the adapter name; the adapter has it too.
        let Some(name) = text(fs, &class_entry.join("name")).or_else(|| text(fs, &dir.join("name"))) else {
            continue;
        };
        out.push(I2cAdapter {
            number,
            devnode: format!("/dev/{entry}"),
            class: classify(&name),
            pci: read_pci_parent(fs, &dir),
            dir,
            name,
        });
    }
    out.sort_by_key(|a| a.number);
    out
}

/// What Lightloom does with an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exposure {
    /// A bus for the SMBus plugins.
    Bus(SmbusBusDescriptor),
    /// A bus of a known kind left out, and why (shown to the user).
    Skipped(String),
    /// Not an RGB bus.
    Ignored,
}

fn chipset_kind(vendor: u16) -> SmbusBusKind {
    match vendor {
        // AMD, ATI (SB700/SB800), Hygon.
        0x1022 | 0x1002 | 0x1d94 => SmbusBusKind::Amd,
        0x8086 => SmbusBusKind::Intel,
        _ => SmbusBusKind::System,
    }
}

/// Decide what to do with `adapter` ([`I2cAdapter::pci`] supplies the ids;
/// `fs` the i801 I/O base on 6.15+).
pub fn expose(fs: &impl Files, adapter: &I2cAdapter) -> Exposure {
    let desc = |name: String, kind, pci: PciIds, instance: String| {
        Exposure::Bus(SmbusBusDescriptor {
            name,
            kind,
            pci,
            instance,
            max_block_len: SMBUS_BLOCK_MAX,
        })
    };
    let pci = adapter.pci.as_ref();
    match &adapter.class {
        AdapterClass::Piix4 { port, base } => {
            if matches!(port, Some(p) if *p > 1) {
                return Exposure::Skipped(format!(
                    "{} ({}): PIIX4 ports 2-4 are not lighting buses",
                    adapter.devnode, adapter.name
                ));
            }
            let ids = pci.map(|p| p.ids).unwrap_or_default();
            desc(
                format!("AMD PIIX4 Bus - 0x{base:04x}"),
                chipset_kind(ids.vendor),
                ids,
                format!("{base:04x}"),
            )
        }
        AdapterClass::I801 { idf: true, .. } => Exposure::Skipped(format!(
            "{} ({}): the i801 IDF function is not the DIMM bus",
            adapter.devnode, adapter.name
        )),
        AdapterClass::I801 { base, idf: false } => {
            let ids = pci.map(|p| p.ids).unwrap_or_default();
            let base = base.or_else(|| pci.and_then(|p| read_io_bar(fs, &p.dir, 4)));
            let Some(base) = base else {
                return Exposure::Skipped(format!(
                    "{} ({}): no I/O base for the i801 adapter",
                    adapter.devnode, adapter.name
                ));
            };
            desc(
                format!("Intel SMBus - 0x{base:04x}"),
                SmbusBusKind::Intel,
                ids,
                format!("{base:04x}"),
            )
        }
        AdapterClass::Nvidia { port } => {
            if *port != 1 {
                return Exposure::Ignored;
            }
            let Some((p, bus)) = pci.and_then(|p| Some((p, p.bus()?))) else {
                return Exposure::Skipped(format!("{} ({}): no PCI device", adapter.devnode, adapter.name));
            };
            desc(
                format!("NVIDIA GPU {}", p.dir.file_name().unwrap_or_default().to_string_lossy()),
                SmbusBusKind::Nvidia,
                p.ids,
                format!("pci{bus:02x}"),
            )
        }
        AdapterClass::AmdGpu(what) => {
            let Some((p, bus)) = pci.and_then(|p| Some((p, p.bus()?))) else {
                return Exposure::Skipped(format!("{} ({}): no PCI device", adapter.devnode, adapter.name));
            };
            desc(adapter.name.clone(), SmbusBusKind::AmdGpu, p.ids, format!("pci{bus:02x}-{what}"))
        }
        AdapterClass::Other => Exposure::Ignored,
    }
}

/// The transfers of one opened `/dev/i2c-N` (a fake in tests).
pub trait I2cDev: Send {
    /// `I2C_FUNCS`.
    fn funcs(&mut self) -> io::Result<u32>;
    /// `I2C_SLAVE` (never forced).
    fn set_slave(&mut self, addr: u8) -> io::Result<()>;
    /// `I2C_SMBUS` at the current slave address.
    fn smbus(&mut self, read_write: u8, command: u8, size: u32, data: &mut SmbusData) -> io::Result<()>;
    /// `I2C_RDWR` at `addr`: `write`, then `read` (either may be empty).
    fn rdwr(&mut self, addr: u8, write: &[u8], read: &mut [u8]) -> io::Result<()>;
}

/// Linux errno values the adapters return (`asm-generic/errno*.h`).
mod errno {
    pub const EPERM: i32 = 1;
    pub const ENXIO: i32 = 6;
    pub const EACCES: i32 = 13;
    pub const EBUSY: i32 = 16;
    pub const EOPNOTSUPP: i32 = 95;
    pub const EREMOTEIO: i32 = 121;
}

/// Whether `e` is the slave not acknowledging (PIIX4 / i801 `ENXIO`, I2C `EREMOTEIO`).
fn is_nack(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(errno::ENXIO | errno::EREMOTEIO))
}

/// An i2c-dev error as the HAL sees it: a bus failure is `-1` for plugins.
fn hal_error(e: io::Error, addr: u8) -> HalError {
    match e.raw_os_error() {
        Some(errno::EBUSY) => HalError::Busy(format!("0x{addr:02x} is owned by a kernel driver")),
        Some(errno::EPERM | errno::EACCES) => HalError::PermissionDenied(e.to_string()),
        Some(errno::EOPNOTSUPP) => HalError::Unsupported("transfer not supported by this adapter"),
        _ => HalError::Bus { status: -1 },
    }
}

/// One adapter as a [`SmbusBus`], with the Windows policy (module docs).
pub struct I2cSmbus<D, F> {
    descriptor: SmbusBusDescriptor,
    number: u32,
    dir: PathBuf,
    funcs: u32,
    dev: D,
    fs: F,
    slave: Option<u8>,
}

impl<D, F> std::fmt::Debug for I2cSmbus<D, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I2cSmbus")
            .field("descriptor", &self.descriptor)
            .field("number", &self.number)
            .field("funcs", &format_args!("{:#010x}", self.funcs))
            .finish_non_exhaustive()
    }
}

impl<D: I2cDev, F: Files + Send> I2cSmbus<D, F> {
    /// The bus `descriptor` of `adapter` over `dev`; reads `I2C_FUNCS`.
    pub fn new(descriptor: SmbusBusDescriptor, adapter: &I2cAdapter, mut dev: D, fs: F) -> io::Result<Self> {
        let funcs = dev.funcs()?;
        Ok(Self {
            descriptor,
            number: adapter.number,
            dir: adapter.dir.clone(),
            funcs,
            dev,
            fs,
            slave: None,
        })
    }

    /// The adapter's `I2C_FUNCS` mask.
    pub fn funcs(&self) -> u32 {
        self.funcs
    }

    fn kind(&self) -> SmbusBusKind {
        self.descriptor.kind
    }

    fn need(&self, bits: u32, what: &'static str) -> HalResult<()> {
        if self.funcs & bits == bits {
            Ok(())
        } else {
            Err(HalError::Unsupported(what))
        }
    }

    /// The write checks: [`write_denied`], then the broker allowlist on DIMM buses.
    fn check_write(&self, addr: u8) -> HalResult<()> {
        validate_address(addr)?;
        if write_denied(self.kind(), addr) {
            return Err(HalError::PermissionDenied(format!(
                "write to 0x{addr:02x} (SPD / EEPROM / write-protect / PMIC)"
            )));
        }
        if self.kind().is_dimm_bus() && !BROKER_WRITE_ALLOWLIST.contains(&addr) {
            return Err(HalError::PermissionDenied(format!(
                "write to 0x{addr:02x}: not a RAM lighting address"
            )));
        }
        Ok(())
    }

    fn check_block(len: usize) -> HalResult<()> {
        if (1..=SMBUS_BLOCK_MAX).contains(&len) {
            Ok(())
        } else {
            Err(HalError::InvalidArgument(format!("block of {len} bytes (1..=32)")))
        }
    }

    fn no_chipset(&self, what: &'static str) -> HalResult<()> {
        if self.kind().is_dimm_bus() {
            Err(HalError::Unsupported(what))
        } else {
            Ok(())
        }
    }

    fn select(&mut self, addr: u8) -> HalResult<()> {
        if self.slave != Some(addr) {
            self.slave = None;
            self.dev.set_slave(addr).map_err(|e| hal_error(e, addr))?;
            self.slave = Some(addr);
        }
        Ok(())
    }

    /// One `I2C_SMBUS` transfer at `addr`, no policy (callers check first).
    fn xfer(&mut self, addr: u8, rw: u8, command: u8, size: u32, data: &mut SmbusData) -> HalResult<()> {
        self.select(addr)?;
        self.dev
            .smbus(rw, command, size, data)
            .map_err(|e| hal_error(e, addr))
    }

    fn rdwr(&mut self, addr: u8, write: &[u8], read: &mut [u8]) -> HalResult<()> {
        // I2C_RDWR skips the driver check; I2C_SLAVE does it first.
        self.select(addr)?;
        self.dev.rdwr(addr, write, read).map_err(|e| hal_error(e, addr))
    }

    fn raw_read_byte(&mut self, addr: u8, reg: u8) -> HalResult<u8> {
        let mut d = SmbusData::default();
        self.xfer(addr, READ, reg, size::BYTE_DATA, &mut d)?;
        Ok(d.block[0])
    }

    /// The SPD bytes from the kernel's EEPROM driver, when one owns `addr`
    /// (`<adapter>/N-00aa/eeprom`, read-only).
    fn spd_from_kernel(&self, addr: u8, offset: u16, buf: &mut [u8]) -> Option<usize> {
        let path = self.dir.join(format!("{}-{:04x}", self.number, addr)).join("eeprom");
        let bytes = self.fs.read(&path)?;
        let at = usize::from(offset);
        let src = bytes.get(at..at + buf.len())?;
        buf.copy_from_slice(src);
        Some(buf.len())
    }

    /// EE1004 Set Page Address (Send Byte to 0x36 / 0x37), as the broker:
    /// a NACKed select of page 0 counts when Read Page Address answers page 0.
    fn spd_set_page(&mut self, page: u8) -> HalResult<()> {
        let at = SPD_SET_PAGE[usize::from(page & 1)];
        self.select(at)?;
        let mut d = SmbusData::default();
        match self.dev.smbus(WRITE, 0x00, size::BYTE, &mut d) {
            Ok(()) => Ok(()),
            Err(e) if is_nack(&e) && page & 1 == 0 && self.spd_page()? == 0 => Ok(()),
            Err(e) => Err(hal_error(e, at)),
        }
    }

    /// EE1004 Read Page Address: Receive Byte at 0x36 is ACKed on page 0 only.
    fn spd_page(&mut self) -> HalResult<u8> {
        let at = SPD_SET_PAGE[0];
        self.select(at)?;
        let mut d = SmbusData::default();
        match self.dev.smbus(READ, 0, size::BYTE, &mut d) {
            Ok(()) => Ok(0),
            Err(e) if is_nack(&e) => Ok(1),
            Err(e) => Err(hal_error(e, at)),
        }
    }
}

impl<D: I2cDev, F: Files + Send> SmbusBus for I2cSmbus<D, F> {
    fn descriptor(&self) -> &SmbusBusDescriptor {
        &self.descriptor
    }

    fn write_quick(&mut self, addr: u8) -> HalResult<()> {
        if !self.kind().supports_write_quick() {
            return Err(HalError::Unsupported("WriteQuick on this bus"));
        }
        self.need(func::SMBUS_QUICK, "WriteQuick on this adapter")?;
        self.check_write(addr)?;
        self.xfer(addr, WRITE, 0, size::QUICK, &mut SmbusData::default())
    }

    fn read_byte(&mut self, addr: u8, reg: u8) -> HalResult<u8> {
        validate_address(addr)?;
        self.need(func::SMBUS_READ_BYTE_DATA, "ReadByte on this adapter")?;
        self.raw_read_byte(addr, reg)
    }

    fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> HalResult<()> {
        self.need(func::SMBUS_WRITE_BYTE_DATA, "WriteByte on this adapter")?;
        self.check_write(addr)?;
        let mut d = SmbusData::default();
        d.block[0] = value;
        self.xfer(addr, WRITE, reg, size::BYTE_DATA, &mut d)
    }

    fn read_word(&mut self, addr: u8, reg: u8) -> HalResult<u16> {
        validate_address(addr)?;
        self.need(func::SMBUS_READ_WORD_DATA, "ReadWord on this adapter")?;
        let mut d = SmbusData::default();
        self.xfer(addr, READ, reg, size::WORD_DATA, &mut d)?;
        Ok(d.word())
    }

    fn write_word(&mut self, addr: u8, reg: u8, value: u16) -> HalResult<()> {
        self.need(func::SMBUS_WRITE_WORD_DATA, "WriteWord on this adapter")?;
        self.check_write(addr)?;
        let mut d = SmbusData::default();
        d.block[..2].copy_from_slice(&value.to_ne_bytes());
        self.xfer(addr, WRITE, reg, size::WORD_DATA, &mut d)
    }

    fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> HalResult<()> {
        self.need(func::SMBUS_WRITE_BLOCK_DATA, "WriteBlock on this adapter")?;
        self.check_write(addr)?;
        Self::check_block(data.len())?;
        let mut d = SmbusData::default();
        d.block[0] = data.len() as u8;
        d.block[1..=data.len()].copy_from_slice(data);
        self.xfer(addr, WRITE, reg, size::BLOCK_DATA, &mut d)
    }

    fn read_block(&mut self, addr: u8, reg: u8, buf: &mut [u8]) -> HalResult<usize> {
        self.no_chipset("ReadBlockBytes on a chipset bus")?;
        validate_address(addr)?;
        Self::check_block(buf.len())?;
        if self.funcs & func::I2C != 0 {
            self.rdwr(addr, &[reg], buf)?;
        } else {
            self.need(func::SMBUS_READ_I2C_BLOCK, "ReadBlockBytes on this adapter")?;
            let mut d = SmbusData::default();
            d.block[0] = buf.len() as u8;
            self.xfer(addr, READ, reg, size::I2C_BLOCK_DATA, &mut d)?;
            let n = usize::from(d.block[0]).min(buf.len());
            buf[..n].copy_from_slice(&d.block[1..=n]);
            return Ok(n);
        }
        Ok(buf.len())
    }

    fn write_block_no_reg(&mut self, addr: u8, data: &[u8]) -> HalResult<()> {
        self.no_chipset("WriteBlockWithoutRegister on a chipset bus")?;
        self.need(func::I2C, "WriteBlockWithoutRegister on this adapter")?;
        self.check_write(addr)?;
        Self::check_block(data.len())?;
        self.rdwr(addr, data, &mut [])
    }

    fn read_block_no_reg(&mut self, addr: u8, buf: &mut [u8]) -> HalResult<usize> {
        self.no_chipset("ReadBlockWithoutRegister on a chipset bus")?;
        self.need(func::I2C, "ReadBlockWithoutRegister on this adapter")?;
        validate_address(addr)?;
        Self::check_block(buf.len())?;
        self.rdwr(addr, &[], buf)?;
        Ok(buf.len())
    }

    fn read_byte_no_reg(&mut self, addr: u8) -> HalResult<u8> {
        if !self.kind().supports_read_byte_no_reg() {
            return Err(HalError::Unsupported("ReadByteWithoutRegister on this bus"));
        }
        self.need(func::SMBUS_READ_BYTE, "ReadByteWithoutRegister on this adapter")?;
        validate_address(addr)?;
        let mut d = SmbusData::default();
        self.xfer(addr, READ, 0, size::BYTE, &mut d)?;
        Ok(d.block[0])
    }

    /// From the kernel's `eeprom` file when a driver owns the SPD; otherwise
    /// page select, Read Byte per byte, page 0 again (even after a failure).
    fn spd_read(&mut self, addr: u8, offset: u16, buf: &mut [u8]) -> HalResult<usize> {
        if !self.kind().is_dimm_bus() {
            return Err(HalError::Unsupported("SPD read on this bus"));
        }
        validate_spd_read(addr, offset, buf.len())?;
        if let Some(n) = self.spd_from_kernel(addr, offset, buf) {
            return Ok(n);
        }
        self.need(
            func::SMBUS_WRITE_BYTE | func::SMBUS_READ_BYTE | func::SMBUS_READ_BYTE_DATA,
            "SPD read on this adapter",
        )?;
        let page = (offset / SPD_PAGE_LEN) as u8;
        let start = (offset % SPD_PAGE_LEN) as u8;
        tracing::info!(
            target: "lightloom::audit",
            bus = %self.descriptor.name,
            "SPD read 0x{addr:02x} page {page} offset {start} len {}",
            buf.len()
        );
        let mut result = self.spd_set_page(page);
        if result.is_ok() {
            for (i, b) in buf.iter_mut().enumerate() {
                match self.raw_read_byte(addr, start.wrapping_add(i as u8)) {
                    Ok(v) => *b = v,
                    Err(e) => {
                        result = Err(e);
                        break;
                    }
                }
            }
        }
        if page != 0 {
            if let Err(e) = self.spd_set_page(0) {
                tracing::warn!(target: "lightloom::audit", "SPD page 0 not selected again: {e}");
                result = result.and(Err(e));
            }
        }
        result.map(|()| buf.len())
    }
}

#[cfg(target_os = "linux")]
pub use linux::{open_buses, LinuxI2cDev};

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::sys;
    use crate::sysfs::RealFs;
    use std::fs::File;

    /// An opened `/dev/i2c-N`.
    #[derive(Debug)]
    pub struct LinuxI2cDev {
        file: File,
    }

    impl LinuxI2cDev {
        /// Open `devnode` read-write (nothing is sent).
        pub fn open(devnode: &str) -> io::Result<Self> {
            let file = std::fs::OpenOptions::new().read(true).write(true).open(devnode)?;
            Ok(Self { file })
        }
    }

    impl I2cDev for LinuxI2cDev {
        fn funcs(&mut self) -> io::Result<u32> {
            sys::i2c_funcs(&self.file)
        }

        fn set_slave(&mut self, addr: u8) -> io::Result<()> {
            sys::i2c_slave(&self.file, addr)
        }

        fn smbus(&mut self, read_write: u8, command: u8, size: u32, data: &mut SmbusData) -> io::Result<()> {
            sys::i2c_smbus(&self.file, read_write, command, size, data)
        }

        fn rdwr(&mut self, addr: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
            sys::i2c_rdwr(&self.file, addr, write, read)
        }
    }

    /// The SMBus buses of this machine through `/dev/i2c-*` (chipset and
    /// GPU), plus notes on what is missing. Opens the nodes and reads
    /// `I2C_FUNCS`; nothing is sent on a bus.
    pub fn open_buses() -> (Vec<I2cSmbus<LinuxI2cDev, RealFs>>, Vec<String>) {
        let class_dir = Path::new("/sys/class/i2c-dev");
        let mut notes = Vec::new();
        if !class_dir.exists() {
            notes.push(
                "no /sys/class/i2c-dev: load the i2c-dev module (modprobe i2c-dev): no RAM or GPU I2C lighting"
                    .to_string(),
            );
            return (Vec::new(), notes);
        }
        let mut buses: Vec<I2cSmbus<LinuxI2cDev, RealFs>> = Vec::new();
        for adapter in discover(&RealFs, class_dir) {
            let descriptor = match expose(&RealFs, &adapter) {
                Exposure::Bus(d) => d,
                Exposure::Skipped(why) => {
                    tracing::info!("i2c: {why}");
                    continue;
                }
                Exposure::Ignored => continue,
            };
            if buses.iter().any(|b| b.descriptor().key() == descriptor.key()) {
                tracing::info!("i2c: {} ({}): a second adapter for {}", adapter.devnode, adapter.name, descriptor.key());
                continue;
            }
            let opened = LinuxI2cDev::open(&adapter.devnode)
                .and_then(|dev| I2cSmbus::new(descriptor, &adapter, dev, RealFs));
            match opened {
                Ok(bus) => buses.push(bus),
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied => notes.push(format!(
                    "{} ({}): permission denied: install the Lightloom udev rules and join group i2c",
                    adapter.devnode, adapter.name
                )),
                Err(e) => notes.push(format!("{} ({}): {e}", adapter.devnode, adapter.name)),
            }
        }
        (buses, notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::MemFs;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[test]
    fn names_classify() {
        let piix4 = |port, base| AdapterClass::Piix4 { port, base };
        assert_eq!(classify("SMBus PIIX4 adapter port 0 at 0b00\n"), piix4(Some(0), 0x0b00));
        assert_eq!(classify("SMBus PIIX4 adapter port 2 at 0b00"), piix4(Some(2), 0x0b00));
        assert_eq!(classify("SMBus PIIX4 adapter port 1 at 0b20"), piix4(Some(1), 0x0b20));
        assert_eq!(classify("SMBus PIIX4 adapter at 0b00"), piix4(None, 0x0b00));
        assert_eq!(classify("SMBus PIIX4 adapter port x at 0b00"), AdapterClass::Other);
        assert_eq!(
            classify("SMBus I801 adapter at efa0"),
            AdapterClass::I801 {
                base: Some(0xefa0),
                idf: false
            }
        );
        assert_eq!(
            classify("SMBus I801 IDF adapter at f040"),
            AdapterClass::I801 {
                base: Some(0xf040),
                idf: true
            }
        );
        assert_eq!(
            classify("SMBus I801 adapter at 0000:00:1f.4"),
            AdapterClass::I801 { base: None, idf: false }
        );
        assert_eq!(classify("NVIDIA i2c adapter 1 at 1:00.0\n"), AdapterClass::Nvidia { port: 1 });
        assert_eq!(classify("NVIDIA i2c adapter 3 at 1:00.0"), AdapterClass::Nvidia { port: 3 });
        assert_eq!(classify("NVIDIA SOC i2c adapter 0"), AdapterClass::Other);
        assert_eq!(classify("NVIDIA GPU I2C adapter"), AdapterClass::Other);
        assert_eq!(classify("AMDGPU DM i2c OEM bus"), AdapterClass::AmdGpu("oem".into()));
        assert_eq!(classify("AMDGPU i2c bit bus OEM 0x97"), AdapterClass::AmdGpu("oem97".into()));
        assert_eq!(classify("AMDGPU DM aux hw bus 2"), AdapterClass::Other);
        assert_eq!(classify("AMDGPU DM i2c hw bus 0"), AdapterClass::Other);
        assert_eq!(classify("AMD ASF adapter"), AdapterClass::Other);
        assert_eq!(classify("Synopsys DesignWare I2C adapter"), AdapterClass::Other);
    }

    const FCH: &str = "/sys/devices/pci0000:00/0000:00:14.0";
    const GPU: &str = "/sys/devices/pci0000:00/0000:00:03.1/0000:01:00.0";

    fn pci(fs: &mut MemFs, dir: &str, ids: [u16; 4]) {
        let d = Path::new(dir);
        fs.file(d.join("vendor"), format!("0x{:04x}\n", ids[0]));
        fs.file(d.join("device"), format!("0x{:04x}\n", ids[1]));
        fs.file(d.join("subsystem_vendor"), format!("0x{:04x}\n", ids[2]));
        fs.file(d.join("subsystem_device"), format!("0x{:04x}\n", ids[3]));
        fs.file(d.join("class"), "0x0c0500\n");
    }

    fn adapter(fs: &mut MemFs, n: u32, parent: &str, name: &str) -> PathBuf {
        let dir = Path::new(parent).join(format!("i2c-{n}"));
        let class = PathBuf::from(format!("/sys/class/i2c-dev/i2c-{n}"));
        fs.links.insert(class.join("device"), dir.clone());
        fs.file(class.join("name"), format!("{name}\n"));
        fs.file(dir.join("name"), format!("{name}\n"));
        dir
    }

    /// The reference machine: B550 (PIIX4 ports 0, 2 and aux 1) and an RTX
    /// 3060 (NVIDIA ports 0-3), plus an unrelated DesignWare adapter.
    fn machine() -> MemFs {
        let mut fs = MemFs::default();
        pci(&mut fs, FCH, [0x1022, 0x790b, 0x1462, 0x7c56]);
        pci(&mut fs, GPU, [0x10de, 0x2504, 0x1462, 0x3903]);
        adapter(&mut fs, 0, FCH, "SMBus PIIX4 adapter port 0 at 0b00");
        adapter(&mut fs, 1, FCH, "SMBus PIIX4 adapter port 2 at 0b00");
        adapter(&mut fs, 2, FCH, "SMBus PIIX4 adapter port 1 at 0b20");
        for port in 0..4 {
            adapter(&mut fs, 3 + port, GPU, &format!("NVIDIA i2c adapter {port} at 1:00.0"));
        }
        adapter(&mut fs, 12, "/sys/devices/platform/AMDI0010:00", "Synopsys DesignWare I2C adapter");
        fs
    }

    #[test]
    fn discovery_on_the_reference_machine() {
        let fs = machine();
        let adapters = discover(&fs, Path::new("/sys/class/i2c-dev"));
        assert_eq!(adapters.len(), 8);
        assert_eq!(adapters[0].devnode, "/dev/i2c-0");
        assert_eq!(adapters[0].pci.as_ref().unwrap().ids.sub_vendor, 0x1462);
        let buses: Vec<SmbusBusDescriptor> = adapters
            .iter()
            .filter_map(|a| match expose(&fs, a) {
                Exposure::Bus(d) => Some(d),
                _ => None,
            })
            .collect();
        let keys: Vec<String> = buses.iter().map(|b| b.key()).collect();
        assert_eq!(
            keys,
            vec!["1022:790b:1462:7c56@0b00", "1022:790b:1462:7c56@0b20", "10de:2504:1462:3903@pci01"]
        );
        assert_eq!(buses[0].name, "AMD PIIX4 Bus - 0x0b00");
        assert_eq!(buses[0].kind, SmbusBusKind::Amd);
        assert_eq!(buses[2].kind, SmbusBusKind::Nvidia);
        assert!(matches!(expose(&fs, &adapters[1]), Exposure::Skipped(_)), "port 2");
        assert_eq!(expose(&fs, &adapters[7]), Exposure::Ignored);
    }

    #[test]
    fn i801_base_from_bar4_on_new_kernels() {
        let mut fs = MemFs::default();
        let smbus = "/sys/devices/pci0000:00/0000:00:1f.4";
        pci(&mut fs, smbus, [0x8086, 0x7a23, 0x1043, 0x8882]);
        let zero = "0x0000000000000000 0x0000000000000000 0x0000000000000000\n";
        fs.file(
            Path::new(smbus).join("resource"),
            format!(
                "0x000000604d1a8000 0x000000604d1a80ff 0x0000000000140204\n{zero}{zero}{zero}\
                 0x000000000000efa0 0x000000000000efbf 0x0000000000040101\n{zero}"
            ),
        );
        adapter(&mut fs, 5, smbus, "SMBus I801 adapter at 0000:00:1f.4");
        let adapters = discover(&fs, Path::new("/sys/class/i2c-dev"));
        let Exposure::Bus(d) = expose(&fs, &adapters[0]) else {
            panic!("i801 not exposed");
        };
        assert_eq!(d.kind, SmbusBusKind::Intel);
        assert_eq!(d.key(), "8086:7a23:1043:8882@efa0");
    }

    #[test]
    fn amdgpu_oem_bus() {
        let mut fs = MemFs::default();
        let gpu = "/sys/devices/pci0000:00/0000:00:01.1/0000:03:00.0";
        pci(&mut fs, gpu, [0x1002, 0x73bf, 0x1da2, 0xe438]);
        adapter(&mut fs, 7, gpu, "AMDGPU DM i2c OEM bus");
        adapter(&mut fs, 8, gpu, "AMDGPU DM i2c hw bus 0");
        let adapters = discover(&fs, Path::new("/sys/class/i2c-dev"));
        let Exposure::Bus(d) = expose(&fs, &adapters[0]) else {
            panic!("OEM bus not exposed");
        };
        assert_eq!((d.kind, d.key().as_str()), (SmbusBusKind::AmdGpu, "1002:73bf:1da2:e438@pci03-oem"));
        assert_eq!(expose(&fs, &adapters[1]), Exposure::Ignored, "DDC");
    }

    /// One transfer as the fake saw it.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Sent {
        Slave(u8),
        Smbus { rw: u8, cmd: u8, size: u32, data0: u8 },
        Rdwr { addr: u8, write: Vec<u8>, read: usize },
    }

    /// Devices by address with byte registers; NACK elsewhere. EE1004 at
    /// 0x50 with two pages; 0x36 / 0x37 switch them; `busy` addresses have a
    /// kernel driver.
    #[derive(Clone, Default)]
    struct FakeDev {
        funcs: u32,
        regs: BTreeMap<u8, BTreeMap<u8, u8>>,
        spd: Option<[u8; 512]>,
        page: Arc<Mutex<u8>>,
        busy: Vec<u8>,
        slave: u8,
        log: Arc<Mutex<Vec<Sent>>>,
    }

    fn nack() -> io::Error {
        io::Error::from_raw_os_error(errno::ENXIO)
    }

    impl I2cDev for FakeDev {
        fn funcs(&mut self) -> io::Result<u32> {
            Ok(self.funcs)
        }
        fn set_slave(&mut self, addr: u8) -> io::Result<()> {
            self.log.lock().unwrap().push(Sent::Slave(addr));
            if self.busy.contains(&addr) {
                return Err(io::Error::from_raw_os_error(errno::EBUSY));
            }
            self.slave = addr;
            Ok(())
        }
        fn smbus(&mut self, rw: u8, cmd: u8, size: u32, data: &mut SmbusData) -> io::Result<()> {
            self.log.lock().unwrap().push(Sent::Smbus {
                rw,
                cmd,
                size,
                data0: data.block[0],
            });
            let a = self.slave;
            if let Some(i) = SPD_SET_PAGE.iter().position(|p| *p == a) {
                if self.spd.is_none() {
                    return Err(nack());
                }
                if rw == WRITE && size == size::BYTE {
                    *self.page.lock().unwrap() = i as u8;
                    return Ok(());
                }
                // Read Page Address: ACK at 0x36 on page 0.
                return if *self.page.lock().unwrap() == 0 && i == 0 { Ok(()) } else { Err(nack()) };
            }
            if a == 0x50 {
                if let Some(spd) = &self.spd {
                    let page = usize::from(*self.page.lock().unwrap());
                    data.block[0] = spd[page * 256 + usize::from(cmd)];
                    return Ok(());
                }
            }
            let Some(regs) = self.regs.get_mut(&a) else {
                return Err(nack());
            };
            match (rw, size) {
                (READ, size::BYTE_DATA) => data.block[0] = regs.get(&cmd).copied().unwrap_or(0xFF),
                (READ, size::WORD_DATA) => {
                    data.block[0] = regs.get(&cmd).copied().unwrap_or(0xFF);
                    data.block[1] = regs.get(&cmd.wrapping_add(1)).copied().unwrap_or(0xFF);
                }
                (WRITE, size::BYTE_DATA) => {
                    regs.insert(cmd, data.block[0]);
                }
                _ => {}
            }
            Ok(())
        }
        fn rdwr(&mut self, addr: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
            self.log.lock().unwrap().push(Sent::Rdwr {
                addr,
                write: write.to_vec(),
                read: read.len(),
            });
            read.fill(0xAB);
            Ok(())
        }
    }

    const PIIX4_FUNCS: u32 = func::SMBUS_QUICK
        | func::SMBUS_READ_BYTE
        | func::SMBUS_WRITE_BYTE
        | func::SMBUS_READ_BYTE_DATA
        | func::SMBUS_WRITE_BYTE_DATA
        | func::SMBUS_READ_WORD_DATA
        | func::SMBUS_WRITE_WORD_DATA
        | 0x0100_0000
        | func::SMBUS_WRITE_BLOCK_DATA;

    fn bus(kind: SmbusBusKind, dev: FakeDev, fs: MemFs) -> I2cSmbus<FakeDev, MemFs> {
        let a = I2cAdapter {
            number: 0,
            devnode: "/dev/i2c-0".into(),
            dir: PathBuf::from(FCH).join("i2c-0"),
            name: "SMBus PIIX4 adapter port 0 at 0b00".into(),
            class: classify("SMBus PIIX4 adapter port 0 at 0b00"),
            pci: None,
        };
        let d = SmbusBusDescriptor {
            name: "AMD PIIX4 Bus - 0x0b00".into(),
            kind,
            pci: PciIds::default(),
            instance: "0b00".into(),
            max_block_len: SMBUS_BLOCK_MAX,
        };
        I2cSmbus::new(d, &a, dev, fs).unwrap()
    }

    fn piix4() -> (I2cSmbus<FakeDev, MemFs>, Arc<Mutex<Vec<Sent>>>) {
        let mut dev = FakeDev {
            funcs: PIIX4_FUNCS,
            ..FakeDev::default()
        };
        dev.regs.insert(0x39, BTreeMap::from([(0x10, 0x42)]));
        dev.regs.insert(0x77, BTreeMap::new());
        let log = dev.log.clone();
        (bus(SmbusBusKind::Amd, dev, MemFs::default()), log)
    }

    #[test]
    fn chipset_policy_mirrors_the_broker() {
        let (mut b, log) = piix4();
        assert_eq!(b.read_byte(0x39, 0x10).unwrap(), 0x42);
        b.write_byte(0x39, 0x11, 7).unwrap();
        assert_eq!(b.read_word(0x39, 0x10).unwrap(), 0x0742);
        assert!(matches!(b.write_quick(0x39), Ok(())));
        // SPD, write-protect / page select, PMIC: denied before anything is sent.
        let before = log.lock().unwrap().len();
        for addr in [0x50, 0x57, 0x36, 0x37, 0x30, 0x48, 0x5F] {
            assert!(matches!(b.write_byte(addr, 0, 0), Err(HalError::PermissionDenied(_))), "0x{addr:02x}");
            assert!(matches!(b.write_quick(addr), Err(HalError::PermissionDenied(_))), "0x{addr:02x}");
        }
        // Outside the broker allowlist (a clock generator, a VRM part).
        assert!(matches!(b.write_byte(0x69, 0, 0), Err(HalError::PermissionDenied(_))));
        assert!(matches!(b.write_block(0x77, 0, &[1]), Err(HalError::PermissionDenied(_))));
        assert_eq!(log.lock().unwrap().len(), before, "nothing sent");
        // Reads elsewhere stay allowed (the plugins' probes).
        assert_eq!(b.read_byte(0x77, 0).unwrap(), 0xFF);
        // No block reads or register-less transfers on chipset buses.
        assert!(matches!(b.read_block(0x39, 0, &mut [0; 4]), Err(HalError::Unsupported(_))));
        assert!(matches!(b.write_block_no_reg(0x39, &[1]), Err(HalError::Unsupported(_))));
        assert!(matches!(b.read_block_no_reg(0x39, &mut [0; 1]), Err(HalError::Unsupported(_))));
        // Blocks 1..=32.
        assert!(b.write_block(0x39, 3, &[0; 32]).is_ok());
        assert!(matches!(b.write_block(0x39, 3, &[0; 33]), Err(HalError::InvalidArgument(_))));
        assert!(matches!(b.write_block(0x39, 3, &[]), Err(HalError::InvalidArgument(_))));
        // Address range.
        assert!(matches!(b.read_byte(0x78, 0), Err(HalError::InvalidArgument(_))));
        // A NACK is a bus failure (-1 to plugins).
        let e = b.read_byte(0x2A, 0).unwrap_err();
        assert_eq!(e.plugin_status(), -1);
    }

    #[test]
    fn block_write_is_count_prefixed_and_slave_is_cached() {
        let (mut b, log) = piix4();
        b.write_block(0x39, 0x03, &[1, 2, 3]).unwrap();
        b.write_byte(0x39, 0x01, 9).unwrap();
        let log = log.lock().unwrap();
        assert_eq!(
            *log,
            vec![
                Sent::Slave(0x39),
                Sent::Smbus {
                    rw: WRITE,
                    cmd: 0x03,
                    size: size::BLOCK_DATA,
                    data0: 3
                },
                Sent::Smbus {
                    rw: WRITE,
                    cmd: 0x01,
                    size: size::BYTE_DATA,
                    data0: 9
                },
            ]
        );
    }

    #[test]
    fn functionality_mask_is_checked() {
        let dev = FakeDev {
            funcs: func::SMBUS_READ_BYTE_DATA,
            ..FakeDev::default()
        };
        let mut b = bus(SmbusBusKind::Amd, dev, MemFs::default());
        assert!(matches!(b.write_quick(0x39), Err(HalError::Unsupported(_))));
        assert!(matches!(b.write_block(0x39, 0, &[1]), Err(HalError::Unsupported(_))));
        assert!(matches!(b.read_byte_no_reg(0x39), Err(HalError::Unsupported(_))));
    }

    #[test]
    fn kernel_driver_owned_address_is_busy() {
        let dev = FakeDev {
            funcs: PIIX4_FUNCS,
            busy: vec![0x39],
            ..FakeDev::default()
        };
        let mut b = bus(SmbusBusKind::Amd, dev, MemFs::default());
        assert!(matches!(b.read_byte(0x39, 0), Err(HalError::Busy(_))));
        // The failed select is not cached.
        assert!(matches!(b.read_byte(0x39, 0), Err(HalError::Busy(_))));
    }

    #[test]
    fn gpu_bus_does_plain_i2c() {
        let dev = FakeDev {
            funcs: func::I2C | PIIX4_FUNCS,
            ..FakeDev::default()
        };
        let log = dev.log.clone();
        let mut b = bus(SmbusBusKind::Nvidia, dev, MemFs::default());
        assert!(matches!(b.write_quick(0x68), Err(HalError::Unsupported(_))), "NvAPI parity");
        let mut buf = [0u8; 4];
        assert_eq!(b.read_block(0x68, 0x20, &mut buf).unwrap(), 4);
        assert_eq!(buf, [0xAB; 4]);
        b.write_block_no_reg(0x68, &[1, 2]).unwrap();
        assert!(matches!(b.write_block_no_reg(0x50, &[1]), Err(HalError::PermissionDenied(_))));
        // GPU buses keep 0x36 (ASRock GPU quick-writes it on AMD cards).
        assert!(!matches!(b.write_byte(0x36, 0, 0), Err(HalError::PermissionDenied(_))));
        assert!(matches!(b.spd_read(0x50, 0, &mut [0; 1]), Err(HalError::Unsupported(_))));
        let log = log.lock().unwrap();
        assert!(log.contains(&Sent::Rdwr {
            addr: 0x68,
            write: vec![0x20],
            read: 4
        }));
        assert!(log.contains(&Sent::Rdwr {
            addr: 0x68,
            write: vec![1, 2],
            read: 0
        }));
    }

    fn spd() -> [u8; 512] {
        let mut s = [0u8; 512];
        s[2] = 0x0C;
        s[320] = 0x85;
        s[321] = 0x9B;
        s
    }

    #[test]
    fn spd_read_selects_the_page_and_puts_page_0_back() {
        let dev = FakeDev {
            funcs: PIIX4_FUNCS,
            spd: Some(spd()),
            ..FakeDev::default()
        };
        let (log, page) = (dev.log.clone(), dev.page.clone());
        let mut b = bus(SmbusBusKind::Amd, dev, MemFs::default());
        let mut id = [0u8; 2];
        assert_eq!(b.spd_read(0x50, 320, &mut id).unwrap(), 2);
        assert_eq!(id, [0x85, 0x9B]);
        assert_eq!(*page.lock().unwrap(), 0, "page 0 again");
        let mut ty = [0u8; 1];
        b.spd_read(0x50, 2, &mut ty).unwrap();
        assert_eq!(ty, [0x0C]);
        let slaves: Vec<u8> = log
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| match s {
                Sent::Slave(a) => Some(*a),
                _ => None,
            })
            .collect();
        assert_eq!(slaves, vec![0x37, 0x50, 0x36, 0x50]);
        // Plugins still cannot select a page.
        assert!(matches!(b.write_byte(0x37, 0, 0), Err(HalError::PermissionDenied(_))));
        assert!(matches!(b.spd_read(0x58, 0, &mut ty), Err(HalError::InvalidArgument(_))));
    }

    #[test]
    fn spd_read_uses_the_kernel_eeprom_when_a_driver_owns_it() {
        let mut fs = MemFs::default();
        fs.file(PathBuf::from(FCH).join("i2c-0/0-0050/eeprom"), spd().to_vec());
        let dev = FakeDev {
            funcs: PIIX4_FUNCS,
            busy: vec![0x36, 0x37, 0x50],
            ..FakeDev::default()
        };
        let log = dev.log.clone();
        let mut b = bus(SmbusBusKind::Amd, dev, fs);
        let mut id = [0u8; 2];
        assert_eq!(b.spd_read(0x50, 320, &mut id).unwrap(), 2);
        assert_eq!(id, [0x85, 0x9B]);
        assert!(log.lock().unwrap().is_empty(), "nothing sent");
        // No driver at 0x51 and nothing there: the select fails, cleanly.
        assert!(b.spd_read(0x51, 0, &mut id).is_err());
    }
}
