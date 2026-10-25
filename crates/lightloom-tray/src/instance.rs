// SPDX-License-Identifier: GPL-3.0-or-later
//! Find the running tray of this session and ask it to close (`lightloomw
//! --stop`, before the installer replaces or removes the files).

use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

/// The tray's (hidden) window class.
pub const WINDOW_CLASS: PCWSTR = w!("LightloomTray");

/// Close the tray if one runs and wait up to `wait` for it to go; `true`
/// when none is left.
pub fn close_tray(wait: Duration) -> bool {
    // SAFETY: a lookup by a static class name; posting WM_CLOSE to a window
    // handle is harmless even if the window has just gone.
    let posted = unsafe {
        match FindWindowW(WINDOW_CLASS, PCWSTR::null()) {
            Ok(hwnd) => PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok(),
            Err(_) => return true,
        }
    };
    if !posted {
        return false;
    }
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        // SAFETY: as above.
        if unsafe { FindWindowW(WINDOW_CLASS, PCWSTR::null()) }.is_err() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}
