// SPDX-License-Identifier: GPL-3.0-or-later
//! Start a program elevated, through the UAC prompt (`ShellExecuteExW` with
//! the `runas` verb), and wait for it. Used only on an explicit user action
//! (the dashboard's PawnIO / RAM lighting buttons, the installer's checkbox):
//! Windows always shows the prompt and the user may decline it
//! ([`crate::pawnio::ERROR_CANCELLED`]). HueDeck never elevates silently.

use std::io;
use std::path::Path;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, WaitForSingleObject, INFINITE,
};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// A process started through the UAC prompt.
#[derive(Debug)]
pub struct Elevated {
    /// The process handle (as an integer: `HANDLE` is not `Send`).
    process: isize,
}

impl Elevated {
    /// Wait for the process to end; its exit code.
    pub fn wait(self) -> io::Result<u32> {
        let h = HANDLE(self.process as *mut core::ffi::c_void);
        // SAFETY: a process handle owned by `self`, open until `drop`.
        let waited = unsafe { WaitForSingleObject(h, INFINITE) };
        if waited != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        let mut code = 0u32;
        // SAFETY: valid handle of an ended process; valid out pointer.
        unsafe { GetExitCodeProcess(h, &mut code) }.map_err(|e| io::Error::from_raw_os_error(e.code().0))?;
        Ok(code)
    }
}

impl Drop for Elevated {
    fn drop(&mut self) {
        // SAFETY: the handle came from ShellExecuteExW (SEE_MASK_NOCLOSEPROCESS) and is closed once.
        unsafe {
            let _ = CloseHandle(HANDLE(self.process as *mut core::ffi::c_void));
        }
    }
}

/// Start `exe` with `parameters` (one command line, see
/// [`crate::pawnio::command_line`]) through the UAC prompt, in its own
/// console window. Declining the prompt is an error with raw OS code
/// [`crate::pawnio::ERROR_CANCELLED`].
pub fn run_elevated(exe: &Path, parameters: &str) -> io::Result<Elevated> {
    let file = wide(exe.as_os_str());
    let params = wide(std::ffi::OsStr::new(parameters));
    let dir = exe.parent().map(|d| wide(d.as_os_str()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        lpDirectory: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size; the strings are
    // NUL-terminated and outlive the call.
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;
    if info.hProcess.is_invalid() {
        return Err(io::Error::other("the elevated process handle is missing"));
    }
    Ok(Elevated {
        process: info.hProcess.0 as isize,
    })
}

/// Whether this process runs elevated (an administrator's full token).
pub fn is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo handle of this process; valid out pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: valid token; the buffer is a TOKEN_ELEVATION of the size passed.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    }
    .is_ok();
    // SAFETY: opened above, closed once.
    let _ = unsafe { CloseHandle(token) };
    ok && elevation.TokenIsElevated != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevation_is_readable() {
        // Whatever this test runs as, the call answers without prompting.
        let _ = is_elevated();
    }
}
