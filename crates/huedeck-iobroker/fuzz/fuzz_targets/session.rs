// SPDX-License-Identifier: GPL-3.0-or-later
//! Bytes driving a session of well-formed requests (batches, leases, SPD
//! reads, dry-run, rate limit, failures) against the fake broker, with the
//! policy invariants checked after every step.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    huedeck_iobroker::fuzz::session(data);
});
