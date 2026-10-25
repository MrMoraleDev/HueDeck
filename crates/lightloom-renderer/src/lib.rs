// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-renderer` plumbing that does not need Chromium.
//!
//! The binary (`src/main.rs`, feature `cef`) hosts CEF windowless and feeds every `OnPaint`
//! buffer into a [`sink::FrameSink`]; everything here is CEF-free so it builds and is tested on
//! hosts without the CEF download (`cargo test -p lightloom-renderer`; `cef` is opt-in).
//!
//! * [`ring`] — writer/reader for the `lightloom_protocol::shm` frame ring (seqlock, atomics).
//! * [`sink`] — per-frame handling: publish, paint timestamps, snapshots, report.
//! * [`stats`] — paint-interval percentiles / dropped-frame count.
//! * [`snapshot`] — frame hash, "not black" metrics, PNG export.
//! * [`inject`] — `window.engine` bootstrap + control globals evaluated before page scripts.
//! * [`control`] — the control channel of a serving renderer (`--serve`): commands → JS.
//! * [`ipc`] — per-platform transport: where the ring file lives (tmpfs on Linux), the control
//!   channel (stdin, or a unix domain socket on Linux), bounded line reads.
//! * [`url`] — `file://` URLs for effect paths.
//! * [`cache`] — Chromium user-data dir: created/wiped per run, never a directory we did not make.
//! * [`cli`] — command line.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

pub mod cache;
pub mod cli;
pub mod control;
pub mod inject;
pub mod ipc;
pub mod ring;
pub mod sink;
pub mod snapshot;
pub mod stats;
pub mod url;
