// SPDX-License-Identifier: GPL-3.0-or-later
//! The per-user `Run` key in the registry (`HKEY_CURRENT_USER`).

use crate::{RunKey, RUN_SUBKEY};
use std::io;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
/// The values under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub struct WinRunKey;

fn os_error(err: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(err.0 as i32)
}

impl RunKey for WinRunKey {
    fn get(&self, name: &str) -> io::Result<Option<String>> {
        let (key, value) = (wide(RUN_SUBKEY), wide(name));
        let mut len = 0u32;
        // SAFETY: NUL-terminated names; a size query (no buffer).
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut len),
            )
        };
        if err == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if err != ERROR_SUCCESS {
            return Err(os_error(err));
        }
        let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: the buffer holds `len` bytes and outlives the call.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        if err == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if err != ERROR_SUCCESS {
            return Err(os_error(err));
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(Some(String::from_utf16_lossy(&buf[..end])))
    }

    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        let key = wide(RUN_SUBKEY);
        let mut hkey = HKEY::default();
        // SAFETY: NUL-terminated key name; valid out pointer.
        let err = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut hkey,
                None,
            )
        };
        if err != ERROR_SUCCESS {
            return Err(os_error(err));
        }
        let (name, data) = (wide(name), wide(value));
        let bytes: Vec<u8> = data.iter().flat_map(|u| u.to_le_bytes()).collect();
        // SAFETY: the key is open; name and data outlive the call; closed once.
        let err = unsafe {
            let err = RegSetValueExW(hkey, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&bytes));
            let _ = RegCloseKey(hkey);
            err
        };
        if err != ERROR_SUCCESS {
            return Err(os_error(err));
        }
        Ok(())
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        let key = wide(RUN_SUBKEY);
        let mut hkey = HKEY::default();
        // SAFETY: NUL-terminated key name; valid out pointer.
        let err = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                KEY_SET_VALUE,
                &mut hkey,
            )
        };
        if err == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        if err != ERROR_SUCCESS {
            return Err(os_error(err));
        }
        let name = wide(name);
        // SAFETY: the key is open; the name outlives the call; closed once.
        let err = unsafe {
            let err = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
            let _ = RegCloseKey(hkey);
            err
        };
        if err != ERROR_SUCCESS && err != ERROR_FILE_NOT_FOUND {
            return Err(os_error(err));
        }
        Ok(())
    }
}
