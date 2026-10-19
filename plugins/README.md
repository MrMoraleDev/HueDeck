<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Lightloom device plugins

Each directory is one device **family**: one JavaScript file that says which USB ids
it drives and turns the LED colours Lightloom samples from the canvas into the packets
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
| `msi-gpu` | MSI graphics card RGB at I2C 0x68 (SMBus plugin, NVIDIA through NvAPI): RTX 3060 Gaming X Trio LHR | none yet |
| `crucial-ballistix` | Crucial Ballistix DDR4 RGB modules (SMBus plugin, chipset SMBus through the Lightloom I/O broker and PawnIO): BL8G32C16U4WL | none yet |

## A plugin directory

```
plugins/<vendor>-<family>/
  plugin.js        the plugin (GPL-3.0-or-later, like the rest of Lightloom)
  manifest.json    id, name, license, devices (vid, pids, interfaces, which pids are tested), permissions
  fixtures/        packets recorded from the device, for the tests
  tests/*.test.mjs node:test tests against plugins/_testkit/mock-device.mjs
```

and a protocol note in `docs/hardware/<device>.md`: where every fact comes from.

## Adding a model to a family Lightloom already has

Usually a product id: add it to `ProductId()` in `plugin.js` and to `pids` in
`manifest.json`. Leave it out of `tested` until someone has run it on the device.
If the model has more or fewer LEDs, adjust `Size`, `LedNames` and `LedPositions`.

## Writing a new family

Learn the protocol from the device's own traffic, from public standards or from the
vendor's published documentation, or adapt it from OpenRGB (next section). Never
copy SignalRGB plugin code: it has no license. The full rules are in
[CONTRIBUTING.md](../CONTRIBUTING.md).

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

## SMBus plugins

`Type()` `"SMBUS"` plugins (GPU and RAM lighting) find their devices with `Scan(bus)`
instead of USB ids, and Lightloom runs them only when they declare where they may look:

- `SmbusAddresses()` (required, Lightloom extension): the 7-bit addresses `Scan()` may touch
  and the devices may live at. Nothing else on the bus is reachable.
- `SmbusBuses()` (optional): bus kinds to be offered (`"nvidia"`, `"amd"`, `"intel"`, ...).
- `SmbusDeniedRegisters()` (optional, Lightloom extension): registers the host never lets the
  plugin write (save-to-flash and the like; MSI GPU: `0x3F`). The manifest lists them as
  `smbus.deniedRegisters` (checked against the export).
- GPU plugins export `BrandGPUList()`; a GPU bus is offered only to the plugins that list
  that card's four PCI ids.
- `manifest.json` has an `smbus` claim with the same `addresses` (the manifest test
  checks they match), `buses`, and for GPU plugins `pci` / `tested` card tuples.

Writes to the SPD EEPROM (0x50-0x57) are refused on every bus, and on DIMM buses also
0x30-0x37, 0x48-0x4F and 0x50-0x5F; nothing runs while SignalRGB, OpenRGB or a vendor
lighting service is running. `TransactionLock()` returns `false` when the bus cannot be
held: send nothing then. An SMBus session renders at most every 33 ms. On Windows the
chipset SMBus (RAM) is reached through the Lightloom I/O broker, which needs the PawnIO
driver installed by the user and `lightloom-setup --smbus` run once, and which writes only to
the reviewed addresses of the first-party RAM plugins; GPU buses need neither. Test with
`plugins/_testkit/mock-bus.mjs` (the `bus` objects, both shapes) next to `mock-device.mjs`,
and in Lightloom's host with `lightloom bench --mock-smbus <plugin>`. Details:
[docs/host-api/smbus.md](../docs/host-api/smbus.md), Lightloom section.

## Adapting a device from OpenRGB

OpenRGB's controllers are GPL-2.0-or-later, so their protocol code can be adapted
into a Lightloom plugin (Lightloom is GPL-3.0-or-later). This is the fastest way to
support a device nobody here owns.

1. **Find the controller** in OpenRGB's `Controllers/<Name>/`:
   - `*ControllerDetect.cpp` lists the vendor/product ids, interface and usage page
     (Lightloom: `VendorId()`, `ProductId()`, `Validate()`);
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
