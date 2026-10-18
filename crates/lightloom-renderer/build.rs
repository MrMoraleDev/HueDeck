// SPDX-License-Identifier: GPL-3.0-or-later
//! Embeds the Windows application manifest CEF expects (supportedOS list + Common Controls v6)
//! into `lightloom-renderer.exe`. Chromium reads the compatibility section to detect the OS
//! version; without it some features silently degrade. The CEF runtime files themselves
//! (libcef.dll, *.pak, locales/ ...) are copied next to the exe by `cef-dll-sys`'s own build
//! script, see README.md.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=lightloom-renderer.exe.manifest");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lightloom-renderer.exe.manifest");
        println!("cargo::rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo::rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
