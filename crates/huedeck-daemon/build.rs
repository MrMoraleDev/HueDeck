// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows icon and version resource (huedeck-winres).

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    huedeck_winres::embed("HueDeck daemon");
}
