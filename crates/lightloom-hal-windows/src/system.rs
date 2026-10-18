// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Process-level queries.
//!
//! * [`running_process_names`]: executable names of every running process
//!   (Toolhelp32 snapshot). Hosts compare them with a plugin's
//!   `ConflictingProcesses()` and with known RGB suites to *report* conflicts;
//!   Lightloom never terminates another application.
//! * [`working_set_bytes`]: working set (resident memory) of this process,
//!   for the RSS figures of `lightloom rainbow` / `lightloom bench`.

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
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

#[cfg(test)]
mod tests {
    use super::*;

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
