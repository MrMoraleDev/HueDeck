// SPDX-License-Identifier: GPL-3.0-or-later
//! NvAPI status codes (NVIDIA `nvapi_lite_common.h`, MIT).
//!
//! NvAPI has no dedicated I2C NACK status: a device that does not answer
//! shows up as [`NvStatus::Error`] or [`NvStatus::PortIdNotFound`].

/// The NvAPI statuses HueDeck names; any other value passes through as is.
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
    /// NvAPI not initialised (re-initialise and retry once).
    ApiNotInitialized = -4,
    /// Invalid argument.
    InvalidArgument = -5,
    /// No NVIDIA device found.
    NvidiaDeviceNotFound = -6,
    /// No more items to enumerate.
    EndEnumeration = -7,
    /// GPU handle invalid.
    InvalidHandle = -8,
    /// Struct version not supported.
    IncompatibleStructVersion = -9,
    /// Handles are stale, e.g. after a driver reset (re-enumerate and retry once).
    HandleInvalidated = -10,
    /// Not supported.
    NotSupported = -104,
    /// No port id found for the I2C transaction.
    PortIdNotFound = -105,
    /// Device busy.
    DeviceBusy = -108,
    /// Argument exceeds the maximum size (more than 16 bytes with a register).
    ArgumentExceedMaxSize = -116,
    /// Timeout.
    Timeout = -191,
}

/// Every named status, for lookups.
const NAMED: &[(i32, &str)] = &[
    (0, "NVAPI_OK"),
    (-1, "NVAPI_ERROR"),
    (-2, "NVAPI_LIBRARY_NOT_FOUND"),
    (-3, "NVAPI_NO_IMPLEMENTATION"),
    (-4, "NVAPI_API_NOT_INITIALIZED"),
    (-5, "NVAPI_INVALID_ARGUMENT"),
    (-6, "NVAPI_NVIDIA_DEVICE_NOT_FOUND"),
    (-7, "NVAPI_END_ENUMERATION"),
    (-8, "NVAPI_INVALID_HANDLE"),
    (-9, "NVAPI_INCOMPATIBLE_STRUCT_VERSION"),
    (-10, "NVAPI_HANDLE_INVALIDATED"),
    (-104, "NVAPI_NOT_SUPPORTED"),
    (-105, "NVAPI_PORTID_NOT_FOUND"),
    (-108, "NVAPI_DEVICE_BUSY"),
    (-116, "NVAPI_ARGUMENT_EXCEED_MAX_SIZE"),
    (-191, "NVAPI_TIMEOUT"),
];

/// The NVIDIA name of `status` (`"NVAPI_ERROR"`), or `"unknown"`.
pub fn name(status: i32) -> &'static str {
    NAMED
        .iter()
        .find(|(code, _)| *code == status)
        .map_or("unknown", |(_, n)| n)
}

/// Whether `status` asks for a re-initialise and re-enumerate before one retry.
pub fn needs_reinit(status: i32) -> bool {
    status == NvStatus::ApiNotInitialized as i32 || status == NvStatus::HandleInvalidated as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names_cover_documented_codes() {
        for s in [
            NvStatus::Ok,
            NvStatus::Error,
            NvStatus::LibraryNotFound,
            NvStatus::NoImplementation,
            NvStatus::ApiNotInitialized,
            NvStatus::InvalidArgument,
            NvStatus::NvidiaDeviceNotFound,
            NvStatus::EndEnumeration,
            NvStatus::InvalidHandle,
            NvStatus::IncompatibleStructVersion,
            NvStatus::HandleInvalidated,
            NvStatus::NotSupported,
            NvStatus::PortIdNotFound,
            NvStatus::DeviceBusy,
            NvStatus::ArgumentExceedMaxSize,
            NvStatus::Timeout,
        ] {
            assert_ne!(name(s as i32), "unknown", "{s:?}");
        }
        assert_eq!(name(-105), "NVAPI_PORTID_NOT_FOUND");
        assert_eq!(name(-39), "unknown", "there is no I2C NACK status");
        assert!(needs_reinit(-4) && needs_reinit(-10) && !needs_reinit(-1));
    }
}
