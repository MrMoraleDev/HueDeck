// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows: embeds the application manifest CEF expects (supportedOS list + Common Controls v6)
//! into `lightloom-renderer.exe`. Chromium reads the compatibility section to detect the OS
//! version; without it some features silently degrade.
//!
//! Linux (feature `cef`): adds an `$ORIGIN` rpath to the binaries. `libcef.so` sits next to the
//! executable (cef-dll-sys copies it there, and packages ship it there), and without the rpath
//! the dynamic loader would only find it through `LD_LIBRARY_PATH`. The same goes for every
//! Chromium subprocess, which is this executable again.
//!
//! The CEF runtime files themselves (libcef.dll / libcef.so, *.pak, locales/ ...) are copied
//! next to the binary by `cef-dll-sys`'s own build script, see README.md.

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
    if target_os == "linux" && std::env::var_os("CARGO_FEATURE_CEF").is_some() {
        // Passed to the linker literally (no shell), so `$ORIGIN` needs no escaping.
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
