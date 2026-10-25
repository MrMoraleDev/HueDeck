// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows icon and version resource (lightloom-winres).

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    lightloom_winres::embed("Lightloom tray");
}
