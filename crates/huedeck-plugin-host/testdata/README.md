<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Synthetic compat-shape plugins

Written for HueDeck's tests (not taken from any corpus). They exercise the
rquickjs loader and stub host: call-if-function metadata, `@SignalRGB/*`
host modules, relative imports, `export default class`, lifecycle detection,
ControllableParameters globals, API-gap recording and the watchdog.

- `plugins/hid_mouse.js`: HID device, `ProductId` as array, parameters, hooks.
- `plugins/smbus_ram.js`: `Type() "SMBUS"` with `Scan(bus)`.
- `plugins/network_lights.js`: `Type() "network"` with `DiscoveryService`, `@SignalRGB/udp`.
- `plugins/vendor/class_keyboard.js`: `export default class`, `ProductId` from `Object.keys`, relative import.
- `plugins/vendor/lib/protocol.js`: helper module (no `ProductId`).
- `plugins/gaps_and_loops.js`: touches undocumented members; `Initialize()` never returns.
- `device/rawusb.js`: `Type() "rawusb"`, vendor control requests and bulk IN/OUT (raw USB session tests).
- `device/hybrid.js`: `Type() "Hybrid"`, HID writes plus bulk OUT and `setProtocol("hid" | "hybrid")`.
- `broken/syntax_error.js`: does not parse.
- `oom/hoarder.js`: allocates until the 64 MiB (test: 8 MiB) heap cap stops it.
- `edge/`: harness edge cases. A promise job that never ends (`job_spin.js`),
  an async `Initialize` that never settles (`async_never.js`), an unknown
  `@SignalRGB/*` import (`unknown_module.js`), a link error against
  `lib/dep.js` (`link_error.js`), a run-time `SyntaxError`
  (`runtime_syntax_error.js`), an attempt to replace the host helper object
  (`host_tamper.js`), an import of a host-internal module
  (`internal_import.js`) and a thrown non-Error object (`throw_object.js`).
