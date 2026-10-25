<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# tools/

Developer and packaging helpers that are not part of the shipped binaries.

| Tool              | Purpose                                                                                   | Status |
|-------------------|-------------------------------------------------------------------------------------------|--------|
| `gen-udev/`       | Emit `70-lightloom.rules` from the plugin manifests: `lightloom udev generate` (see its README) | done (M7) |
| `pawnio-hash-pin/`| `pawnio-hash-pin.ps1`: recompute the PawnIO module hashes (a folder, or the release downloaded) and check them against `pins.rs` and `fetch-pawnio-modules.ps1`; `-Emit` prints new pins for a release bump (see its README) | done |
| `shm-dump/`       | Dump the newest frame of the renderer's shared-memory ring to PNG, with the header (seq, slot generations) and frame metrics: `lightloom shm-dump` (see its README) | done |
| `effects-bench/`  | `bench.py`: paint cadence, errors and memory of effects through `lightloom-renderer` (GPU and software compositing); `psnr.py`: frames against stock Chrome on a frozen harness page (docs/hardware/effects-m3.md) | done (M3) |
| `device-probe/`   | Probe effect and trace readers for learning a device's lighting protocol from its own traffic (see plugins/README.md) | done |
| `package/`        | `stage-windows.ps1`: the Windows build in one directory (daemon and `lightloomw`, CLI, renderer + CEF runtime, `lightloom-setup` and the I/O broker, dashboard, plugins, effects, licenses, Chromium notices) for the portable zip and the MSI; `fetch-pawnio-modules.ps1`: the PawnIO.Modules 0.2.11 files and sources, SHA-256 pinned; used by the release workflow | done (M4) |

Conventions:

- Small tools are Rust binaries in their own crate under `tools/<name>/` (not workspace
  default members); scripts that need no compilation may be Python 3.13 run with `-I`.
- Every file carries `SPDX-License-Identifier: GPL-3.0-or-later`.
- Tools never read WhirlwindFX-authored files from the repository; they operate on the
  user's own installation directories when a corpus is required.
