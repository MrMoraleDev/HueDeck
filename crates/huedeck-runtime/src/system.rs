// SPDX-License-Identifier: GPL-3.0-or-later
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

/// Other processes listening on TCP `port` (any address); empty where the OS
/// query is not implemented yet.
pub fn other_listeners(port: u16) -> Vec<u32> {
    #[cfg(windows)]
    {
        let me = std::process::id();
        huedeck_hal_windows::system::tcp_listener_pids(port)
            .map(|pids| pids.into_iter().filter(|p| *p != me).collect())
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        Vec::new()
    }
}
