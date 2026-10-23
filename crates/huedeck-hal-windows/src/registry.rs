// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only access to registry values: under `HKEY_CURRENT_USER`, importing
//! another application's per-user settings (SignalRGB keeps its component
//! assignments and canvas placements there); under `HKEY_LOCAL_MACHINE`,
//! whether a driver is installed (PawnIO's uninstall entry). Nothing here
//! writes the registry (the one key HueDeck writes, its notification sender,
//! is in [`crate::toast`]).

use std::io;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_EXPAND_SZ, REG_QWORD, REG_SZ,
};

/// Values larger than this are skipped (a channel array is a few KB).
const MAX_VALUE_BYTES: u32 = 4 << 20;

/// A registry value HueDeck can import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegValue {
    /// `REG_SZ` / `REG_EXPAND_SZ` (unexpanded).
    Text(String),
    /// `REG_DWORD`.
    Dword(u32),
    /// `REG_QWORD`.
    Qword(u64),
}

impl RegValue {
    /// The value as text (numbers in decimal).
    pub fn as_text(&self) -> String {
        match self {
            RegValue::Text(s) => s.clone(),
            RegValue::Dword(v) => v.to_string(),
            RegValue::Qword(v) => v.to_string(),
        }
    }
}

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

/// `HKCU\<subkey>` opened for reading; `None` when it does not exist.
fn open(subkey: &str) -> io::Result<Option<Key>> {
    open_in(HKEY_CURRENT_USER, subkey)
}

/// `<root>\<subkey>` opened for reading; `None` when it does not exist.
fn open_in(root: HKEY, subkey: &str) -> io::Result<Option<Key>> {
    let wkey = wide(subkey);
    let mut hkey = HKEY::default();
    // SAFETY: NUL-terminated key name that outlives the call; valid out pointer.
    let err = unsafe { RegOpenKeyExW(root, PCWSTR(wkey.as_ptr()), None, KEY_READ, &mut hkey) };
    if err == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if err != ERROR_SUCCESS {
        return Err(os_error(err));
    }
    Ok(Some(Key(hkey)))
}

/// Subkey count, longest subkey name, value count, longest value name, largest value.
fn info(key: &Key) -> io::Result<(u32, u32, u32, u32, u32)> {
    let (mut subkeys, mut max_subkey) = (0u32, 0u32);
    let (mut count, mut max_name, mut max_data) = (0u32, 0u32, 0u32);
    // SAFETY: the key is open; every out pointer is valid for the call.
    let err = unsafe {
        RegQueryInfoKeyW(
            key.0,
            None,
            None,
            None,
            Some(&mut subkeys),
            Some(&mut max_subkey),
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
    Ok((subkeys, max_subkey, count, max_name, max_data))
}

/// The values directly under `HKCU\<subkey>` that HueDeck can import (strings,
/// DWORDs, QWORDs), as `(name, value)` in enumeration order. A missing key is
/// an empty list; other value types and oversized values are skipped.
pub fn current_user_values(subkey: &str) -> io::Result<Vec<(String, RegValue)>> {
    let Some(key) = open(subkey)? else {
        return Ok(Vec::new());
    };
    values(&key)
}

/// The string values directly under `HKLM\<subkey>` (read-only), as
/// [`current_user_strings`] does for `HKCU`.
pub fn local_machine_strings(subkey: &str) -> io::Result<Vec<(String, String)>> {
    let Some(key) = open_in(HKEY_LOCAL_MACHINE, subkey)? else {
        return Ok(Vec::new());
    };
    Ok(strings(values(&key)?))
}

fn strings(values: Vec<(String, RegValue)>) -> Vec<(String, String)> {
    values
        .into_iter()
        .filter_map(|(name, value)| match value {
            RegValue::Text(text) => Some((name, text)),
            _ => None,
        })
        .collect()
}

fn values(key: &Key) -> io::Result<Vec<(String, RegValue)>> {
    let (_, _, count, max_name, max_data) = info(key)?;
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
        if err != ERROR_SUCCESS {
            continue;
        }
        let bytes = &data[..(data_len as usize).min(data.len())];
        let value = if kind == REG_SZ.0 || kind == REG_EXPAND_SZ.0 {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            RegValue::Text(
                String::from_utf16_lossy(&units)
                    .trim_end_matches('\0')
                    .to_string(),
            )
        } else if kind == REG_DWORD.0 && bytes.len() >= 4 {
            RegValue::Dword(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        } else if kind == REG_QWORD.0 && bytes.len() >= 8 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&bytes[..8]);
            RegValue::Qword(u64::from_le_bytes(b))
        } else {
            continue;
        };
        out.push((
            String::from_utf16_lossy(&name[..(name_len as usize).min(name.len())]),
            value,
        ));
    }
    Ok(out)
}

/// The string (`REG_SZ` / `REG_EXPAND_SZ`, unexpanded) values directly under
/// `HKCU\<subkey>`, as `(name, value)` in enumeration order. A missing key is
/// an empty list; other value types and oversized values are skipped.
pub fn current_user_strings(subkey: &str) -> io::Result<Vec<(String, String)>> {
    Ok(strings(current_user_values(subkey)?))
}

/// The names of the keys directly under `HKCU\<subkey>`, in enumeration order.
/// A missing key is an empty list.
pub fn current_user_subkeys(subkey: &str) -> io::Result<Vec<String>> {
    let Some(key) = open(subkey)? else {
        return Ok(Vec::new());
    };
    let (count, max_subkey, ..) = info(&key)?;
    let mut name = vec![0u16; max_subkey as usize + 1];
    let mut out = Vec::new();
    for index in 0..count {
        let mut name_len = name.len() as u32;
        // SAFETY: `name` holds `name_len` characters; the other out pointers are unused.
        let err = unsafe {
            RegEnumKeyExW(
                key.0,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if err == ERROR_NO_MORE_ITEMS {
            break;
        }
        if err != ERROR_SUCCESS {
            continue;
        }
        out.push(String::from_utf16_lossy(
            &name[..(name_len as usize).min(name.len())],
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
        assert!(current_user_subkeys(r"Software\HueDeck\definitely-not-here")
            .unwrap()
            .is_empty());
        // Every interactive user profile has the console settings key, with DWORD values.
        let values = current_user_values("Console").unwrap();
        assert!(values.iter().all(|(n, _)| !n.contains('\0')));
        assert!(values.iter().any(|(_, v)| matches!(v, RegValue::Dword(_))));
        assert!(!current_user_subkeys("Software").unwrap().is_empty());
    }

    #[test]
    fn reads_local_machine_strings() {
        assert!(local_machine_strings(r"SOFTWARE\HueDeck\definitely-not-here")
            .unwrap()
            .is_empty());
        let nt = local_machine_strings(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").unwrap();
        assert!(nt.iter().any(|(n, _)| n == "ProductName"), "{nt:?}");
    }
}
