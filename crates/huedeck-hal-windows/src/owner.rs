// SPDX-License-Identifier: GPL-3.0-or-later
//! One HueDeck process at a time drives SMBus and GPU I2C.
//!
//! `huedeckd` (with SMBus on) and a `huedeck` run on the buses (SMBus is on
//! unless `--no-smbus`) both take
//! [`SmbusOwner`] before they touch a bus: the named mutex
//! `Global\HueDeck.SmbusOwner`, held for as long as they drive SMBus devices.
//! The second process finds it taken and leaves the buses alone, whichever
//! started first.
//! A process that dies without letting go leaves the mutex abandoned, which
//! the next one takes as free.
//!
//! A Win32 mutex belongs to the thread that took it, and the owners of this
//! one (the daemon's engine, the CLI's main thread) are not guaranteed to
//! drop it on the same thread: a small thread of its own takes it, waits for
//! the [`SmbusOwner`] to be dropped, then releases it.

use std::sync::mpsc;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenMutexW, ReleaseMutex, WaitForSingleObject, MUTEX_MODIFY_STATE,
    SYNCHRONIZATION_SYNCHRONIZE,
};

/// The mutex name (every session, every user: the buses are machine-wide).
pub const SMBUS_OWNER_MUTEX: &str = r"Global\HueDeck.SmbusOwner";
/// Its DACL when this process creates it: MUTEX_ALL_ACCESS for Everyone.
const OWNER_SDDL: &str = "D:(A;;0x001F0001;;;WD)";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Held while this process drives SMBus; dropping it lets go.
#[derive(Debug)]
pub struct SmbusOwner {
    release: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SmbusOwner {
    /// Take the owner mutex without waiting. `Ok(None)` when another HueDeck
    /// process holds it.
    pub fn try_acquire() -> Result<Option<Self>, String> {
        let (taken_tx, taken_rx) = mpsc::channel::<Result<bool, String>>();
        let (release, release_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("huedeck-smbus-owner".into())
            .spawn(move || {
                let handle = match open() {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = taken_tx.send(Err(e));
                        return;
                    }
                };
                // SAFETY: valid mutex handle, no wait.
                let r = unsafe { WaitForSingleObject(handle, 0) };
                let taken = r == WAIT_OBJECT_0 || r == WAIT_ABANDONED;
                let _ = taken_tx.send(Ok(taken));
                if taken {
                    // Until the owner is dropped (or the process ends).
                    let _ = release_rx.recv();
                    // SAFETY: this thread took the mutex above.
                    let _ = unsafe { ReleaseMutex(handle) };
                }
                // SAFETY: handle from CreateMutexW/OpenMutexW, closed once.
                let _ = unsafe { CloseHandle(handle) };
            })
            .map_err(|e| format!("owner thread: {e}"))?;
        match taken_rx.recv() {
            Ok(Ok(true)) => Ok(Some(Self {
                release: Some(release),
                thread: Some(thread),
            })),
            Ok(Ok(false)) => {
                drop(release);
                let _ = thread.join();
                Ok(None)
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("owner thread ended early".into()),
        }
    }
}

impl Drop for SmbusOwner {
    fn drop(&mut self) {
        drop(self.release.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Create the mutex with the open DACL, or open the existing one.
fn open() -> Result<HANDLE, String> {
    let name = wide(SMBUS_OWNER_MUTEX);
    let sddl = wide(OWNER_SDDL);
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
    // SAFETY: `sa` and its descriptor outlive the call; NUL-terminated name.
    let created = unsafe { CreateMutexW(Some(&sa), false, PCWSTR(name.as_ptr())) };
    // SAFETY: `sd` came from ConvertStringSecurityDescriptorToSecurityDescriptorW, freed once.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sd.0)));
    }
    match created {
        Ok(h) => Ok(h),
        // SAFETY: NUL-terminated name; plain open of a named object.
        Err(_) => unsafe {
            OpenMutexW(
                SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
                false,
                PCWSTR(name.as_ptr()),
            )
        }
        .map_err(|e| format!("{SMBUS_OWNER_MUTEX}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_owner_at_a_time() {
        let first = SmbusOwner::try_acquire().unwrap();
        if first.is_none() {
            // A HueDeck process on this machine drives SMBus right now.
            return;
        }
        assert!(SmbusOwner::try_acquire().unwrap().is_none(), "taken: refused");
        drop(first);
        let again = SmbusOwner::try_acquire().unwrap();
        assert!(again.is_some(), "free once dropped");
    }
}
