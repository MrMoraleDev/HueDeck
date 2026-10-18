<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# Lightloom device plugins

Each directory is one device **family**: one JavaScript file that says which USB ids
it drives and turns the LED colours Lightloom samples from the canvas into the packets
the device expects. The host API is documented in [docs/host-api/](../docs/host-api/)
(the same shape SignalRGB plugins use, so both run on the same loader).

| Plugin | Devices | Tested on |
|---|---|---|
| `msi-mystic-light-7c91` | MSI Mystic Light 185-byte boards | MAG B550 Tomahawk `1462:7C91` |
| `razer-deathadder-v2-pro` | Razer DeathAdder V2 Pro | wired `1532:007C` |
| `nzxt-kraken-x3` | NZXT Kraken X53 / X63 / X73 | `1E71:2007` |
| `steelseries-arctis-5` | SteelSeries Arctis 5 | `1038:12AA` |

## A plugin directory

```
plugins/<vendor>-<family>/
  plugin.js        the plugin (MIT)
  manifest.json    id, name, devices (vid, pids, interfaces, which pids are tested), permissions
  LICENSE          MIT
  fixtures/        packets recorded from the device, for the tests
  tests/*.test.mjs node:test tests against plugins/_testkit/mock-device.mjs
```

and a protocol note in `docs/hardware/<device>.md`: where every fact comes from.

## Adding a model to a family Lightloom already has

Usually a product id: add it to `ProductId()` in `plugin.js` and to `pids` in
`manifest.json`. Leave it out of `tested` until someone has run it on the device.
If the model has more or fewer LEDs, adjust `Size`, `LedNames` and `LedPositions`.

## Writing a new family

The rule: **facts, not code**. Learn the protocol from the device's own traffic,
from public standards or from the vendor's published documentation; never copy code
from another RGB project.

1. **Record the traffic.** With any program that already lights the device:
   - Lightloom itself, when a compatible plugin is available (for example from a local
     SignalRGB installation with `--signalrgb`):
     `lightloom effect tools/device-probe/coords.html --signalrgb --device <vid:pid> --layout probe.json --duration 30 --record trace.json`
     with `probe.json` placing the device at `(0, 0)` with scale 10:
     ```json
     { "name": "probe", "brightness": 100,
       "endpoints": { "<vid:pid:serial>": { "position": { "x": 0, "y": 0 }, "scale": { "x": 10, "y": 10 } } } }
     ```
   - or the vendor's software, captured with USBPcap and Wireshark.
2. **Read it.** `python -I tools/device-probe/shapes.py trace.json 4` lists the
   packet shapes: which bytes are fixed headers and which carry colours.
   `python -I tools/device-probe/decode.py trace.json <vid:pid>` shows, for the probe
   effect, which bytes carry which LED (and in which colour order). Write what you
   learned in `docs/hardware/<device>.md`.
3. **Write the plugin** from those notes. Keep packets exactly as recorded, including
   bytes whose meaning is unknown (note them as such).
4. **Test it byte for byte.** Put a recorded frame (and start/shutdown sequences) in
   `fixtures/`, scrubbed of serial numbers, and assert in `tests/` that the plugin
   sends exactly those bytes for the same colours. `node --test "plugins/**/*.test.mjs"`
   runs every plugin test.
5. **Run it.** `lightloom bench --mock <vid:pid>` runs it in Lightloom's real host against
   a scripted device; `lightloom effect effects/rainbow-wave.html --device <vid:pid> --dry-run`
   then without `--dry-run` drives the real device (close other RGB software first).
