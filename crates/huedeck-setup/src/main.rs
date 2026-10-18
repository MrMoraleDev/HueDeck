// SPDX-License-Identifier: GPL-3.0-or-later
//! `huedeck-setup` — one-shot elevated installer (M5).
//!
//! Runs with UAC exactly once, only when the user opts into SMBus / GPU-I2C /
//! SuperIO: installs PawnIO (hash-pinned modules), registers the
//! `huedeck-iobroker` service, binds WinUSB to user-approved VID:PIDs. Everything
//! else in HueDeck runs unprivileged and never needs this tool.
//!
//! M0 stub.

#![forbid(unsafe_code)]

fn main() {
    println!(
        "huedeck-setup {} (stub; the installer lands in M5)",
        env!("CARGO_PKG_VERSION")
    );
}
