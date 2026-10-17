// SPDX-License-Identifier: Apache-2.0 OR MIT
//! SMBus / I2C bus trait.
//!
//! One [`SmbusBus`] per host controller: chipset SMBus (AMD FCH "PIIX4",
//! Intel i801/"ICH8"), SuperIO-embedded masters (Nuvoton), and per-GPU buses
//! (NvAPI, AMD ADLX). On Windows the chipset buses are reached through the
//! privileged `lightloom-iobroker`; on Linux through `/dev/i2c-*`.
//!
//! Return convention at this layer is a normal `Result`; the plugin-facing
//! wrapper converts errors to the SignalRGB negative status codes.

use crate::error::{HalError, HalResult};
use serde::{Deserialize, Serialize};

/// Lowest valid 7-bit slave address.
pub const SMBUS_ADDR_MIN: u8 = 0x03;
/// Highest valid 7-bit slave address.
pub const SMBUS_ADDR_MAX: u8 = 0x77;
/// SMBus block transfers carry at most 32 data bytes.
pub const SMBUS_BLOCK_MAX: usize = 32;

/// Which family of host controller owns a bus. Drives the plugin-visible
/// `IsSystemBus/IsAMDBus/IsIntelBus/IsNvidiaBus/IsNuvotonBus/IsAMDGpuBus` predicates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SmbusBusKind {
    /// Chipset SMBus of unknown vendor (e.g. Linux `/dev/i2c-N` without PCI info).
    System,
    /// Per-GPU NVIDIA I2C bus reached through NvAPI.
    Nvidia,
    /// Per-GPU AMD I2C bus reached through ADLX.
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
        matches!(self, SmbusBusKind::System | SmbusBusKind::Amd | SmbusBusKind::Intel)
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
    /// NvAPI buses have no Quick command.
    pub fn supports_write_quick(self) -> bool {
        self != SmbusBusKind::Nvidia
    }
    /// Register-less byte receive exists only on NvAPI buses.
    pub fn supports_read_byte_no_reg(self) -> bool {
        self == SmbusBusKind::Nvidia
    }
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
    /// Largest block transfer the backend accepts (data bytes).
    pub max_block_len: usize,
}

/// Validate a 7-bit address against the SMBus reserved ranges.
pub fn validate_address(addr: u8) -> HalResult<()> {
    if (SMBUS_ADDR_MIN..=SMBUS_ADDR_MAX).contains(&addr) {
        Ok(())
    } else {
        Err(HalError::InvalidArgument(format!("SMBus address 0x{addr:02x} outside 0x03..=0x77")))
    }
}

/// One SMBus/I2C host controller.
///
/// Every method takes the 7-bit slave `addr`; the fixed-address wrapper the
/// plugin sees simply binds it. Implementations serialise access themselves
/// (the Windows broker holds `Global\Access_SMBUS.HTP.Method` per operation).
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
    /// Block write with command byte `reg` (I2C combined write: reg + data).
    fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> HalResult<()>;
    /// Block read with command byte `reg`, exactly `buf.len()` bytes expected
    /// (I2C combined write-reg/read-N, *not* count-prefixed). Returns bytes read.
    fn read_block(&mut self, addr: u8, reg: u8, buf: &mut [u8]) -> HalResult<usize>;
    /// Raw I2C write with no command byte.
    fn write_block_no_reg(&mut self, addr: u8, data: &[u8]) -> HalResult<()>;
    /// Raw I2C read of `buf.len()` bytes with no command byte. Returns bytes read.
    fn read_block_no_reg(&mut self, addr: u8, buf: &mut [u8]) -> HalResult<usize>;
    /// SMBus Receive Byte (no command byte). NvAPI buses only.
    fn read_byte_no_reg(&mut self, addr: u8) -> HalResult<u8>;
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
        assert!(!SmbusBusKind::Nuvoton.supports_read_byte_no_reg());
    }

    #[test]
    fn address_range() {
        assert!(validate_address(0x02).is_err());
        assert!(validate_address(0x03).is_ok());
        assert!(validate_address(0x77).is_ok());
        assert!(validate_address(0x78).is_err());
    }
}
