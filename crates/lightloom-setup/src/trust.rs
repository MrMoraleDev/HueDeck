// SPDX-License-Identifier: GPL-3.0-or-later
//! Authenticode check of a downloaded file: `WinVerifyTrust` (generic
//! verify v2, no UI, embedded signature) must accept it, and the signer
//! certificate's simple display name must be the pinned one. A defence in
//! depth: the SHA-256 pin already fixes the exact bytes.

use std::path::Path;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
use windows::Win32::Security::WinTrust::{
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
    WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

/// Whether `found` (the certificate's display name) is the `expected` signer.
pub fn signer_matches(found: &str, expected: &str) -> bool {
    found.trim().eq_ignore_ascii_case(expected)
}

/// Verify `path`'s embedded Authenticode signature and its signer's name.
pub fn check_signer(path: &Path, expected: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let wpath: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wpath.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` and `file` are initialised with their sizes; the path is
    // NUL-terminated; all outlive the call. The state is closed below.
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };
    let signer = if status == 0 {
        signer_name(data.hWVTStateData)
    } else {
        None
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: closes the state opened by the verify call above.
    unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }
    if status != 0 {
        return Err(format!(
            "{}: Authenticode signature not valid (WinVerifyTrust 0x{:08x})",
            path.display(),
            status as u32
        ));
    }
    match signer {
        Some(name) if signer_matches(&name, expected) => Ok(()),
        Some(name) => Err(format!(
            "{}: signed by {name:?}, not by {expected:?}",
            path.display()
        )),
        None => Err(format!("{}: the signer could not be read", path.display())),
    }
}

/// The simple display name (CN) of the first signer's certificate.
fn signer_name(state: HANDLE) -> Option<String> {
    // SAFETY: `state` comes from a successful WinVerifyTrust and is still open;
    // the returned structures live in it.
    unsafe {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
        if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
            return None;
        }
        let cert = (*(*signer).pasCertChain).pCert;
        if cert.is_null() {
            return None;
        }
        let mut buf = [0u16; 256];
        let n = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf)) as usize;
        (n > 1).then(|| String::from_utf16_lossy(&buf[..n - 1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signer_names_compare_exactly() {
        assert!(signer_matches("namazso.eu", "namazso.eu"));
        assert!(signer_matches(" NAMAZSO.EU ", "namazso.eu"));
        assert!(!signer_matches("namazso.eu.evil", "namazso.eu"));
        assert!(!signer_matches("evil namazso.eu", "namazso.eu"));
        assert!(!signer_matches("", "namazso.eu"));
    }

    #[test]
    fn unsigned_files_are_refused() {
        let dir = std::env::temp_dir().join(format!("lightloom-setup-trust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("unsigned.exe");
        std::fs::write(&path, b"MZ not a signed program").unwrap();
        let err = check_signer(&path, "namazso.eu").unwrap_err();
        assert!(err.contains("not valid"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
