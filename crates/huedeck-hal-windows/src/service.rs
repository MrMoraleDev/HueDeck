// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only service state (is the HueDeck I/O broker registered, running?):
//! `SC_MANAGER_CONNECT` and `SERVICE_QUERY_STATUS`, which every user has.

use crate::pawnio::ServiceState;
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SERVICE_DOES_NOT_EXIST;
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_HANDLE, SC_MANAGER_CONNECT,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS, SERVICE_STOPPED,
};

struct Handle(SC_HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: opened by OpenSCManagerW / OpenServiceW, closed once.
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

/// The state of service `name`.
pub fn service_state(name: &str) -> ServiceState {
    let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: local machine, default database; no strings passed.
    let Ok(scm) = (unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }) else {
        return ServiceState::Unknown;
    };
    let scm = Handle(scm);
    // SAFETY: valid manager handle; NUL-terminated name that outlives the call.
    let service = match unsafe { OpenServiceW(scm.0, PCWSTR(wname.as_ptr()), SERVICE_QUERY_STATUS) } {
        Ok(s) => Handle(s),
        Err(e) if e.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() => return ServiceState::NotInstalled,
        Err(_) => return ServiceState::Unknown,
    };
    let mut status = SERVICE_STATUS::default();
    // SAFETY: valid service handle; valid out pointer.
    if unsafe { QueryServiceStatus(service.0, &mut status) }.is_err() {
        return ServiceState::Unknown;
    }
    match status.dwCurrentState {
        SERVICE_RUNNING => ServiceState::Running,
        SERVICE_STOPPED => ServiceState::Stopped,
        _ => ServiceState::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_services_are_not_installed() {
        assert_eq!(
            service_state("HueDeckNoSuchService-7f3a"),
            ServiceState::NotInstalled
        );
    }
}
