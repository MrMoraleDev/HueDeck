# huedeck-iobroker fuzzing

The broker is HueDeck's privilege boundary (a LocalSystem service), so its
request decoding and handling are fuzzed. The harness lives in
`src/fuzz.rs` (behind the `fuzzing` feature, and in `cfg(test)`); its module
docs list every invariant. It has two entry points over arbitrary bytes:

- `frame`: bytes as they arrive on the pipe (length prefix, `frame_complete`,
  `read_frame`, postcard decoding of requests and responses, round trips),
  each decoded request then served by the fake broker;
- `session`: bytes driving a sequence of well-formed requests (`Batch`,
  `BusLock` / `BusUnlock`, `SpdRead`, dry-run, rate limit, simulated time,
  contention, failing transfers) against `Server` over fake PawnIO buses.

## On stable, any OS (what CI runs)

```sh
cargo test -p huedeck-iobroker --lib fuzz          # proptest + 5,000-input driver
HUEDECK_FUZZ_ITERS=200000 cargo test -p huedeck-iobroker --lib fuzz::tests::driver_fast_run
# The WP-FUZZ acceptance run (1M inputs):
cargo test --release -p huedeck-iobroker --lib fuzz::tests::driver_one_million -- --ignored --nocapture
```

The driver is deterministic (SplitMix64, fixed seeds); a failure prints the
iteration and the input bytes, which reproduce through `fuzz::session` or
`fuzz::frame`.

## cargo-fuzz (nightly + libFuzzer, Linux)

This directory is its own workspace, excluded from the main one.

```sh
cargo install cargo-fuzz
cd crates/huedeck-iobroker/fuzz
cargo +nightly fuzz run frame
cargo +nightly fuzz run session -- -max_len=2048
```

On Windows MSVC libFuzzer support is experimental; use the stable driver above.
