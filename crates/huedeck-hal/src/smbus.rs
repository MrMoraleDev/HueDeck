// SPDX-License-Identifier: GPL-3.0-or-later
//! SMBus / I2C bus trait.
//!
//! One [`SmbusBus`] per host controller: chipset SMBus (AMD FCH "PIIX4",
//! Intel i801/"ICH8"), SuperIO-embedded masters (Nuvoton), and per-GPU buses
//! (NvAPI, AMD ADL). On Windows the chipset buses are reached through the
//! privileged `huedeck-iobroker`; on Linux through `/dev/i2c-*`.
//!
//! Return convention at this layer is a normal `Result`; the plugin-facing
//! wrapper converts errors to the SignalRGB negative status codes.

use crate::error::{HalError, HalResult};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// Lowest valid 7-bit slave address.
pub const SMBUS_ADDR_MIN: u8 = 0x03;
/// Highest valid 7-bit slave address.
pub const SMBUS_ADDR_MAX: u8 = 0x77;
/// SMBus block transfers carry at most 32 data bytes.
pub const SMBUS_BLOCK_MAX: usize = 32;
/// DDR4/DDR5 SPD EEPROM addresses. Never written, on any bus.
pub const SPD_EEPROM: RangeInclusive<u8> = 0x50..=0x57;
/// DDR4 SPD write-protect / page-select addresses. Never written on a DIMM bus.
pub const SPD_WRITE_PROTECT: RangeInclusive<u8> = 0x30..=0x37;
/// DDR5 PMIC addresses. Never written on a DIMM bus.
pub const DDR5_PMIC: RangeInclusive<u8> = 0x48..=0x4F;
/// The EEPROM area 0x50-0x5F: the SPD, then the DDR5 RCD (0x58-0x5F) and
/// 24RF08-class EEPROMs, which a Quick write can corrupt (Linux never sends
/// one there). Never written on a DIMM bus.
pub const EEPROM_AREA: RangeInclusive<u8> = 0x50..=0x5F;
/// A DDR4 SPD EEPROM (JEDEC EE1004): 512 bytes in two 256-byte pages.
pub const SPD_DDR4_LEN: u16 = 512;
/// EE1004 page size.
pub const SPD_PAGE_LEN: u16 = 256;
/// EE1004 "Set Page Address" commands: SPA0 (page 0) at 0x36, SPA1 (page 1)
/// at 0x37. A Send Byte (data 0x00, ignored by the EEPROM) to one of them
/// switches the page of every DDR4 SPD on the bus, as Linux's `ee1004`
/// driver does. The only writes HueDeck ever sends into 0x30-0x37, and only
/// from [`SmbusBus::spd_read`] inside a backend.
pub const SPD_SET_PAGE: [u8; 2] = [0x36, 0x37];

/// Check the arguments of [`SmbusBus::spd_read`]: an SPD address
/// (0x50..=0x57), 1..=32 bytes within one page of the 512-byte DDR4 SPD.
pub fn validate_spd_read(addr: u8, offset: u16, len: usize) -> HalResult<()> {
    if !SPD_EEPROM.contains(&addr) {
        return Err(HalError::InvalidArgument(format!(
            "SPD read at 0x{addr:02x}: not an SPD address (0x50..=0x57)"
        )));
    }
    let in_page = usize::from(offset % SPD_PAGE_LEN);
    if offset >= SPD_DDR4_LEN
        || len == 0
        || len > SMBUS_BLOCK_MAX
        || in_page + len > usize::from(SPD_PAGE_LEN)
    {
        return Err(HalError::InvalidArgument(format!(
            "SPD read of {len} bytes at {offset}: 1..=32 bytes within one 256-byte page of 512"
        )));
    }
    Ok(())
}

/// Which family of host controller owns a bus. Drives the plugin-visible
/// `IsSystemBus/IsAMDBus/IsIntelBus/IsNvidiaBus/IsNuvotonBus/IsAMDGpuBus` predicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SmbusBusKind {
    /// Chipset SMBus of unknown vendor (e.g. Linux `/dev/i2c-N` without PCI info).
    System,
    /// Per-GPU NVIDIA I2C bus reached through NvAPI.
    Nvidia,
    /// Per-GPU AMD I2C bus reached through ADL (`huedeck-adl`).
    AmdGpu,
    /// Nuvoton SuperIO-embedded SMBus master (LPC 0x2E/0x4E).
    Nuvoton,
    /// Intel i801 / ICH8-family chipset SMBus.
    Intel,
    /// AMD FCH / PIIX4-compatible chipset SMBus.
    Amd,
}

impl SmbusBusKind {
    /// `bus.IsSystemBus()`: chipset buses (AMD or Intel or unknown chipset).
    pub fn is_system_bus(self) -> bool {
        matches!(
            self,
            SmbusBusKind::System | SmbusBusKind::Amd | SmbusBusKind::Intel
        )
    }
    /// `bus.IsAMDBus()`.
    pub fn is_amd_bus(self) -> bool {
        self == SmbusBusKind::Amd
    }
    /// `bus.IsIntelBus()`.
    pub fn is_intel_bus(self) -> bool {
        self == SmbusBusKind::Intel
    }
    /// `bus.IsNvidiaBus()`.
    pub fn is_nvidia_bus(self) -> bool {
        self == SmbusBusKind::Nvidia
    }
    /// `bus.IsNuvotonBus()`.
    pub fn is_nuvoton_bus(self) -> bool {
        self == SmbusBusKind::Nuvoton
    }
    /// `bus.IsAMDGpuBus()`.
    pub fn is_amd_gpu_bus(self) -> bool {
        self == SmbusBusKind::AmdGpu
    }
    /// Chipset SMBus: System, Amd, Intel.
    pub fn is_chipset(self) -> bool {
        self.is_system_bus()
    }
    /// A bus that may reach the DIMMs (SPD, write-protect, PMIC, RCD): the
    /// chipset buses and Nuvoton SuperIO masters (wired to the DIMMs on
    /// some boards).
    pub fn is_dimm_bus(self) -> bool {
        self.is_chipset() || self == SmbusBusKind::Nuvoton
    }
    /// NvAPI buses have no Quick command.
    pub fn supports_write_quick(self) -> bool {
        self != SmbusBusKind::Nvidia
    }
    /// SMBus Receive Byte: NvAPI, and the AMD PIIX4 / Intel i801 chipset
    /// controllers (protocol BYTE). SignalRGB refuses it on chipset buses by
    /// policy; HueDeck allows it because OpenRGB's RAM detection probes with
    /// it (docs/host-api/smbus.md, HueDeck section).
    pub fn supports_read_byte_no_reg(self) -> bool {
        matches!(
            self,
            SmbusBusKind::Nvidia | SmbusBusKind::Amd | SmbusBusKind::Intel
        )
    }
}

/// Whether a write (or Quick command) to `addr` is refused on a bus of
/// `kind`: the SPD EEPROM addresses on every bus; on DIMM buses
/// ([`SmbusBusKind::is_dimm_bus`]) also the SPD write-protect addresses, the
/// DDR5 PMIC and the whole EEPROM area 0x50-0x5F. GPU buses keep the other
/// addresses (the ASRock GPU plugin quick-writes 0x36 on its ADLX bus).
/// Mirrored by `huedeck_protocol::broker::BrokerOp::touches_spd`.
pub fn write_denied(kind: SmbusBusKind, addr: u8) -> bool {
    SPD_EEPROM.contains(&addr)
        || (kind.is_dimm_bus()
            && (SPD_WRITE_PROTECT.contains(&addr)
                || DDR5_PMIC.contains(&addr)
                || EEPROM_AREA.contains(&addr)))
}

/// PCI identifiers of the controller that owns a bus (`bus.Vendor()/Product()/SubVendor()/SubDevice()`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PciIds {
    /// PCI vendor id (0x1022 AMD FCH, 0x8086 Intel, 0x10DE NVIDIA).
    pub vendor: u16,
    /// PCI device id (0x790B AMD FCH SMBus, GPU device id for GPU buses).
    pub device: u16,
    /// PCI subsystem vendor (board maker for GPUs).
    pub sub_vendor: u16,
    /// PCI subsystem device (board id for GPUs).
    pub sub_device: u16,
}

/// Static description of a bus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmbusBusDescriptor {
    /// Human-readable name (`"AMD PIIX4 Bus - 0x0b00"`, `"NVIDIA GeForce RTX 3060"`).
    pub name: String,
    /// Controller family.
    pub kind: SmbusBusKind,
    /// PCI ids of the controller.
    pub pci: PciIds,
    /// Which bus of that controller: the PIIX4 port base (`"0b00"`, `"0b20"`)
    /// or the GPU's PCI bus (`"pci01"`). Two PIIX4 buses share one PCI device,
    /// so the PCI ids alone do not tell buses apart.
    #[serde(default)]
    pub instance: String,
    /// Largest block transfer the backend accepts (data bytes).
    pub max_block_len: usize,
}

impl SmbusBusDescriptor {
    /// Stable key of the bus: `vendor:device:subvendor:subdevice@instance`
    /// in lower-case hex, e.g. `10de:2504:1462:3903@pci01`.
    pub fn key(&self) -> String {
        let p = self.pci;
        format!(
            "{:04x}:{:04x}:{:04x}:{:04x}@{}",
            p.vendor, p.device, p.sub_vendor, p.sub_device, self.instance
        )
    }
}

/// Validate a 7-bit address against the SMBus reserved ranges.
pub fn validate_address(addr: u8) -> HalResult<()> {
    if (SMBUS_ADDR_MIN..=SMBUS_ADDR_MAX).contains(&addr) {
        Ok(())
    } else {
        Err(HalError::InvalidArgument(format!(
            "SMBus address 0x{addr:02x} outside 0x03..=0x77"
        )))
    }
}

/// One SMBus/I2C host controller.
///
/// Every method takes the 7-bit slave `addr`; the fixed-address wrapper the
/// plugin sees simply binds it. Implementations serialise access themselves
/// (the Windows broker holds `Global\Access_SMBUS.HTP.Method` per operation
/// or batch; NvAPI buses go through one worker thread per process).
pub trait SmbusBus: Send {
    /// Static description.
    fn descriptor(&self) -> &SmbusBusDescriptor;

    /// SMBus Quick command (address probe). `Ok(())` when the slave ACKed.
    fn write_quick(&mut self, addr: u8) -> HalResult<()>;
    /// SMBus Read Byte (command `reg`).
    fn read_byte(&mut self, addr: u8, reg: u8) -> HalResult<u8>;
    /// SMBus Write Byte (command `reg`).
    fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> HalResult<()>;
    /// SMBus Read Word (LSB first on the wire).
    fn read_word(&mut self, addr: u8, reg: u8) -> HalResult<u16>;
    /// SMBus Write Word (LSB first on the wire).
    fn write_word(&mut self, addr: u8, reg: u8, value: u16) -> HalResult<()>;
    /// SMBus Block Write with command byte `reg`: on the wire `reg`, a count
    /// byte (`data.len()`), then `data` (PIIX4 protocol 5; on NvAPI the
    /// payload `[len, data…]` after the register, as OpenRGB sends it).
    fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> HalResult<()>;
    /// Block read with command byte `reg`, exactly `buf.len()` bytes expected
    /// (I2C combined write-reg/read-N, *not* count-prefixed). Returns bytes read.
    fn read_block(&mut self, addr: u8, reg: u8, buf: &mut [u8]) -> HalResult<usize>;
    /// Raw I2C write with no command byte.
    fn write_block_no_reg(&mut self, addr: u8, data: &[u8]) -> HalResult<()>;
    /// Raw I2C read of `buf.len()` bytes with no command byte. Returns bytes read.
    fn read_block_no_reg(&mut self, addr: u8, buf: &mut [u8]) -> HalResult<usize>;
    /// SMBus Receive Byte (no command byte); see
    /// [`SmbusBusKind::supports_read_byte_no_reg`].
    fn read_byte_no_reg(&mut self, addr: u8) -> HalResult<u8>;

    /// Keep the bus for this client across the next operations (a plugin's
    /// `TransactionLock`), for at most `lease_ms`. Buses owned by this
    /// process need nothing beyond their own mutex, hence the default; the
    /// I/O broker client holds the global SMBus mutex in the broker
    /// (`BusLock`).
    fn bus_lock(&mut self, _lease_ms: u16) -> HalResult<()> {
        Ok(())
    }
    /// End [`SmbusBus::bus_lock`].
    fn bus_unlock(&mut self) -> HalResult<()> {
        Ok(())
    }

    /// Read `buf.len()` bytes (1..=32, within one page) of the DDR4 SPD
    /// EEPROM (EE1004) at `addr` (0x50..=0x57) from byte `offset` (0..512),
    /// for the host's SPD check of RAM plugins ([`validate_spd_read`]). The
    /// backend selects the page ([`SPD_SET_PAGE`]), reads, and selects page 0
    /// again, all under its bus mutex; it is the only write any backend sends
    /// to 0x30-0x37, and plugins have no way to it (their `bus` object has no
    /// such call). Chipset buses only; the default refuses (GPU buses).
    fn spd_read(&mut self, _addr: u8, _offset: u16, _buf: &mut [u8]) -> HalResult<usize> {
        Err(HalError::Unsupported("SPD read on this bus"))
    }

    /// Run `ops` in order and stop at the first failure ([`SmbusBatch`]).
    /// The default sends them one by one; the I/O broker client sends them
    /// as one request (one pipe round trip, one hold of the global SMBus
    /// mutex). Every op gets the same checks as when sent alone.
    fn run_batch(&mut self, ops: &[SmbusOp]) -> SmbusBatch {
        run_one_by_one(self, ops)
    }
}

/// [`SmbusBus::run_batch`] op by op: stops at the first failure.
pub fn run_one_by_one<B: SmbusBus + ?Sized>(bus: &mut B, ops: &[SmbusOp]) -> SmbusBatch {
    let mut done = SmbusBatch::default();
    for op in ops {
        match op.run_on(bus) {
            Ok(reply) => done.replies.push(reply),
            Err(e) => {
                done.error = Some(e);
                break;
            }
        }
    }
    done
}

/// One transfer of a [`SmbusBus::run_batch`]: the operations a plugin host
/// queues (writes) plus the reads that end a batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmbusOp {
    /// [`SmbusBus::write_quick`].
    WriteQuick {
        /// 7-bit address.
        addr: u8,
    },
    /// [`SmbusBus::write_byte`].
    WriteByte {
        /// 7-bit address.
        addr: u8,
        /// Command byte.
        reg: u8,
        /// Value.
        value: u8,
    },
    /// [`SmbusBus::write_word`].
    WriteWord {
        /// 7-bit address.
        addr: u8,
        /// Command byte.
        reg: u8,
        /// Value.
        value: u16,
    },
    /// [`SmbusBus::write_block`].
    WriteBlock {
        /// 7-bit address.
        addr: u8,
        /// Command byte.
        reg: u8,
        /// Data.
        data: Vec<u8>,
    },
    /// [`SmbusBus::write_block_no_reg`].
    WriteBlockNoReg {
        /// 7-bit address.
        addr: u8,
        /// Data.
        data: Vec<u8>,
    },
    /// [`SmbusBus::read_byte`].
    ReadByte {
        /// 7-bit address.
        addr: u8,
        /// Command byte.
        reg: u8,
    },
    /// [`SmbusBus::read_word`].
    ReadWord {
        /// 7-bit address.
        addr: u8,
        /// Command byte.
        reg: u8,
    },
    /// [`SmbusBus::read_byte_no_reg`].
    ReadByteNoReg {
        /// 7-bit address.
        addr: u8,
    },
}

impl SmbusOp {
    /// Whether the op changes device state (a Quick command counts).
    pub fn is_write(&self) -> bool {
        matches!(
            self,
            SmbusOp::WriteQuick { .. }
                | SmbusOp::WriteByte { .. }
                | SmbusOp::WriteWord { .. }
                | SmbusOp::WriteBlock { .. }
                | SmbusOp::WriteBlockNoReg { .. }
        )
    }

    /// Run the op alone on `bus`.
    pub fn run_on<B: SmbusBus + ?Sized>(&self, bus: &mut B) -> HalResult<SmbusReply> {
        match self {
            SmbusOp::WriteQuick { addr } => bus.write_quick(*addr).map(|()| SmbusReply::Done),
            SmbusOp::WriteByte { addr, reg, value } => {
                bus.write_byte(*addr, *reg, *value).map(|()| SmbusReply::Done)
            }
            SmbusOp::WriteWord { addr, reg, value } => {
                bus.write_word(*addr, *reg, *value).map(|()| SmbusReply::Done)
            }
            SmbusOp::WriteBlock { addr, reg, data } => {
                bus.write_block(*addr, *reg, data).map(|()| SmbusReply::Done)
            }
            SmbusOp::WriteBlockNoReg { addr, data } => {
                bus.write_block_no_reg(*addr, data).map(|()| SmbusReply::Done)
            }
            SmbusOp::ReadByte { addr, reg } => bus.read_byte(*addr, *reg).map(SmbusReply::Byte),
            SmbusOp::ReadWord { addr, reg } => bus.read_word(*addr, *reg).map(SmbusReply::Word),
            SmbusOp::ReadByteNoReg { addr } => bus.read_byte_no_reg(*addr).map(SmbusReply::Byte),
        }
    }
}

/// What one [`SmbusOp`] answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmbusReply {
    /// A write completed.
    Done,
    /// A byte read.
    Byte(u8),
    /// A word read.
    Word(u16),
}

/// The outcome of [`SmbusBus::run_batch`]: one reply per op done, in order;
/// when it stopped early, the error of op `replies.len()` (the ops after it
/// were not sent).
#[derive(Debug, Default)]
pub struct SmbusBatch {
    /// Replies of the ops done.
    pub replies: Vec<SmbusReply>,
    /// Why op `replies.len()` failed, if one did.
    pub error: Option<HalError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicates() {
        assert!(SmbusBusKind::Amd.is_system_bus());
        assert!(SmbusBusKind::Intel.is_system_bus());
        assert!(!SmbusBusKind::Nvidia.is_system_bus());
        assert!(!SmbusBusKind::Nvidia.supports_write_quick());
        assert!(SmbusBusKind::Nvidia.supports_read_byte_no_reg());
        assert!(SmbusBusKind::Amd.supports_read_byte_no_reg());
        assert!(SmbusBusKind::Intel.supports_read_byte_no_reg());
        assert!(!SmbusBusKind::Nuvoton.supports_read_byte_no_reg());
        assert!(!SmbusBusKind::System.supports_read_byte_no_reg());
        assert!(SmbusBusKind::System.is_chipset() && SmbusBusKind::Amd.is_chipset());
        assert!(!SmbusBusKind::Nvidia.is_chipset() && !SmbusBusKind::AmdGpu.is_chipset());
    }

    #[test]
    fn descriptor_key_distinguishes_piix4_ports() {
        let piix4 = |instance: &str| SmbusBusDescriptor {
            name: format!("AMD PIIX4 Bus - 0x{instance}"),
            kind: SmbusBusKind::Amd,
            pci: PciIds {
                vendor: 0x1022,
                device: 0x790B,
                sub_vendor: 0x1022,
                sub_device: 0x790B,
            },
            instance: instance.into(),
            max_block_len: SMBUS_BLOCK_MAX,
        };
        assert_eq!(piix4("0b00").key(), "1022:790b:1022:790b@0b00");
        assert_ne!(piix4("0b00").key(), piix4("0b20").key());
    }

    #[test]
    fn dimm_addresses_denied_on_dimm_buses() {
        for kind in [
            SmbusBusKind::System,
            SmbusBusKind::Amd,
            SmbusBusKind::Intel,
            SmbusBusKind::Nuvoton,
        ] {
            assert!(kind.is_dimm_bus());
            for addr in [0x30, 0x37, 0x48, 0x4F, 0x50, 0x57, 0x58, 0x5F] {
                assert!(write_denied(kind, addr), "{kind:?} 0x{addr:02x}");
            }
            for addr in [0x20, 0x27, 0x2F, 0x38, 0x39, 0x3C, 0x47, 0x60, 0x68] {
                assert!(!write_denied(kind, addr), "{kind:?} 0x{addr:02x}");
            }
        }
    }

    #[test]
    fn spd_denied_on_every_bus_the_rest_only_on_dimm_buses() {
        for kind in [SmbusBusKind::Nvidia, SmbusBusKind::AmdGpu] {
            assert!(!kind.is_dimm_bus());
            assert!(write_denied(kind, 0x50) && write_denied(kind, 0x57));
            for addr in [0x36, 0x48, 0x58, 0x68] {
                assert!(!write_denied(kind, addr), "{kind:?} 0x{addr:02x}");
            }
        }
    }

    #[test]
    fn spd_read_arguments() {
        assert!(validate_spd_read(0x50, 2, 1).is_ok());
        assert!(validate_spd_read(0x57, 320, 2).is_ok());
        assert!(validate_spd_read(0x50, 0x1E0, 32).is_ok());
        assert!(validate_spd_read(0x4F, 0, 1).is_err(), "not an SPD address");
        assert!(validate_spd_read(0x58, 0, 1).is_err());
        assert!(validate_spd_read(0x36, 0, 1).is_err());
        assert!(validate_spd_read(0x50, 0, 0).is_err());
        assert!(validate_spd_read(0x50, 0, 33).is_err());
        assert!(validate_spd_read(0x50, 250, 8).is_err(), "crosses into page 1");
        assert!(validate_spd_read(0x50, 512, 1).is_err());
    }

    #[test]
    fn address_range() {
        assert!(validate_address(0x02).is_err());
        assert!(validate_address(0x03).is_ok());
        assert!(validate_address(0x77).is_ok());
        assert!(validate_address(0x78).is_err());
    }
}
