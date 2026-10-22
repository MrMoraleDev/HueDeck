// SPDX-License-Identifier: GPL-3.0-or-later
//! `NV_I2C_INFO_V3` and the SMBus → NvAPI transfer mapping. Pure: no NvAPI call.
//!
//! The structure layout is NVIDIA's (`nvapi.h`, MIT, x64 offsets recomputed
//! in research m5-privileged-buses.md §4). The mapping of each SMBus
//! operation onto one `I2CReadEx` / `I2CWriteEx` follows OpenRGB's
//! `i2c_smbus_nvapi.cpp` (facts only): a register byte when the operation has
//! one, words LSB first, a Block Write as `[count, data…]` after the register,
//! no Quick command.

use crate::NvapiError;
use huedeck_hal::smbus::SMBUS_BLOCK_MAX;

/// `NV_I2C_INFO_V3`, x64 layout (64 bytes). Pointer fields are set by the
/// caller right before the call and must outlive it.
#[repr(C)]
#[derive(Debug)]
pub struct NvI2cInfoV3 {
    /// [`NV_I2C_INFO_VER3`].
    pub version: u32,
    /// Display mask; 0 (no display, not a DDC transfer).
    pub display_mask: u32,
    /// `bIsDDCPort`; always 0: HueDeck never talks to display ports.
    pub is_ddc_port: u8,
    /// 7-bit device address shifted left by one.
    pub i2c_dev_address: u8,
    /// Register bytes (NULL for register-less transfers).
    pub reg_address: *mut u8,
    /// Number of register bytes (0 or 1).
    pub reg_addr_size: u32,
    /// Data buffer.
    pub data: *mut u8,
    /// Data bytes to transfer.
    pub size: u32,
    /// Must be `NVAPI_I2C_SPEED_DEPRECATED` (0xFFFF).
    pub i2c_speed: u32,
    /// `NV_I2C_SPEED` enum; 0 = default.
    pub i2c_speed_khz: i32,
    /// I2C port; RGB controllers sit on port 1.
    pub port_id: u8,
    /// `bIsPortIdSet`; 1.
    pub is_port_id_set: u32,
}

const _: () = assert!(core::mem::size_of::<NvI2cInfoV3>() == 64);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, is_ddc_port) == 8);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, i2c_dev_address) == 9);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, reg_address) == 16);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, reg_addr_size) == 24);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, data) == 32);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, size) == 40);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, i2c_speed) == 44);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, i2c_speed_khz) == 48);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, port_id) == 52);
const _: () = assert!(core::mem::offset_of!(NvI2cInfoV3, is_port_id_set) == 56);

/// `MAKE_NVAPI_VERSION(NV_I2C_INFO_V3, 3)`: size | version << 16.
pub const NV_I2C_INFO_VER3: u32 = 0x0003_0040;
/// `NVAPI_I2C_SPEED_DEPRECATED`.
pub const I2C_SPEED_DEPRECATED: u32 = 0xFFFF;
/// The GPU I2C port RGB controllers are attached to (OpenRGB: "Always use GPU port 1").
pub const RGB_PORT: u8 = 1;
/// Longest data buffer NvAPI accepts with a register address (more may give −116).
pub const MAX_REGISTER_READ: usize = 16;

/// One SMBus/I2C operation on an NvAPI bus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I2cOp {
    /// SMBus Quick command: not available through NvAPI.
    Quick,
    /// Read Byte Data.
    ReadByte {
        /// Command byte.
        reg: u8,
    },
    /// Write Byte Data.
    WriteByte {
        /// Command byte.
        reg: u8,
        /// Value.
        val: u8,
    },
    /// Read Word Data (LSB first).
    ReadWord {
        /// Command byte.
        reg: u8,
    },
    /// Write Word Data (LSB first).
    WriteWord {
        /// Command byte.
        reg: u8,
        /// Value.
        val: u16,
    },
    /// SMBus Block Write: count byte, then the data.
    WriteBlock {
        /// Command byte.
        reg: u8,
        /// 1..=32 bytes.
        data: Vec<u8>,
    },
    /// Register-addressed read of exactly `len` bytes (no count byte).
    ReadBlock {
        /// Command byte.
        reg: u8,
        /// 1..=16 bytes.
        len: usize,
    },
    /// Raw write without a register byte.
    WriteNoReg {
        /// 1..=32 bytes.
        data: Vec<u8>,
    },
    /// Raw read without a register byte.
    ReadNoReg {
        /// 1..=32 bytes.
        len: usize,
    },
    /// SMBus Receive Byte.
    ReadByteNoReg,
}

/// What one NvAPI transfer sends: direction, optional register byte, and the
/// data buffer (bytes to write, or a zeroed buffer of the length to read).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I2cPlan {
    /// `I2CWriteEx` (else `I2CReadEx`).
    pub write: bool,
    /// Register byte, if the operation has one.
    pub reg: Option<u8>,
    /// Data buffer.
    pub data: Vec<u8>,
}

fn check_len(len: usize, max: usize) -> Result<usize, NvapiError> {
    if (1..=max).contains(&len) {
        Ok(len)
    } else {
        Err(NvapiError::Length(len))
    }
}

impl I2cPlan {
    /// The transfer for `op` (OpenRGB's mapping, research §4).
    pub fn of(op: &I2cOp) -> Result<Self, NvapiError> {
        let read = |reg: Option<u8>, len: usize| Self {
            write: false,
            reg,
            data: vec![0; len],
        };
        let write = |reg: Option<u8>, data: Vec<u8>| Self {
            write: true,
            reg,
            data,
        };
        Ok(match op {
            I2cOp::Quick => return Err(NvapiError::Unsupported("WriteQuick")),
            I2cOp::ReadByte { reg } => read(Some(*reg), 1),
            I2cOp::WriteByte { reg, val } => write(Some(*reg), vec![*val]),
            I2cOp::ReadWord { reg } => read(Some(*reg), 2),
            I2cOp::WriteWord { reg, val } => write(Some(*reg), val.to_le_bytes().to_vec()),
            I2cOp::WriteBlock { reg, data } => {
                let len = check_len(data.len(), SMBUS_BLOCK_MAX)?;
                let mut payload = Vec::with_capacity(len + 1);
                payload.push(len as u8);
                payload.extend_from_slice(data);
                write(Some(*reg), payload)
            }
            I2cOp::ReadBlock { reg, len } => read(Some(*reg), check_len(*len, MAX_REGISTER_READ)?),
            I2cOp::WriteNoReg { data } => {
                check_len(data.len(), SMBUS_BLOCK_MAX)?;
                write(None, data.clone())
            }
            I2cOp::ReadNoReg { len } => read(None, check_len(*len, SMBUS_BLOCK_MAX)?),
            I2cOp::ReadByteNoReg => read(None, 1),
        })
    }
}

/// The info block for `plan` to the 7-bit `addr`, with null pointers (the
/// caller points them at buffers that outlive the call). Port 1, never DDC.
pub fn fill_info(plan: &I2cPlan, addr: u8) -> NvI2cInfoV3 {
    NvI2cInfoV3 {
        version: NV_I2C_INFO_VER3,
        display_mask: 0,
        is_ddc_port: 0,
        i2c_dev_address: addr << 1,
        reg_address: std::ptr::null_mut(),
        reg_addr_size: u32::from(plan.reg.is_some()),
        data: std::ptr::null_mut(),
        size: plan.data.len() as u32,
        i2c_speed: I2C_SPEED_DEPRECATED,
        i2c_speed_khz: 0,
        port_id: RGB_PORT,
        is_port_id_set: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(op: I2cOp) -> I2cPlan {
        I2cPlan::of(&op).unwrap()
    }

    #[test]
    fn info_v3_layout_is_64_bytes_with_documented_offsets() {
        assert_eq!(core::mem::size_of::<NvI2cInfoV3>(), 64);
        assert_eq!(core::mem::offset_of!(NvI2cInfoV3, port_id), 52);
        assert_eq!(core::mem::offset_of!(NvI2cInfoV3, is_port_id_set), 56);
        assert_eq!(NV_I2C_INFO_VER3, 64 | (3 << 16));
    }

    #[test]
    fn write_byte_plan_matches_openrgb() {
        assert_eq!(
            plan(I2cOp::WriteByte { reg: 0x30, val: 0x12 }),
            I2cPlan {
                write: true,
                reg: Some(0x30),
                data: vec![0x12]
            }
        );
        assert_eq!(
            plan(I2cOp::ReadByte { reg: 0x22 }),
            I2cPlan {
                write: false,
                reg: Some(0x22),
                data: vec![0]
            }
        );
        assert_eq!(plan(I2cOp::ReadByteNoReg).reg, None);
    }

    #[test]
    fn word_is_lsb_first() {
        assert_eq!(
            plan(I2cOp::WriteWord { reg: 0, val: 0x8300 }).data,
            vec![0x00, 0x83]
        );
        assert_eq!(plan(I2cOp::ReadWord { reg: 4 }).data.len(), 2);
    }

    #[test]
    fn block_write_prefixes_count() {
        let p = plan(I2cOp::WriteBlock {
            reg: 0x03,
            data: vec![1, 2, 3],
        });
        assert_eq!((p.reg, p.data), (Some(0x03), vec![3, 1, 2, 3]));
        assert!(I2cPlan::of(&I2cOp::WriteBlock {
            reg: 0,
            data: vec![0; 33]
        })
        .is_err());
        assert!(I2cPlan::of(&I2cOp::WriteBlock { reg: 0, data: vec![] }).is_err());
        assert_eq!(plan(I2cOp::WriteNoReg { data: vec![9; 32] }).data.len(), 32);
    }

    #[test]
    fn quick_is_unsupported() {
        assert_eq!(
            I2cPlan::of(&I2cOp::Quick),
            Err(NvapiError::Unsupported("WriteQuick"))
        );
    }

    #[test]
    fn address_is_shifted_and_port_1_never_ddc() {
        let info = fill_info(&plan(I2cOp::WriteByte { reg: 0x30, val: 1 }), 0x68);
        assert_eq!(info.i2c_dev_address, 0xD0);
        assert_eq!((info.port_id, info.is_port_id_set, info.is_ddc_port), (1, 1, 0));
        assert_eq!((info.reg_addr_size, info.size), (1, 1));
        assert_eq!(
            (info.i2c_speed, info.i2c_speed_khz, info.display_mask),
            (0xFFFF, 0, 0)
        );
        assert_eq!(info.version, NV_I2C_INFO_VER3);
        let raw = fill_info(&plan(I2cOp::ReadNoReg { len: 4 }), 0x68);
        assert_eq!((raw.reg_addr_size, raw.size), (0, 4));
    }

    #[test]
    fn register_reads_over_16_bytes_are_refused() {
        assert_eq!(plan(I2cOp::ReadBlock { reg: 0, len: 16 }).data.len(), 16);
        assert_eq!(
            I2cPlan::of(&I2cOp::ReadBlock { reg: 0, len: 17 }),
            Err(NvapiError::Length(17))
        );
        assert!(I2cPlan::of(&I2cOp::ReadNoReg { len: 0 }).is_err());
    }
}
