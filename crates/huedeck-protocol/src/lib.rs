// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Wire types shared by the HueDeck processes.
//!
//! * [`shm`]: layout of the shared-memory frame ring written by
//!   `huedeck-renderer` and read by `huedeckd` (`"HDFR"` header, three
//!   320x200 BGRA slots with seqlock-style generation stamps).
//! * [`broker`]: the closed operation enum spoken over the named pipe / unix
//!   socket between `huedeckd` and the privileged `huedeck-iobroker`.
//!
//! Everything is `serde` and encoded with `postcard`; the shm header is
//! additionally a fixed little-endian byte layout because it lives in raw memory.

#![forbid(unsafe_code)]

pub mod broker;
pub mod shm;

/// Encode any wire message with postcard.
pub fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(value)
}

/// Decode any wire message with postcard.
pub fn decode<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}
