// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `huedeck-renderer` — CEF offscreen renderer process (M3).
//!
//! Will host a windowless Chromium browser per effect (plus one per LCD
//! face), copy each `on_paint` BGRA buffer into the next shm slot described
//! by `huedeck_protocol::shm`, and signal the sequence number on the control
//! pipe. The same executable serves as the CEF subprocess.
//!
//! M0 stub: prints the shm geometry it will produce and exits.

#![forbid(unsafe_code)]

use huedeck_protocol::shm::{FrameHeader, CANVAS_HEIGHT, CANVAS_WIDTH, SLOT_COUNT};

fn main() {
    let header = FrameHeader::new(CANVAS_WIDTH, CANVAS_HEIGHT);
    println!(
        "huedeck-renderer {} (stub; CEF OSR lands in M3): {}x{} BGRA, {} slots, {} bytes shm",
        env!("CARGO_PKG_VERSION"),
        header.width,
        header.height,
        SLOT_COUNT,
        header.total_bytes()
    );
}
