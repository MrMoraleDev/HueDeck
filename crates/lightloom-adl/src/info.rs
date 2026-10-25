// SPDX-License-Identifier: GPL-3.0-or-later
//! `ADLI2C`, `AdapterInfoX2` and the SMBus → ADL transfer mapping. Pure: no ADL call.
//!
//! The layouts are AMD's (`adl_structures.h`, MIT), declared here by hand with
//! their x64 sizes and offsets checked at compile time. The mapping of each
//! SMBus operation onto one `ADL2_Display_WriteAndReadI2C` follows OpenRGB's
//! `i2c_smbus_amdadl.cpp` (facts only):
//!
//! * a read names its register in `iOffset` and receives the data;
//! * a write has `iOffset = 0` and the register **prepended** to the data
//!   (`iDataSize = n + 1`): on the wire the register is just the first byte;
//! * a Block Write sends `[reg, count, data…]`;
//! * register-less transfers use `iOffset = 0` with the raw data.

use crate::AdlError;
use lightloom_hal::smbus::{PciIds, SMBUS_BLOCK_MAX};
use std::ffi::c_char;

/// `ADL_MAX_PATH`.
pub const ADL_MAX_PATH: usize = 256;
/// `ADL_DL_I2C_ACTIONREAD`.
pub const ACTION_READ: i32 = 1;
/// `ADL_DL_I2C_ACTIONWRITE`.
pub const ACTION_WRITE: i32 = 2;
/// The GPU I2C line RGB controllers sit on (OpenRGB: "location of the Aura chip").
pub const RGB_LINE: i32 = 1;
/// I2C clock in kHz (OpenRGB's value).
pub const I2C_SPEED_KHZ: i32 = 100;
/// AMD's PCI vendor id: the only adapters turned into buses.
pub const AMD_VENDOR: u16 = 0x1002;
/// Longest register-addressed read (an I2C block read has no count byte).
pub const MAX_READ: usize = SMBUS_BLOCK_MAX;

/// `ADLI2C`, x64 layout (40 bytes). `pcData` is set by the caller right
/// before the call and must outlive it.
#[repr(C)]
#[derive(Debug)]
pub struct AdlI2c {
    /// `iSize`: `sizeof(ADLI2C)`.
    pub size: i32,
    /// `iLine`: the hardware I2C line; [`RGB_LINE`].
    pub line: i32,
    /// `iAddress`: 7-bit device address shifted left by one.
    pub address: i32,
    /// `iOffset`: the register a read starts at (0 for writes).
    pub offset: i32,
    /// `iAction`: [`ACTION_READ`] or [`ACTION_WRITE`].
    pub action: i32,
    /// `iSpeed`: clock in kHz.
    pub speed: i32,
    /// `iDataSize`: bytes sent or received.
    pub data_size: i32,
    /// `pcData`: the buffer.
    pub data: *mut u8,
}

const _: () = assert!(core::mem::size_of::<AdlI2c>() == 40);
const _: () = assert!(core::mem::offset_of!(AdlI2c, offset) == 12);
const _: () = assert!(core::mem::offset_of!(AdlI2c, data_size) == 24);
const _: () = assert!(core::mem::offset_of!(AdlI2c, data) == 32);

/// `AdapterInfoX2` (1580 bytes, 4-byte aligned), as `ADL2_Adapter_AdapterInfoX4_Get` returns it.
#[repr(C)]
#[derive(Clone)]
pub struct AdapterInfoX2 {
    /// `iSize`.
    pub size: i32,
    /// `iAdapterIndex`: the ADL index (one GPU may have several).
    pub adapter_index: i32,
    /// `strUDID`.
    pub udid: [c_char; ADL_MAX_PATH],
    /// `iBusNumber`: PCI bus.
    pub bus_number: i32,
    /// `iDeviceNumber`.
    pub device_number: i32,
    /// `iFunctionNumber`.
    pub function_number: i32,
    /// `iVendorID`.
    pub vendor_id: i32,
    /// `strAdapterName` ("AMD Radeon RX 6700 XT").
    pub adapter_name: [c_char; ADL_MAX_PATH],
    /// `strDisplayName`.
    pub display_name: [c_char; ADL_MAX_PATH],
    /// `iPresent`.
    pub present: i32,
    /// `iExist`.
    pub exist: i32,
    /// `strDriverPath`.
    pub driver_path: [c_char; ADL_MAX_PATH],
    /// `strDriverPathExt`.
    pub driver_path_ext: [c_char; ADL_MAX_PATH],
    /// `strPNPString` (`PCI\VEN_1002&DEV_73DF&SUBSYS_E4451DA2&REV_C1\…`).
    pub pnp_string: [c_char; ADL_MAX_PATH],
    /// `iOSDisplayIndex`.
    pub os_display_index: i32,
    /// `iInfoMask`.
    pub info_mask: i32,
    /// `iInfoValue`.
    pub info_value: i32,
}

const _: () = assert!(core::mem::size_of::<AdapterInfoX2>() == 1580);
const _: () = assert!(core::mem::offset_of!(AdapterInfoX2, bus_number) == 264);
const _: () = assert!(core::mem::offset_of!(AdapterInfoX2, adapter_name) == 280);
const _: () = assert!(core::mem::offset_of!(AdapterInfoX2, pnp_string) == 1312);

impl AdapterInfoX2 {
    /// An all-zero record (tests).
    pub fn zeroed() -> Self {
        Self {
            size: core::mem::size_of::<Self>() as i32,
            adapter_index: 0,
            udid: [0; ADL_MAX_PATH],
            bus_number: 0,
            device_number: 0,
            function_number: 0,
            vendor_id: 0,
            adapter_name: [0; ADL_MAX_PATH],
            display_name: [0; ADL_MAX_PATH],
            present: 0,
            exist: 0,
            driver_path: [0; ADL_MAX_PATH],
            driver_path_ext: [0; ADL_MAX_PATH],
            pnp_string: [0; ADL_MAX_PATH],
            os_display_index: 0,
            info_mask: 0,
            info_value: 0,
        }
    }

    /// The fields Lightloom uses.
    pub fn record(&self) -> AdapterRecord {
        AdapterRecord {
            index: self.adapter_index,
            bus: self.bus_number,
            name: c_string(&self.adapter_name),
            pnp: c_string(&self.pnp_string),
        }
    }
}

/// A NUL-terminated (or full) `char` array as a trimmed string.
pub fn c_string(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// One adapter as ADL lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterRecord {
    /// ADL adapter index.
    pub index: i32,
    /// PCI bus number.
    pub bus: i32,
    /// Adapter name.
    pub name: String,
    /// Windows PNP string.
    pub pnp: String,
}

fn hex4(s: &str, at: usize) -> Option<u16> {
    u16::from_str_radix(s.get(at..at + 4)?, 16).ok()
}

/// PCI ids from a PNP string (`…VEN_vvvv&DEV_dddd&SUBSYS_ssssSSSS…`, where the
/// SUBSYS field is the subsystem device, then the subsystem vendor), as
/// OpenRGB parses it. `None` when a field is missing or not hex.
pub fn parse_pnp(pnp: &str) -> Option<PciIds> {
    let upper = pnp.to_ascii_uppercase();
    let ven = upper.find("VEN_")? + 4;
    let dev = upper.find("DEV_")? + 4;
    let sub = upper.find("SUBSYS_")? + 7;
    Some(PciIds {
        vendor: hex4(&upper, ven)?,
        device: hex4(&upper, dev)?,
        sub_device: hex4(&upper, sub)?,
        sub_vendor: hex4(&upper, sub + 4)?,
    })
}

/// The adapters that become buses: one per PCI bus (the first ADL index of
/// each; ADL lists one entry per display output), AMD vendor only, with a
/// parseable PNP string. Returns `(adapter index, name, PCI ids, bus)`.
pub fn select_gpus(adapters: &[AdapterRecord]) -> Vec<(i32, String, PciIds, u32)> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for a in adapters {
        if seen.contains(&a.bus) {
            continue;
        }
        seen.push(a.bus);
        let Some(pci) = parse_pnp(&a.pnp) else {
            continue;
        };
        if pci.vendor != AMD_VENDOR {
            continue;
        }
        let name = if a.name.is_empty() {
            "AMD Radeon GPU".to_string()
        } else {
            a.name.clone()
        };
        out.push((a.index, name, pci, a.bus.max(0) as u32));
    }
    out
}

/// One SMBus/I2C operation on an ADL bus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I2cOp {
    /// SMBus Quick command: a zero-length write.
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
        /// 1..=32 bytes.
        len: usize,
    },
    /// Raw write without a register byte.
    WriteNoReg {
        /// 1..=32 bytes.
        data: Vec<u8>,
    },
    /// Raw read without a register byte (sent with offset 0).
    ReadNoReg {
        /// 1..=32 bytes.
        len: usize,
    },
    /// SMBus Receive Byte: not available through ADL.
    ReadByteNoReg,
}

/// What one ADL transfer sends: direction, `iOffset`, and the data buffer
/// (the bytes to write, register first; or a zeroed buffer of the length to read).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I2cPlan {
    /// [`ACTION_WRITE`] (else [`ACTION_READ`]).
    pub write: bool,
    /// `iOffset`: the register a read starts at; 0 for writes.
    pub offset: u8,
    /// Data buffer.
    pub data: Vec<u8>,
}

fn check_len(len: usize, max: usize) -> Result<usize, AdlError> {
    if (1..=max).contains(&len) {
        Ok(len)
    } else {
        Err(AdlError::Length(len))
    }
}

impl I2cPlan {
    /// The transfer for `op` (OpenRGB's mapping, module docs).
    pub fn of(op: &I2cOp) -> Result<Self, AdlError> {
        let read = |offset: u8, len: usize| Self {
            write: false,
            offset,
            data: vec![0; len],
        };
        let write = |data: Vec<u8>| Self {
            write: true,
            offset: 0,
            data,
        };
        Ok(match op {
            I2cOp::Quick => write(Vec::new()),
            I2cOp::ReadByte { reg } => read(*reg, 1),
            I2cOp::WriteByte { reg, val } => write(vec![*reg, *val]),
            I2cOp::ReadWord { reg } => read(*reg, 2),
            I2cOp::WriteWord { reg, val } => {
                let [lo, hi] = val.to_le_bytes();
                write(vec![*reg, lo, hi])
            }
            I2cOp::WriteBlock { reg, data } => {
                let len = check_len(data.len(), SMBUS_BLOCK_MAX)?;
                let mut payload = Vec::with_capacity(len + 2);
                payload.push(*reg);
                payload.push(len as u8);
                payload.extend_from_slice(data);
                write(payload)
            }
            I2cOp::ReadBlock { reg, len } => read(*reg, check_len(*len, MAX_READ)?),
            I2cOp::WriteNoReg { data } => {
                check_len(data.len(), SMBUS_BLOCK_MAX)?;
                write(data.clone())
            }
            I2cOp::ReadNoReg { len } => read(0, check_len(*len, SMBUS_BLOCK_MAX)?),
            I2cOp::ReadByteNoReg => return Err(AdlError::Unsupported("ReadByteWithoutRegister")),
        })
    }
}

/// The `ADLI2C` for `plan` to the 7-bit `addr`, with a null data pointer
/// (the caller points it at a buffer that outlives the call). Line 1, 100 kHz.
pub fn fill_i2c(plan: &I2cPlan, addr: u8) -> AdlI2c {
    AdlI2c {
        size: core::mem::size_of::<AdlI2c>() as i32,
        line: RGB_LINE,
        address: i32::from(addr) << 1,
        offset: i32::from(plan.offset),
        action: if plan.write { ACTION_WRITE } else { ACTION_READ },
        speed: I2C_SPEED_KHZ,
        data_size: plan.data.len() as i32,
        data: std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(op: I2cOp) -> I2cPlan {
        I2cPlan::of(&op).unwrap()
    }

    #[test]
    fn layouts_match_the_sdk() {
        assert_eq!(core::mem::size_of::<AdlI2c>(), 40);
        assert_eq!(core::mem::size_of::<AdapterInfoX2>(), 1580);
        assert_eq!(core::mem::offset_of!(AdapterInfoX2, os_display_index), 1568);
    }

    #[test]
    fn reads_name_the_register_in_the_offset() {
        assert_eq!(
            plan(I2cOp::ReadByte { reg: 0x1A }),
            I2cPlan {
                write: false,
                offset: 0x1A,
                data: vec![0]
            }
        );
        assert_eq!(plan(I2cOp::ReadWord { reg: 4 }).data.len(), 2);
        assert_eq!(plan(I2cOp::ReadBlock { reg: 0x10, len: 32 }).data.len(), 32);
        assert_eq!(
            I2cPlan::of(&I2cOp::ReadBlock { reg: 0, len: 33 }),
            Err(AdlError::Length(33))
        );
        assert_eq!(plan(I2cOp::ReadNoReg { len: 4 }).offset, 0);
    }

    #[test]
    fn writes_prepend_the_register() {
        assert_eq!(
            plan(I2cOp::WriteByte { reg: 0x10, val: 0x06 }),
            I2cPlan {
                write: true,
                offset: 0,
                data: vec![0x10, 0x06]
            }
        );
        assert_eq!(
            plan(I2cOp::WriteWord { reg: 0, val: 0x8300 }).data,
            vec![0x00, 0x00, 0x83],
            "LSB first"
        );
        assert_eq!(
            plan(I2cOp::WriteBlock {
                reg: 0x03,
                data: vec![1, 2, 3]
            })
            .data,
            vec![0x03, 3, 1, 2, 3],
            "register, count, data"
        );
        assert!(I2cPlan::of(&I2cOp::WriteBlock { reg: 0, data: vec![] }).is_err());
        assert!(I2cPlan::of(&I2cOp::WriteBlock {
            reg: 0,
            data: vec![0; 33]
        })
        .is_err());
        assert_eq!(plan(I2cOp::WriteNoReg { data: vec![9; 3] }).data, vec![9, 9, 9]);
    }

    #[test]
    fn quick_is_a_zero_length_write_and_receive_byte_is_refused() {
        let q = plan(I2cOp::Quick);
        assert!(q.write && q.data.is_empty());
        assert_eq!(fill_i2c(&q, 0x36).data_size, 0);
        assert_eq!(
            I2cPlan::of(&I2cOp::ReadByteNoReg),
            Err(AdlError::Unsupported("ReadByteWithoutRegister"))
        );
    }

    #[test]
    fn i2c_block_is_line_1_at_100_khz() {
        let w = fill_i2c(&plan(I2cOp::WriteByte { reg: 0x1A, val: 0xFF }), 0x28);
        assert_eq!(
            (
                w.size,
                w.line,
                w.address,
                w.offset,
                w.action,
                w.speed,
                w.data_size
            ),
            (40, 1, 0x50, 0, ACTION_WRITE, 100, 2)
        );
        let r = fill_i2c(&plan(I2cOp::ReadByte { reg: 0x10 }), 0x28);
        assert_eq!((r.offset, r.action, r.data_size), (0x10, ACTION_READ, 1));
        assert!(w.data.is_null());
    }

    #[test]
    fn pnp_strings_parse_like_openrgb() {
        assert_eq!(
            parse_pnp(r"PCI\VEN_1002&DEV_73DF&SUBSYS_E4451DA2&REV_C1\6&2a7c4bf0&0&00000019"),
            Some(PciIds {
                vendor: 0x1002,
                device: 0x73DF,
                sub_vendor: 0x1DA2,
                sub_device: 0xE445
            })
        );
        assert_eq!(
            parse_pnp(r"pci\ven_1002&dev_744c&subsys_e4711da2")
                .unwrap()
                .device,
            0x744C
        );
        assert_eq!(parse_pnp(r"PCI\VEN_1002&DEV_73DF"), None, "no SUBSYS");
        assert_eq!(parse_pnp(r"PCI\VEN_10XZ&DEV_73DF&SUBSYS_E4451DA2"), None);
        assert_eq!(
            parse_pnp(r"PCI\VEN_1002&DEV_73DF&SUBSYS_E445"),
            None,
            "short SUBSYS"
        );
    }

    fn rec(index: i32, bus: i32, pnp: &str) -> AdapterRecord {
        AdapterRecord {
            index,
            bus,
            name: "AMD Radeon RX 6700 XT".into(),
            pnp: pnp.into(),
        }
    }

    #[test]
    fn one_bus_per_amd_gpu() {
        const RX6700: &str = r"PCI\VEN_1002&DEV_73DF&SUBSYS_E4451DA2&REV_C1";
        let list = [
            rec(0, 3, RX6700),
            rec(1, 3, RX6700),
            rec(2, 3, RX6700),
            rec(3, 0, r"PCI\VEN_8086&DEV_4680&SUBSYS_86941043"),
            rec(4, 7, r"PCI\VEN_1002&DEV_744C&SUBSYS_E4711DA2"),
            rec(5, 3, RX6700),
            rec(6, 9, "garbage"),
        ];
        let gpus = select_gpus(&list);
        assert_eq!(gpus.len(), 2);
        assert_eq!((gpus[0].0, gpus[0].3, gpus[0].2.sub_vendor), (0, 3, 0x1DA2));
        assert_eq!((gpus[1].0, gpus[1].3, gpus[1].2.device), (4, 7, 0x744C));
    }

    #[test]
    fn adapter_record_reads_c_strings() {
        let mut a = AdapterInfoX2::zeroed();
        a.adapter_index = 2;
        a.bus_number = 3;
        for (i, b) in b"AMD Radeon RX 6700 XT ".iter().enumerate() {
            a.adapter_name[i] = *b as c_char;
        }
        for (i, b) in br"PCI\VEN_1002&DEV_73DF&SUBSYS_E4451DA2".iter().enumerate() {
            a.pnp_string[i] = *b as c_char;
        }
        let r = a.record();
        assert_eq!((r.index, r.bus), (2, 3));
        assert_eq!(r.name, "AMD Radeon RX 6700 XT");
        assert!(parse_pnp(&r.pnp).is_some());
        let mut full = AdapterInfoX2::zeroed();
        full.adapter_name = [b'A' as c_char; ADL_MAX_PATH];
        assert_eq!(full.record().name.len(), ADL_MAX_PATH, "no NUL: the whole array");
    }
}
