// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `huedeck-renderer` plumbing that does not need Chromium.
//!
//! The binary (`src/main.rs`, feature `cef`) hosts CEF windowless and feeds every `OnPaint`
//! buffer into a [`sink::FrameSink`]; everything here is CEF-free so it builds and is tested on
//! hosts without the CEF download (`cargo test -p huedeck-renderer`; `cef` is opt-in).
//!
//! * [`ring`] — writer/reader for the `huedeck_protocol::shm` frame ring (seqlock, atomics).
//! * [`sink`] — per-frame handling: publish, paint timestamps, snapshots, report.
//! * [`stats`] — paint-interval percentiles / dropped-frame count.
//! * [`snapshot`] — frame hash, "not black" metrics, PNG export.
//! * [`inject`] — `window.engine` bootstrap + control globals evaluated before page scripts.
//! * [`url`] — `file://` URLs for effect paths.
//! * [`cache`] — Chromium user-data dir: created/wiped per run, never a directory we did not make.
//! * [`cli`] — command line.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

pub mod cache;
pub mod cli;
pub mod inject;
pub mod ring;
pub mod sink;
pub mod snapshot;
pub mod stats;
pub mod url;
