// SPDX-License-Identifier: GPL-3.0-or-later
//! Dynamic AMD ADL binding: AMD GPU I2C as a HAL SMBus bus (WP-AMDGPU).
//!
//! `atiadlxx.dll` (the legacy AMD Display Library, shipped with every AMD
//! graphics driver) is loaded at runtime from System32 and its `ADL2_*`
//! functions resolved by name; nothing is linked at build time, so machines
//! without an AMD driver run unaffected ([`AdlError::NotAvailable`]). GPU I2C
//! runs in the user's own, unelevated process, like NvAPI: no driver, no
//! broker (research drivers.md §3).
//!
//! The structures and constants are declared here by hand from AMD's public
//! ADL SDK headers (`adl_sdk.h`, `adl_structures.h`, `adl_defines.h`, MIT).
//! ADLX, whose headers are not GPL-compatible, is not used. The call sequence
//! and the SMBus mapping follow OpenRGB's `i2c_smbus_amdadl.cpp`
//! (GPL-2.0-or-later, read for facts at commit d4798c12).
//!
//! * [`info`]: the `ADLI2C` / `AdapterInfoX2` layouts, how each SMBus
//!   operation maps onto one `ADL2_Display_WriteAndReadI2C`, the PNP string
//!   parser and the adapter selection; pure and tested.
//! * [`mem`]: the `ADL_MAIN_MALLOC_CALLBACK` ADL allocates its results with.
//! * `ffi` / `api` (Windows): the loader and the thin calls; the only FFI code.
//! * [`hub`]: one worker thread `huedeck-adl` owns the ADL context; [`hub::AdlHub`]
//!   hands out one [`bus::AdlBus`] per GPU.
//! * [`bus`]: [`huedeck_hal::SmbusBus`] over the hub, kind
//!   [`huedeck_hal::SmbusBusKind::AmdGpu`] (`bus.IsAMDGpuBus()`).
//!
//! Capability notes for the `bus.*` mapping: every transfer goes to I2C line 1
//! (OpenRGB: "location of the Aura chip") at 100 kHz. `WriteQuick` is a
//! zero-length write; SMBus Receive Byte (`ReadByteWithoutRegister`) does not
//! exist (an ADL read always names an offset). Register-less reads are sent
//! with offset 0, as OpenRGB does; whether the driver puts that offset on the
//! wire is unverified.
//!
//! No cross-process mutex is taken, for the reason `huedeck-nvapi` gives:
//! the GPU's own I2C controller is not the chipset SMBus. One worker thread
//! per process serialises the transfers to every AMD GPU.

#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

#[cfg(windows)]
mod api;
pub mod bus;
#[cfg(windows)]
mod ffi;
pub mod hub;
pub mod info;
pub mod mem;
pub mod status;

pub use bus::AdlBus;
pub use hub::{AdlHub, FakeAdlI2c, GpuInfo, I2cTransport};
pub use info::{I2cOp, I2cPlan};

/// DLL name on 64-bit Windows.
pub const ADL_DLL: &str = "atiadlxx.dll";

/// Errors of the ADL binding.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdlError {
    /// `atiadlxx.dll` could not be loaded (no AMD driver, or not Windows).
    #[error("ADL is not available (no AMD graphics driver)")]
    NotAvailable,
    /// An `ADL2_*` export is missing from the DLL.
    #[error("ADL function {0} is missing")]
    MissingFunction(&'static str),
    /// ADL returned a negative status.
    #[error("ADL status {0} ({name})", name = status::name(*.0))]
    Status(i32),
    /// The operation does not exist on ADL buses.
    #[error("unsupported on AMD ADL buses: {0}")]
    Unsupported(&'static str),
    /// A transfer length outside what the operation allows.
    #[error("transfer length {0} outside the allowed range")]
    Length(usize),
    /// The worker thread did not answer in time.
    #[error("ADL worker did not answer within {0} ms")]
    Timeout(u32),
    /// The worker thread is gone.
    #[error("ADL worker thread has stopped")]
    WorkerGone,
}

/// Whether `atiadlxx.dll` can be loaded from System32 with every function
/// HueDeck needs. No ADL function is called (the library is loaded and freed
/// again).
#[cfg(windows)]
pub fn is_available() -> bool {
    ffi::Library::load().is_ok()
}

/// ADL is used on Windows only (Linux reaches AMD GPU I2C through `/dev/i2c-*`).
#[cfg(not(windows))]
pub fn is_available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This repository's test PC has an NVIDIA card and no AMD driver: the
    /// load must fail softly, never panic, and the hub must say so. On a PC
    /// with an AMD driver the hub starts (enumeration only, no transfer).
    #[test]
    fn missing_driver_is_a_soft_failure() {
        match AdlHub::start() {
            Ok(hub) => assert!(is_available(), "a hub started without the DLL: {hub:?}"),
            Err(e) => {
                if !is_available() {
                    assert_eq!(e, AdlError::NotAvailable);
                }
            }
        }
        if !is_available() {
            assert_eq!(
                AdlHub::start().unwrap_err(),
                AdlError::NotAvailable,
                "twice in a row"
            );
        }
    }
}
