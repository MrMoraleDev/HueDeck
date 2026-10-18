<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# tools/device-probe

Helpers for learning a device's lighting protocol from its own traffic, so a HueDeck
plugin can be written from facts rather than from someone else's code. The workflow
is in [plugins/README.md](../../plugins/README.md).

- `coords.html` (CC0) - a probe effect: every canvas pixel encodes its position
  (`R = x`, `G = y`, scaled to 0..255); the blue channel steps every 4 s so plugins
  that only send on change send again.
- `shapes.py` - summarises a HueDeck trace (`--record` JSON): the first and last
  operations, then every packet shape with its count and the byte positions that vary.
- `decode.py` - finds, in a trace of the probe effect, which colour bytes carry which
  canvas cell, for devices placed at `(0, 0)` with scale 10 (see the guide).

Run the scripts with `python -I` (Python 3.13, standard library only).
