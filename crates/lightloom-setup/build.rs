// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows icon and version resource (lightloom-winres).

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    lightloom_winres::embed("Lightloom setup helper (RAM lighting)");
    // asInvoker manifest: started unelevated it asks for elevation itself
    // (`--elevate`), never through installer-detection heuristics.
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo::rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo::rustc-link-arg-bins=/MANIFESTUAC:level='asInvoker'");
    }
}
