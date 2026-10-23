<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# tools/

Developer and packaging helpers that are not part of the shipped binaries.

| Tool              | Purpose                                                                                   | Status |
|-------------------|-------------------------------------------------------------------------------------------|--------|
| `gen-udev/`       | Emit `70-lightloom.rules` from the plugin manifests: `lightloom udev generate` (see its README) | done (M7) |
| `pawnio-hash-pin/`| Compute and verify SHA-256 pins of the PawnIO modules the iobroker is allowed to load      | planned (M5) |
| `shm-dump/`       | Dump the renderer shared-memory ring (3x 320x200 BGRA + engine data + seq) to PNG/JSON for debugging frame timing | planned |
| `effects-bench/`  | `bench.py`: paint cadence, errors and memory of effects through `lightloom-renderer` (GPU and software compositing); `psnr.py`: frames against stock Chrome on a frozen harness page (docs/hardware/effects-m3.md) | done (M3) |
| `device-probe/`   | Probe effect and trace readers for learning a device's lighting protocol from its own traffic (see plugins/README.md) | done |
| `package/`        | `stage-windows.ps1`: the portable Windows build in one directory (daemon, CLI, renderer + CEF runtime, dashboard, plugins, effects, licenses, Chromium notices); used by the release workflow | done (M4) |

Conventions:

- Small tools are Rust binaries in their own crate under `tools/<name>/` (not workspace
  default members); scripts that need no compilation may be Python 3.13 run with `-I`.
- Every file carries `SPDX-License-Identifier: GPL-3.0-or-later`.
- Tools never read WhirlwindFX-authored files from the repository; they operate on the
  user's own installation directories when a corpus is required.
