// SPDX-License-Identifier: GPL-3.0-or-later
//! Machine facts and this process's memory.

/// Motherboard / BIOS / memory facts for `@SignalRGB/systeminfo`.
#[cfg(windows)]
pub fn machine_info() -> Option<lightloom_hal::SystemInfo> {
    lightloom_hal_windows::system::system_info()
}

/// Not available on this OS yet.
#[cfg(not(windows))]
pub fn machine_info() -> Option<lightloom_hal::SystemInfo> {
    None
}

/// This process's resident set (working set on Windows), if known.
pub fn rss_bytes() -> Option<u64> {
    #[cfg(windows)]
    {
        lightloom_hal_windows::system::working_set_bytes()
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
        lightloom_hal_windows::system::tcp_listener_pids(port)
            .map(|pids| pids.into_iter().filter(|p| *p != me).collect())
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        Vec::new()
    }
}

/// The operating system and its build, for device reports:
/// `Windows 11 Pro 25H2 (build 26200.6899)`, `Ubuntu 24.04.1 LTS (kernel 6.8.0-45-generic)`.
pub fn os_description() -> String {
    #[cfg(windows)]
    {
        use lightloom_hal_windows::registry::{local_machine_values, RegValue};
        // Read-only: the version facts every Windows keeps here.
        let values =
            local_machine_values(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").unwrap_or_default();
        let value = |name: &str| {
            values
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
        };
        let text = |name: &str| match value(name) {
            Some(RegValue::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let ubr = match value("UBR") {
            Some(RegValue::Dword(d)) => Some(*d),
            _ => None,
        };
        windows_description(
            &text("ProductName"),
            &text("DisplayVersion"),
            &text("CurrentBuild"),
            ubr,
        )
    }
    #[cfg(target_os = "linux")]
    {
        let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        linux_description(&release, kernel.trim())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        std::env::consts::OS.to_string()
    }
}

/// `ProductName` still says "Windows 10" on Windows 11 (build 22000 and later).
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_description(product: &str, display: &str, build: &str, ubr: Option<u32>) -> String {
    let mut name = if product.is_empty() {
        "Windows".to_string()
    } else {
        product.to_string()
    };
    if build.parse::<u32>().is_ok_and(|b| b >= 22000) {
        name = name.replacen("Windows 10", "Windows 11", 1);
    }
    let mut out = name;
    if !display.is_empty() {
        out.push(' ');
        out.push_str(display);
    }
    if !build.is_empty() {
        match ubr {
            Some(u) => out.push_str(&format!(" (build {build}.{u})")),
            None => out.push_str(&format!(" (build {build})")),
        }
    }
    out
}

/// `PRETTY_NAME` of `/etc/os-release` and the kernel release.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_description(os_release: &str, kernel: &str) -> String {
    let pretty = os_release
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "Linux".to_string());
    if kernel.is_empty() {
        pretty
    } else {
        format!("{pretty} (kernel {kernel})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_is_named_from_the_build() {
        assert_eq!(
            windows_description("Windows 10 Pro", "25H2", "26200", Some(6899)),
            "Windows 11 Pro 25H2 (build 26200.6899)"
        );
        assert_eq!(
            windows_description("Windows 10 Home", "22H2", "19045", None),
            "Windows 10 Home 22H2 (build 19045)"
        );
        assert_eq!(windows_description("", "", "", None), "Windows");
    }

    #[test]
    fn linux_pretty_name_and_kernel() {
        let release = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(
            linux_description(release, "6.8.0-45-generic"),
            "Ubuntu 24.04.1 LTS (kernel 6.8.0-45-generic)"
        );
        assert_eq!(linux_description("", ""), "Linux");
    }
}
