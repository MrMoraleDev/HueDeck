// SPDX-License-Identifier: GPL-3.0-or-later
//! Dynamic NvAPI binding (M5).
//!
//! `nvapi64.dll` is loaded at runtime through `nvapi_QueryInterface`; Lightloom
//! declares the handful of function ids it needs itself (GPU enumeration, PCI
//! identifiers, I2C read/write with and without register, thermal). Nothing is
//! linked at build time, so machines without an NVIDIA driver run unaffected.
//!
//! Capability notes for the `bus.*` mapping: NvAPI buses have **no
//! `WriteQuick`**, and `ReadByteWithoutRegister` exists **only** here.

#![forbid(unsafe_code)]

/// DLL name on 64-bit Windows.
pub const NVAPI_DLL: &str = "nvapi64.dll";

/// Exported entry point used to look up every other function.
pub const QUERY_INTERFACE_SYMBOL: &str = "nvapi_QueryInterface";

/// NvAPI status codes Lightloom interprets (others are passed through as negative statuses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum NvStatus {
    /// Success.
    Ok = 0,
    /// Generic error.
    Error = -1,
    /// Library not found.
    LibraryNotFound = -2,
    /// Function not found.
    NoImplementation = -3,
    /// NvAPI not initialised.
    ApiNotInitialized = -4,
    /// Invalid argument.
    InvalidArgument = -5,
    /// GPU handle invalid.
    InvalidHandle = -8,
    /// Port id not found (`NVAPI_PORTID_NOT_FOUND`). NvAPI has no dedicated I2C NACK
    /// status: a missing device surfaces as [`NvapiStatus::Error`] or as this code.
    PortIdNotFound = -105,
}

/// Error type of the (future) loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvapiError {
    /// `nvapi64.dll` could not be loaded (no NVIDIA driver).
    NotAvailable,
    /// A function id returned null from `nvapi_QueryInterface`.
    MissingFunction(u32),
    /// NvAPI returned a non-zero status.
    Status(i32),
}

impl std::fmt::Display for NvapiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NvapiError::NotAvailable => f.write_str("NvAPI not available"),
            NvapiError::MissingFunction(id) => write!(f, "NvAPI function {id:#010x} missing"),
            NvapiError::Status(s) => write!(f, "NvAPI status {s}"),
        }
    }
}

impl std::error::Error for NvapiError {}

/// Whether NvAPI can be loaded on this machine. Always `false` in the M0 stub.
pub fn is_available() -> bool {
    false
}
