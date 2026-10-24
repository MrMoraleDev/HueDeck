// SPDX-License-Identifier: GPL-3.0-or-later
//! `huedeck-iobroker`: the privileged half of HueDeck's chipset SMBus (RAM lighting).
//!
//! A LocalSystem Windows service, registered once by `huedeck-setup --smbus`
//! only when the user opts in. It talks to the PawnIO driver with raw IOCTLs
//! ([`pawnio`]), runs the signed `SmbusPIIX4` module (hash-pinned, [`pins`])
//! on one handle per PIIX4 port ([`piix4`]) on AMD boards, or `SmbusI801`
//! on the Intel PCH SMBus ([`i801`]; the two share [`chipset`]), and serves the closed operation
//! set of `huedeck_protocol::broker` over a local named pipe ([`pipe`]) to the
//! unprivileged `huedeck` / `huedeckd`. GPU I2C (NvAPI) does not go through
//! here: it runs in the user process.
//!
//! Every request goes through [`server::Server`]: protocol version, argument
//! ranges per bus kind, SPD / write-protect / PMIC / EEPROM write denial,
//! writes only to the first-party plugins' addresses, NVIDIA/AMD-GPU/SuperIO
//! kinds refused, 2000 ops/s, dry-run, an audit line per write ([`policy`]).
//! Each transfer runs under `Global\Access_SMBUS.HTP.Method` ([`mutex`]), held
//! per operation or for `BusLock` leases (at most 250 ms in one stretch), on
//! one thread.
//!
//! [`detect`] says whether PawnIO is usable: installed, 2.2.0 or later, the
//! official (signature-checking) edition. Nothing here installs anything.
//!
//! Unsafe code lives only in `pawnio.rs`, `mutex.rs` and `pipe.rs` (Windows
//! FFI), each block with its `SAFETY:` note. The pure parts (buffer layouts,
//! cell encoding, policy, the request handler) are tested against fakes; the
//! FFI paths are compiled and linted only until the hardware run.

#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

pub mod boot;
pub mod chipset;
pub mod detect;
pub mod i801;
pub mod mutex;
pub mod pawnio;
pub mod piix4;
pub mod pins;
pub mod pipe;
pub mod policy;
pub mod server;

#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub mod fake;
#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub mod fuzz;

/// Windows service name registered by `huedeck-setup --smbus`.
pub const SERVICE_NAME: &str = "HueDeckIoBroker";
/// Display name of the service.
pub const SERVICE_DISPLAY_NAME: &str = "HueDeck I/O broker (RAM lighting over SMBus)";
