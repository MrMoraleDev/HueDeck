// SPDX-License-Identifier: GPL-3.0-or-later
//! `Global\Access_SMBUS.HTP.Method`: the named mutex every SMBus user on
//! Windows takes around a transfer (SignalRGB, OpenRGB, HWiNFO, SIV, LHM).
//! PawnIO serialises calls per handle only, so this mutex is what keeps two
//! programs' transfers from interleaving on the bus (research §1.11).
//!
//! Created with an explicit DACL that lets everyone use it
//! (`D:(A;;0x001F0001;;;WD)`, as LibreHardwareMonitor does): a mutex created
//! by SYSTEM with the default DACL would lock non-elevated programs out of
//! it. Opened instead when another program created it first. If it can be
//! neither created nor opened the broker serves no SMBus traffic at all
//! (fail closed). Waits are bounded; an abandoned mutex counts as acquired.
//!
//! Win32 mutex ownership is per thread: the broker takes and releases it on
//! its single worker thread only.

use huedeck_protocol::broker::BrokerError;

/// The mutex name (literal string in SignalRgb.exe, the PawnIO module docs, LHM).
pub const SMBUS_MUTEX_NAME: &str = r"Global\Access_SMBUS.HTP.Method";
/// DACL of the mutex when the broker creates it: MUTEX_ALL_ACCESS for Everyone.
pub const SMBUS_MUTEX_SDDL: &str = "D:(A;;0x001F0001;;;WD)";
/// Longest wait for the mutex per operation or lease.
pub const ACQUIRE_TIMEOUT_MS: u32 = 50;

/// The global SMBus mutex, or a test double.
pub trait GlobalMutex {
    /// Take it, waiting at most `timeout_ms`; `BusContended` when another
    /// program kept it.
    fn acquire(&mut self, timeout_ms: u32) -> Result<(), BrokerError>;
    /// Release it if held.
    fn release(&mut self);
}

#[cfg(windows)]
pub use win::SmbusMutex;

#[cfg(windows)]
mod win {
    use super::*;
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
                // Created first by a program that denies MUTEX_ALL_ACCESS: ask
                // only for what waiting and releasing need.
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

    impl GlobalMutex for SmbusMutex {
        fn acquire(&mut self, timeout_ms: u32) -> Result<(), BrokerError> {
            if self.held {
                return Ok(());
            }
            // SAFETY: valid mutex handle owned by self.
            let r = unsafe { WaitForSingleObject(self.handle, timeout_ms) };
            if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
                self.held = true;
                Ok(())
            } else if r == WAIT_TIMEOUT {
                Err(BrokerError::BusContended)
            } else {
                Err(BrokerError::Backend {
                    status: -1,
                    message: format!("waiting for {SMBUS_MUTEX_NAME} failed ({})", r.0),
                })
            }
        }

        fn release(&mut self) {
            if self.held {
                self.held = false;
                // SAFETY: this thread owns the mutex (acquired above, same thread).
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
}
