<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# tools/shm-dump

Writes the newest frame of the renderer's shared-memory ring to a PNG, for debugging
("why are the LEDs black?", frame timing). The implementation is the `lightloom shm-dump`
subcommand (`crates/lightloom-cli/src/shm.rs`); there is no separate binary.

```
lightloom shm-dump [RING] [--out FILE] [--json]
```

- `RING`: the ring file. Default: the newest `frames-*.shm` in the daemon's renderer
  directory (`%LOCALAPPDATA%\Lightloom\renderer` on Windows; `LIGHTLOOM_HOME` moves it).
  On Linux the ring sits in a private tmpfs directory (`$XDG_RUNTIME_DIR/lightloom-render-*`),
  so pass its path.
- `--out`: the PNG (RGB, 320x200). Default: `frame-<seq>.png` in the current directory.
- `--json`: print the ring header (`width`, `height`, `seq`, `slotGenerations`) and the frame's
  SHA-256, lit fraction and mean luma as JSON.

The ring is mapped read-only and read through its seqlock (`lightloom_renderer::ring::RingReader`),
so a running renderer is never disturbed. The layout is in `crates/lightloom-protocol/src/shm.rs`.
