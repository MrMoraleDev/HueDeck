// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-tray`: the Lightloom icon in the notification area (see the
//! library for what the menu does). A GUI-subsystem program: no console.
//! One per sign-in session (a named mutex); a second start exits at once.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod win;

fn main() {
    #[cfg(windows)]
    std::process::exit(win::run());
    #[cfg(not(windows))]
    {
        eprintln!(
            "lightloom-tray: no tray icon on this platform yet (StatusNotifierItem is planned); \
             `lightloom url` prints the dashboard's address"
        );
        std::process::exit(2);
    }
}
