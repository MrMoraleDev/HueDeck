// SPDX-License-Identifier: GPL-3.0-or-later
//! Error type shared by every HAL backend.

/// Errors reported by HAL backends.
///
/// Plugin-facing layers map these onto the SignalRGB conventions
/// (`write` returns `-1`, SMBus methods return a negative status) and never
/// let them escape as JS exceptions.
#[derive(Debug, thiserror::Error)]
pub enum HalError {
    /// No device/endpoint/bus with the given identifier exists.
    #[error("not found: {0}")]
    NotFound(String),
    /// The device is already opened exclusively by another process.
    #[error("busy: {0}")]
    Busy(String),
    /// The OS refused access (udev rules, driver binding, ACLs).
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// The interface or device is owned by a driver this process may not
    /// use (raw USB without WinUSB). The text says what to do; HueDeck never
    /// installs or swaps a driver.
    #[error("{0}")]
    NeedsDriver(String),
    /// The device went away while a handle was open.
    #[error("device disconnected")]
    Disconnected,
    /// A blocking operation did not complete within the deadline.
    #[error("operation timed out after {0} ms")]
    Timeout(u32),
    /// The backend cannot perform this operation (e.g. `WriteQuick` on an NvAPI bus).
    #[error("unsupported operation: {0}")]
    Unsupported(&'static str),
    /// Caller passed an out-of-range argument.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Bus-level failure (SMBus NACK, I2C timeout). `status` is the negative
    /// native status code when one exists, `-1` otherwise.
    #[error("bus error (status {status})")]
    Bus {
        /// Negative native status code.
        status: i32,
    },
    /// Any other backend-specific failure, with its text.
    #[error("backend error: {0}")]
    Backend(String),
    /// Plain I/O error from the OS.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result alias used throughout the HAL.
pub type HalResult<T> = Result<T, HalError>;

impl HalError {
    /// Status code to hand back to plugins that expect the SignalRGB
    /// `0 = ok, negative = failure` convention.
    pub fn plugin_status(&self) -> i32 {
        match self {
            HalError::Bus { status } if *status < 0 => *status,
            _ => -1,
        }
    }
}
