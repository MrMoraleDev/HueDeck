// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `lightloomd`, the per-user Lightloom daemon, as a library: the binary in
//! `main.rs` parses arguments and runs it; tests drive the parts directly.

#![forbid(unsafe_code)]

pub mod store;
