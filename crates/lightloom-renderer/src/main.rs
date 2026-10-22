// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-renderer` — CEF offscreen renderer process.
//!
//! Hosts one windowless Chromium browser (CEF, Alloy style) on an effect's `file://` URL at
//! 320x200 and copies every `OnPaint` BGRA buffer into the shm ring described by
//! `lightloom_protocol::shm`. With `--frames N` it stops after N frames; with `--serve` it runs
//! until stdin closes, taking control commands from it (`control.rs`). Either way it prints
//! paint-interval statistics as JSON at the end. The same executable serves as every Chromium
//! subprocess (`--type=renderer|gpu-process|utility|...`) through `cef::execute_process`. See
//! README.md.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

#[cfg(feature = "cef")]
mod cef_host;

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(feature = "cef")]
    {
        // Chromium subprocesses carry `--type=...`; they must not see our CLI parser.
        if let Some(code) = cef_host::run_subprocess_if_any(std::ptr::null_mut()) {
            std::process::exit(code);
        }
    }

    let args = <lightloom_renderer::cli::Args as clap::Parser>::parse();

    #[cfg(feature = "cef")]
    {
        ExitCode::from(cef_host::run(args, std::ptr::null_mut()))
    }

    #[cfg(not(feature = "cef"))]
    {
        let _ = args;
        eprintln!(
            "lightloom-renderer was built without the `cef` feature; rebuild with `--features cef` \
             (needs the CEF binary distribution, see crates/lightloom-renderer/README.md)"
        );
        ExitCode::from(2)
    }
}
