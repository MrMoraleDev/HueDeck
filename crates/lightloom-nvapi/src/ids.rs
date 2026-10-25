// SPDX-License-Identifier: GPL-3.0-or-later
//! `nvapi_QueryInterface` ids (research m5-privileged-buses.md §4).
//!
//! The public ids come from NVIDIA's MIT-licensed `nvapi_interface.h`; the two
//! `Ex` I2C functions are not in NVIDIA's headers and are the ids OpenRGB uses
//! (`dependencies/NVFC/nvapi.cpp`). All of them resolve on driver 581.15.

/// `NvAPI_Initialize`.
pub const INITIALIZE: u32 = 0x0150_E828;
/// `NvAPI_Unload`.
pub const UNLOAD: u32 = 0xD22B_DD7E;
/// `NvAPI_EnumPhysicalGPUs`.
pub const ENUM_PHYSICAL_GPUS: u32 = 0xE5AC_921F;
/// `NvAPI_GPU_GetFullName`.
pub const GPU_GET_FULL_NAME: u32 = 0xCEEE_8E9F;
/// `NvAPI_GPU_GetPCIIdentifiers`.
pub const GPU_GET_PCI_IDENTIFIERS: u32 = 0x2DDF_B66E;
/// `NvAPI_GPU_GetBusId`.
pub const GPU_GET_BUS_ID: u32 = 0x1BE0_B8E5;
/// `NvAPI_I2CReadEx` (community id, OpenRGB).
pub const I2C_READ_EX: u32 = 0x4D7B_0709;
/// `NvAPI_I2CWriteEx` (community id, OpenRGB).
pub const I2C_WRITE_EX: u32 = 0x283A_C65A;
/// `NvAPI_SYS_GetDriverAndBranchVersion`.
pub const SYS_GET_DRIVER_AND_BRANCH_VERSION: u32 = 0x2926_AAAD;

// Read-only sensors (public ids, `nvapi_interface.h`).

/// `NvAPI_GPU_GetThermalSettings`.
pub const GPU_GET_THERMAL_SETTINGS: u32 = 0xE364_0A56;
/// `NvAPI_GPU_GetDynamicPstatesInfoEx`.
pub const GPU_GET_DYNAMIC_PSTATES_INFO_EX: u32 = 0x60DE_D2ED;
/// `NvAPI_GPU_GetTachReading`.
pub const GPU_GET_TACH_READING: u32 = 0x5F60_8315;
/// `NvAPI_GPU_GetAllClockFrequencies`.
pub const GPU_GET_ALL_CLOCK_FREQUENCIES: u32 = 0xDCB6_16C3;
