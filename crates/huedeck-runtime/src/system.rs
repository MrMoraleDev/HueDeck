// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Machine facts and this process's memory.

/// Motherboard / BIOS / memory facts for `@SignalRGB/systeminfo`.
#[cfg(windows)]
pub fn machine_info() -> Option<huedeck_hal::SystemInfo> {
    huedeck_hal_windows::system::system_info()
}

/// Not available on this OS yet.
#[cfg(not(windows))]
pub fn machine_info() -> Option<huedeck_hal::SystemInfo> {
    None
}

/// This process's resident set (working set on Windows), if known.
pub fn rss_bytes() -> Option<u64> {
    #[cfg(windows)]
    {
        huedeck_hal_windows::system::working_set_bytes()
    }
    #[cfg(not(windows))]
    {
        None
    }
}
