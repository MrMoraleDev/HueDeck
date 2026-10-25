// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-tray`: the Lightloom icon in the notification area (see the
//! library for what the menu does). A GUI-subsystem program: no console.
//! One per sign-in session (a named mutex; on Linux the bus name
//! `org.lightloom.Lightloom.Tray`); a second start exits at once.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    std::process::exit(win::run());
    #[cfg(target_os = "linux")]
    std::process::exit(lightloom_tray::sni::run());
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        eprintln!(
            "lightloom-tray: no tray icon on this platform yet (StatusNotifierItem is planned); \
             `lightloom url` prints the dashboard's address"
        );
        std::process::exit(2);
    }
}
