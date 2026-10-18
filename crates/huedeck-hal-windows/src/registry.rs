// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Read-only access to string values under `HKEY_CURRENT_USER`: importing
//! another application's per-user settings (SignalRGB keeps its component
//! assignments there). HueDeck never writes the registry.

use std::io;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    REG_EXPAND_SZ, REG_SZ,
};

/// Values larger than this are skipped (a channel array is a few KB).
const MAX_VALUE_BYTES: u32 = 4 << 20;

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by RegOpenKeyExW and is closed once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn os_error(err: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(err.0 as i32)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The string (`REG_SZ` / `REG_EXPAND_SZ`, unexpanded) values directly under
/// `HKCU\<subkey>`, as `(name, value)` in enumeration order. A missing key is
/// an empty list; other value types and oversized values are skipped.
pub fn current_user_strings(subkey: &str) -> io::Result<Vec<(String, String)>> {
    let wkey = wide(subkey);
    let mut hkey = HKEY::default();
    // SAFETY: NUL-terminated key name that outlives the call; valid out pointer.
    let err = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wkey.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        )
    };
    if err == ERROR_FILE_NOT_FOUND {
        return Ok(Vec::new());
    }
    if err != ERROR_SUCCESS {
        return Err(os_error(err));
    }
    let key = Key(hkey);
    let (mut count, mut max_name, mut max_data) = (0u32, 0u32, 0u32);
    // SAFETY: the key is open; every out pointer is valid for the call.
    let err = unsafe {
        RegQueryInfoKeyW(
            key.0,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&mut count),
            Some(&mut max_name),
            Some(&mut max_data),
            None,
            None,
        )
    };
    if err != ERROR_SUCCESS {
        return Err(os_error(err));
    }
    let mut name = vec![0u16; max_name as usize + 1];
    let mut data = vec![0u8; max_data.min(MAX_VALUE_BYTES) as usize + 2];
    let mut out = Vec::new();
    for index in 0..count {
        let mut name_len = name.len() as u32;
        let mut data_len = data.len() as u32;
        let mut kind = 0u32;
        // SAFETY: the buffers and their lengths (characters for the name, bytes
        // for the data) describe `name` and `data`; out pointers are valid.
        let err = unsafe {
            RegEnumValueW(
                key.0,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr()),
                Some(&mut data_len),
            )
        };
        if err == ERROR_NO_MORE_ITEMS {
            break;
        }
        // ERROR_MORE_DATA (a value grew or is oversized) and friends: skip the value.
        if err != ERROR_SUCCESS || (kind != REG_SZ.0 && kind != REG_EXPAND_SZ.0) {
            continue;
        }
        let units: Vec<u16> = data[..(data_len as usize).min(data.len())]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        out.push((
            String::from_utf16_lossy(&name[..(name_len as usize).min(name.len())]),
            text.trim_end_matches('\0').to_string(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_well_known_key_and_tolerates_missing_ones() {
        assert!(current_user_strings(r"Software\HueDeck\definitely-not-here")
            .unwrap()
            .is_empty());
        // Every interactive user profile has the console settings key.
        let values = current_user_strings("Console").unwrap();
        assert!(values.iter().all(|(n, _)| !n.contains('\0')));
    }
}
