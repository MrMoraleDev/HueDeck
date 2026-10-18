<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# HueDeck device plugins

Each directory is one device **family**: one JavaScript file that says which USB ids
it drives and turns the LED colours HueDeck samples from the canvas into the packets
the device expects. The host API is documented in [docs/host-api/](../docs/host-api/)
(the same shape SignalRGB plugins use, so both run on the same loader).

| Plugin | Devices | Tested on |
|---|---|---|
| `msi-mystic-light-7c91` | MSI Mystic Light 185-byte boards, per-LED: onboard LEDs, JRGB and JRAINBOW headers | MAG B550 Tomahawk `1462:7C91` |
| `razer-deathadder-v2-pro` | Razer DeathAdder V2 Pro | wired `1532:007C` |
| `nzxt-kraken-x3` | NZXT Kraken X53 / X63 / X73 | `1E71:2007` |
| `steelseries-arctis-5` | SteelSeries Arctis 5 | `1038:12AA` |
| `sony-dualsense` | Sony DualSense light bar, over USB | `054C:0CE6` |
| `hid-lamparray` | any HID LampArray (Windows Dynamic Lighting) device no other plugin claims | Razer DeathAdder V2 Pro's LampArray collection |

## A plugin directory

```
plugins/<vendor>-<family>/
  plugin.js        the plugin (GPL-3.0-or-later, like the rest of HueDeck)
  manifest.json    id, name, license, devices (vid, pids, interfaces, which pids are tested), permissions
  fixtures/        packets recorded from the device, for the tests
  tests/*.test.mjs node:test tests against plugins/_testkit/mock-device.mjs
```

and a protocol note in `docs/hardware/<device>.md`: where every fact comes from.

## Adding a model to a family HueDeck already has

Usually a product id: add it to `ProductId()` in `plugin.js` and to `pids` in
`manifest.json`. Leave it out of `tested` until someone has run it on the device.
If the model has more or fewer LEDs, adjust `Size`, `LedNames` and `LedPositions`.

## Writing a new family

Learn the protocol from the device's own traffic, from public standards or from the
vendor's published documentation, or adapt it from OpenRGB (next section). Never
copy SignalRGB plugin code: it has no license. The full rules are in
[CONTRIBUTING.md](../CONTRIBUTING.md).

1. **Record the traffic.** With any program that already lights the device:
   - HueDeck itself, when a compatible plugin is available (for example from a local
     SignalRGB installation with `--signalrgb`):
     `huedeck effect tools/device-probe/coords.html --signalrgb --device <vid:pid> --layout probe.json --duration 30 --record trace.json`
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
5. **Run it.** `huedeck bench --mock <vid:pid>` runs it in HueDeck's real host against
   a scripted device; `huedeck effect effects/rainbow-wave.html --device <vid:pid> --dry-run`
   then without `--dry-run` drives the real device (close other RGB software first).

## Adapting a device from OpenRGB

OpenRGB's controllers are GPL-2.0-or-later, so their protocol code can be adapted
into a HueDeck plugin (HueDeck is GPL-3.0-or-later). This is the fastest way to
support a device nobody here owns.

1. **Find the controller** in OpenRGB's `Controllers/<Name>/`:
   - `*ControllerDetect.cpp` lists the vendor/product ids, interface and usage page
     (HueDeck: `VendorId()`, `ProductId()`, `Validate()`);
   - `RGBController_*.cpp` describes zones and LEDs (`Size`, `LedNames`,
     `LedPositions`, channels for ARGB headers);
   - `*Controller.cpp` builds and sends the packets (`Initialize()`, `Render()`,
     `Shutdown()`).
2. **Check the license header of every file you use.** `GPL-2.0-or-later` is fine;
   `GPL-2.0-only` is not.
3. **Translate** the packet code into `plugin.js` with the host API
   ([docs/host-api/](../docs/host-api/)). Keep byte layouts exactly as in the source.
4. **Credit it** at the top of `plugin.js`, keeping the original copyright lines:
   ```js
   // SPDX-License-Identifier: GPL-3.0-or-later
   // Adapted from OpenRGB Controllers/<Name>/<File>.cpp and RGBController_<Name>.cpp
   // (GPL-2.0-or-later) at commit <sha>, https://gitlab.com/CalcProgrammer1/OpenRGB
   // Copyright (C) <years> <authors, exactly as in the OpenRGB files>
   ```
   and list the files in the "Sources" section of `docs/hardware/<device>.md`.
5. **Test it** like any other plugin: write the expected packets for known colours
   into `fixtures/` by working them out from the OpenRGB code (not by running the
   plugin), and assert them in `tests/`.
6. **Mark it untested** (leave the product ids out of `tested` in `manifest.json`)
   until someone has run it on the real device.
