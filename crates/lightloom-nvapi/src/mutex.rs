// SPDX-License-Identifier: GPL-3.0-or-later
//! `Global\Access_SMBUS.HTP.Method` on the NvAPI worker thread: the named
//! mutex SMBus users on Windows take around a transfer (SignalRGB, OpenRGB,
//! HWiNFO, LibreHardwareMonitor, Lightloom's I/O broker).
//!
//! The same rules as the broker's copy (`lightloom_iobroker::mutex`): created
//! with a DACL that lets everyone use it (`D:(A;;0x001F0001;;;WD)`), opened
//! with just SYNCHRONIZE | MUTEX_MODIFY_STATE when another program created it
//! first, bounded waits, an abandoned mutex counts as taken. Kept separate so
//! the user process does not link the broker. A Win32 mutex belongs to the
//! thread that took it: the worker thread creates, takes and releases it.

use crate::hub::{GlobalLock, BUSY};
use crate::NvapiError;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, HANDLE, HLOCAL, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenMutexW, ReleaseMutex, WaitForSingleObject, MUTEX_MODIFY_STATE,
    SYNCHRONIZATION_SYNCHRONIZE,
};

/// The mutex name.
pub const SMBUS_MUTEX_NAME: &str = r"Global\Access_SMBUS.HTP.Method";
/// DACL when Lightloom creates it: MUTEX_ALL_ACCESS for Everyone.
const SMBUS_MUTEX_SDDL: &str = "D:(A;;0x001F0001;;;WD)";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// The opened (or created) global SMBus mutex.
#[derive(Debug)]
pub struct SmbusMutex {
    handle: HANDLE,
    held: bool,
}

impl SmbusMutex {
    /// Create it with the open DACL, or open the one another program created.
    pub fn create() -> Result<Self, String> {
        let name = wide(SMBUS_MUTEX_NAME);
        let sddl = wide(SMBUS_MUTEX_SDDL);
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: NUL-terminated SDDL string; `sd` receives a LocalAlloc'ed
        // descriptor that is freed below.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
        }
        .map_err(|e| format!("mutex DACL: {e}"))?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        // SAFETY: `sa` and the descriptor it points to stay alive for the
        // call; NUL-terminated name. Not taken on creation.
        let created = unsafe { CreateMutexW(Some(&sa), false, PCWSTR(name.as_ptr())) };
        // SAFETY: `sd` came from ConvertStringSecurityDescriptorToSecurityDescriptorW
        // and is freed exactly once, after its last use.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(sd.0)));
        }
        let handle = match created {
            Ok(h) => h,
            // SAFETY: NUL-terminated name; plain open of a named object.
            Err(_) => unsafe {
                OpenMutexW(
                    SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
                    false,
                    PCWSTR(name.as_ptr()),
                )
            }
            .map_err(|e| format!("{SMBUS_MUTEX_NAME}: {e}"))?,
        };
        Ok(Self { handle, held: false })
    }
}

impl GlobalLock for SmbusMutex {
    fn acquire(&mut self, timeout_ms: u32) -> Result<(), NvapiError> {
        if self.held {
            return Ok(());
        }
        // SAFETY: valid mutex handle owned by self.
        let r = unsafe { WaitForSingleObject(self.handle, timeout_ms) };
        if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
            self.held = true;
            Ok(())
        } else if r == WAIT_TIMEOUT {
            Err(NvapiError::Status(BUSY))
        } else {
            Err(NvapiError::GlobalMutex(format!(
                "waiting for {SMBUS_MUTEX_NAME} failed ({})",
                r.0
            )))
        }
    }

    fn release(&mut self) {
        if self.held {
            self.held = false;
            // SAFETY: this thread owns the mutex (taken above, on the same thread).
            if let Err(e) = unsafe { ReleaseMutex(self.handle) } {
                tracing::error!("ReleaseMutex({SMBUS_MUTEX_NAME}): {e}");
            }
        }
    }
}

impl Drop for SmbusMutex {
    fn drop(&mut self) {
        self.release();
        // SAFETY: the handle was returned by CreateMutexW/OpenMutexW and is closed once.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
