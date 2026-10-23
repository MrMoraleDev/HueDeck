// SPDX-License-Identifier: GPL-3.0-or-later
//! Bytes as they arrive on the broker's pipe: length prefix, postcard
//! decoding, round trips, then every decoded request served by the fake broker.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    huedeck_iobroker::fuzz::frame(data);
});
