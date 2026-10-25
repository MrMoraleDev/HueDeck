// SPDX-License-Identifier: GPL-3.0-or-later
//! System-wide CPU time and memory from `/proc/stat` and `/proc/meminfo`
//! (the `CPU Load` / `RAM Load` sensors). Pure reads through [`Files`].

use crate::sysfs::Files;
use std::path::Path;

/// `(idle, total)` jiffies of the aggregate `cpu` line of `/proc/stat`
/// (idle includes iowait).
pub fn parse_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest is counted in user).
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((idle, total))
}

/// `(total, available)` bytes from `/proc/meminfo` (`MemAvailable`, else
/// `MemFree`).
pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let kb = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
            .and_then(|r| r.split_whitespace().next()?.parse::<u64>().ok())
            .map(|k| k * 1024)
    };
    let total = kb("MemTotal")?;
    Some((total, kb("MemAvailable").or_else(|| kb("MemFree"))?))
}

/// [`parse_stat`] of `/proc/stat`.
pub fn cpu_times(fs: &impl Files) -> Option<(u64, u64)> {
    parse_stat(&String::from_utf8_lossy(&fs.read(Path::new("/proc/stat"))?))
}

/// [`parse_meminfo`] of `/proc/meminfo`.
pub fn memory_status(fs: &impl Files) -> Option<(u64, u64)> {
    parse_meminfo(&String::from_utf8_lossy(&fs.read(Path::new("/proc/meminfo"))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_and_meminfo_parse() {
        let stat = "cpu  100 5 50 800 20 3 2 0 0 0\ncpu0 50 2 25 400 10 1 1 0 0 0\n";
        assert_eq!(parse_stat(stat), Some((820, 980)));
        assert_eq!(parse_stat("intr 1 2\n"), None);
        let mem = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    8000000 kB\n";
        assert_eq!(parse_meminfo(mem), Some((16_000_000 * 1024, 8_000_000 * 1024)));
        assert_eq!(
            parse_meminfo("MemTotal: 2 kB\nMemFree: 1 kB\n"),
            Some((2048, 1024))
        );
    }
}
