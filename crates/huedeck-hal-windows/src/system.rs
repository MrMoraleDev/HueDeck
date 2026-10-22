// SPDX-License-Identifier: GPL-3.0-or-later
//! Process-level queries.
//!
//! * [`running_process_names`]: executable names of every running process
//!   (Toolhelp32 snapshot). Hosts compare them with a plugin's
//!   `ConflictingProcesses()` and with known RGB suites to *report* conflicts;
//!   HueDeck never terminates another application.
//! * [`working_set_bytes`]: working set (resident memory) of this process,
//!   for the RSS figures of `huedeck rainbow` / `huedeck bench`.
//! * [`system_info`]: motherboard, BIOS and memory modules from the SMBIOS
//!   table (what `@SignalRGB/systeminfo` reports; SignalRGB reads WMI, which
//!   reads the same table).
//! * [`tcp_listener_pids`]: processes listening on a TCP port (the daemon
//!   leaves SignalRGB's API ports alone while SignalRGB listens on them).

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, TCP_TABLE_OWNER_PID_LISTENER};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::SystemInformation::{GetSystemFirmwareTable, RSMB};
use windows::Win32::System::Threading::GetCurrentProcess;

/// Executable file names (`SignalRgb.exe`, …) of all running processes, in
/// snapshot order. Duplicates are kept (one entry per process).
pub fn running_process_names() -> Result<Vec<String>, String> {
    // SAFETY: plain snapshot request; the handle is closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|e| format!("CreateToolhelp32Snapshot: {}", e.message()))?;
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut names = Vec::new();
    // SAFETY: valid snapshot handle; `entry.dwSize` is set as the API requires.
    let first = unsafe { Process32FirstW(snapshot, &mut entry) };
    if let Err(e) = &first {
        // SAFETY: handle from CreateToolhelp32Snapshot, closed exactly once.
        let _ = unsafe { CloseHandle(snapshot) };
        // Fail closed: an empty list would hide running conflicting software.
        return Err(format!("Process32FirstW: {}", e.message()));
    }
    let mut more = true;
    while more {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        names.push(String::from_utf16_lossy(&entry.szExeFile[..len]));
        // SAFETY: same handle and entry as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    // SAFETY: handle from CreateToolhelp32Snapshot, closed exactly once.
    let _ = unsafe { CloseHandle(snapshot) };
    Ok(names)
}

/// Working set of the current process in bytes, `None` if the query fails.
pub fn working_set_bytes() -> Option<u64> {
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: pseudo-handle of this process; `counters` is writable and `cb` matches its size.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    ok.as_bool().then_some(counters.WorkingSetSize as u64)
}

/// The machine's SMBIOS facts, `None` when the firmware table is unavailable.
pub fn system_info() -> Option<huedeck_hal::SystemInfo> {
    // SAFETY: a size query without a buffer.
    let size = unsafe { GetSystemFirmwareTable(RSMB, 0, None) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: the slice describes `buf`, `size` bytes long.
    let written = unsafe { GetSystemFirmwareTable(RSMB, 0, Some(&mut buf)) };
    if written == 0 || written > size {
        return None;
    }
    buf.truncate(written as usize);
    huedeck_hal::smbios::parse_raw_smbios_data(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_table_parses() {
        // Every Windows machine (VMs included) exposes an SMBIOS table.
        let info = system_info().expect("SMBIOS table");
        assert!(
            !info.bios.version.is_empty() || !info.motherboard.model.is_empty(),
            "{info:?}"
        );
    }

    #[test]
    fn snapshot_contains_this_process() {
        let names = running_process_names().expect("snapshot");
        let exe = std::env::current_exe().unwrap();
        let me = exe.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            names.iter().any(|n| n.eq_ignore_ascii_case(&me)),
            "{me} not in {} names",
            names.len()
        );
    }

    #[test]
    fn working_set_is_reported() {
        let ws = working_set_bytes().expect("working set");
        assert!(ws > 1024 * 1024, "{ws}");
    }
}

/// Process ids listening on TCP `port` on any local address, IPv4 and IPv6
/// (`GetExtendedTcpTable` listener tables), sorted and deduplicated.
pub fn tcp_listener_pids(port: u16) -> Result<Vec<u32>, String> {
    const AF_INET: u32 = 2;
    const AF_INET6: u32 = 23;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    let mut pids = Vec::new();
    // (family, row size, offset of the local port, offset of the owning pid):
    // MIB_TCPROW_OWNER_PID and MIB_TCP6ROW_OWNER_PID after the u32 entry count.
    for (family, row, port_at, pid_at) in [(AF_INET, 24usize, 8usize, 20usize), (AF_INET6, 56, 20, 52)] {
        let mut words: Vec<u32> = Vec::new();
        let mut attempts = 0;
        loop {
            let mut size = (words.len() * 4) as u32;
            let ptr = if words.is_empty() {
                None
            } else {
                Some(words.as_mut_ptr().cast())
            };
            // SAFETY: `ptr` (when given) points to `size` writable, u32-aligned bytes.
            let err = unsafe {
                GetExtendedTcpTable(ptr, &mut size, false, family, TCP_TABLE_OWNER_PID_LISTENER, 0)
            };
            if err == 0 && !words.is_empty() {
                break;
            }
            attempts += 1;
            if (err != ERROR_INSUFFICIENT_BUFFER && err != 0) || attempts > 4 {
                return Err(format!("GetExtendedTcpTable: error {err}"));
            }
            // The table may grow between the size query and the read: leave room.
            words = vec![0u32; (size as usize).div_ceil(4) + 64];
        }
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_ne_bytes()).collect();
        let count = words[0] as usize;
        for i in 0..count {
            let off = 4 + i * row;
            let Some(r) = bytes.get(off..off + row) else {
                break;
            };
            let word = |o: usize| u32::from_ne_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
            // The port sits in network byte order in the low 16 bits.
            if u16::from_be((word(port_at) & 0xFFFF) as u16) == port {
                pids.push(word(pid_at));
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

#[cfg(test)]
mod listener_tests {
    use super::*;

    #[test]
    fn a_listener_of_this_process_is_found_on_its_port() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(tcp_listener_pids(port).unwrap().contains(&std::process::id()));
        drop(l);
        assert!(!tcp_listener_pids(port).unwrap().contains(&std::process::id()));
    }
}
