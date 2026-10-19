// SPDX-License-Identifier: GPL-3.0-or-later
//! Dynamic NvAPI binding: NVIDIA GPU I2C as a HAL SMBus bus (M5).
//!
//! `nvapi64.dll` is loaded at runtime from System32 and every function is
//! resolved through `nvapi_QueryInterface` with the ids in [`ids`]; nothing is
//! linked at build time, so machines without an NVIDIA driver run unaffected.
//! GPU I2C runs in the user's own process (research m5-privileged-buses.md
//! §0.5, §4): a non-elevated process can reach the RGB controllers on port 1.
//!
//! * [`info`]: the `NV_I2C_INFO_V3` layout and how each SMBus operation maps
//!   onto an NvAPI transfer (OpenRGB's mapping), pure and tested.
//! * `ffi` / `api` (Windows): the loader and the thin calls; the only unsafe code.
//! * [`hub`]: one worker thread `lightloom-nvapi` owns NvAPI (its handles must
//!   not cross threads); [`hub::NvapiHub`] hands out one [`bus::NvapiBus`] per GPU.
//! * [`bus`]: [`lightloom_hal::SmbusBus`] over the hub. NvAPI status codes pass
//!   through as negative bus statuses, the way SignalRGB plugins see them.
//!
//! Capability notes for the `bus.*` mapping: NvAPI buses have **no
//! `WriteQuick`**; register-addressed reads are limited to 16 bytes
//! (`NVAPI_ARGUMENT_EXCEED_MAX_SIZE` otherwise). DDC/display ports are never
//! used: every transfer goes to port 1 with `bIsDDCPort = 0`.
//!
//! Every transfer runs under the global SMBus mutex
//! `Global\Access_SMBUS.HTP.Method` (`mutex`, Windows), as Lightloom's hard
//! rule asks for all SMBus traffic. NvAPI I2C is a different controller from
//! the chipset SMBus and neither OpenRGB nor SignalRGB takes the mutex here
//! (§4); Lightloom takes it anyway, at the cost of at most [`hub::LOCK_TIMEOUT_MS`]
//! of waiting (ADR-0004).

#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

#[cfg(windows)]
mod api;
pub mod bus;
#[cfg(windows)]
mod ffi;
pub mod hub;
pub mod ids;
pub mod info;
#[cfg(windows)]
mod mutex;
pub mod status;

pub use bus::NvapiBus;
pub use hub::{FakeLock, FakeNvI2c, GlobalLock, GpuInfo, I2cTransport, NoLock, NvapiHub};
pub use info::{I2cOp, I2cPlan};
pub use status::NvStatus;

/// DLL name on 64-bit Windows.
pub const NVAPI_DLL: &str = "nvapi64.dll";

/// Exported entry point used to look up every other function.
pub const QUERY_INTERFACE_SYMBOL: &str = "nvapi_QueryInterface";

/// Errors of the NvAPI binding.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NvapiError {
    /// `nvapi64.dll` could not be loaded (no NVIDIA driver, or not Windows).
    #[error("NvAPI is not available (no NVIDIA driver)")]
    NotAvailable,
    /// A function id returned null from `nvapi_QueryInterface`.
    #[error("NvAPI function {0:#010x} is missing")]
    MissingFunction(u32),
    /// NvAPI returned a non-zero status.
    #[error("NvAPI status {0} ({name})", name = status::name(*.0))]
    Status(i32),
    /// The operation does not exist on NvAPI buses.
    #[error("unsupported on NvAPI buses: {0}")]
    Unsupported(&'static str),
    /// A transfer length outside what the operation allows.
    #[error("transfer length {0} outside the allowed range")]
    Length(usize),
    /// The worker thread did not answer in time.
    #[error("NvAPI worker did not answer within {0} ms")]
    Timeout(u32),
    /// The worker thread is gone.
    #[error("NvAPI worker thread has stopped")]
    WorkerGone,
    /// The global SMBus mutex could not be opened or waited for.
    #[error("global SMBus mutex: {0}")]
    GlobalMutex(String),
}

/// Whether `nvapi64.dll` can be loaded from System32. No NvAPI function is
/// called (the library is loaded and freed again).
#[cfg(windows)]
pub fn is_available() -> bool {
    ffi::Library::load().is_ok()
}

/// NvAPI exists on Windows only.
#[cfg(not(windows))]
pub fn is_available() -> bool {
    false
}
