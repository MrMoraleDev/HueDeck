<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# tools/pawnio-hash-pin

The PawnIO modules the I/O broker loads are pinned by SHA-256 and size in two places:

- `crates/lightloom-iobroker/src/pins.rs`: what the broker and `lightloom-setup` accept;
- `tools/package/fetch-pawnio-modules.ps1`: what the packaging downloads and stages.

`cargo test -p lightloom-iobroker --test pawnio_pins` keeps the two in step (same release,
same module hashes) without the network. `pawnio-hash-pin.ps1` checks them against the real
files:

```
tools/pawnio-hash-pin/pawnio-hash-pin.ps1                       # download the pinned release and check it
tools/pawnio-hash-pin/pawnio-hash-pin.ps1 -Dir <pawnio-modules>  # check files already on disk
tools/pawnio-hash-pin/pawnio-hash-pin.ps1 -Release 0.2.12 -Emit  # a new release: print the new pins
```

Without `-Dir` it downloads `release_<x_y_z>.zip` of
https://github.com/namazso/PawnIO.Modules/releases into a temporary folder (removed after),
prints the archive's SHA-256 (checked against the script's `$zipSha` for the pinned release)
and hashes the `*.bin` files in it. Each module `pins.rs` names must be there with the pinned
size and hash, and the script's `$fromZip` must agree; otherwise it exits 1.

`-Emit` prints `Pin` entries and `$fromZip` lines for the files found. Use them only after
checking that the archive is the signed release; then update `MODULES_RELEASE`, the pins,
the script's `$release`, `$zipSha` and source hashes together.
