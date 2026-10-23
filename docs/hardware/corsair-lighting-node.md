<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Corsair Lighting Node family (`1B1C`): two RGB headers, adapted from OpenRGB

Plugin: `plugins/corsair-lighting-node/`. Untested on hardware (`tested` is empty). It drives the
two RGB headers of the Lighting Node Core / Pro, the Commander Pro (lighting only), the LS100 and
LT100 kits and the 1000D Obsidian / SPEC OMEGA RGB cases, per LED, as two Lightloom channels.

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file carries
`SPDX-License-Identifier: GPL-2.0-or-later` and names
`Adam Honse (calcprogrammer1@gmail.com)      12 Jan 2020`.

| # | File | License | What was taken |
|---|------|---------|----------------|
| S1 | `Controllers/CorsairLightingNodeController/CorsairLightingNodeControllerDetect.cpp` | GPL-2.0-or-later | Product ids and names, detector macro (`REGISTER_HID_DETECTOR`: no interface / usage filter) |
| S2 | `.../CorsairLightingNodeController.cpp`, `.h` | GPL-2.0-or-later | Packet ids and layouts (firmware request, port state, direct, commit), report sizes, read length and timeout, 50-LED chunking, the keepalive interval |
| S3 | `.../RGBController_CorsairLightingNode.cpp`, `.h` | GPL-2.0-or-later | Two linear zones "Corsair RGB Header 1 / 2" of 0..204 LEDs, the order zones are sent in Direct mode |

## How the fixtures were made

OpenRGB's own `CorsairLightingNodeController` and `RGBController_CorsairLightingNode` were compiled
with g++ outside the repository (stub headers for hidapi, the logger, threads and the device guard)
and driven by a harness: constructors, `SetChannelLEDs` for both channels as `DeviceUpdateLEDs`
does in Direct mode (empty, short, more than 50 and full 204-LED channels), and the shutdown
sequence below made from OpenRGB's own `SendPortState` / `SendCommit`. `plugin.js` was never run to
make `fixtures/expected.json`.

## Products

| Product id | OpenRGB name |
|---|---|
| `1B1C:0C1A` | Corsair Lighting Node Core (OpenRGB comment: 1 channel) |
| `1B1C:0C0B` | Corsair Lighting Node Pro |
| `1B1C:0C10` | Corsair Commander Pro |
| `1B1C:0C1E` | Corsair LS100 Lighting Kit (1 channel) |
| `1B1C:1D00` | Corsair 1000D Obsidian |
| `1B1C:1D04` | Corsair SPEC OMEGA RGB |
| `1B1C:0C23` | Corsair LT100 |

OpenRGB gives every product two zones (also the one-channel ones), so Lightloom adds two channels,
"Corsair RGB Header 1" and "2", each up to 204 LEDs; the user sets the LED count with components.

**Validate.** `REGISTER_HID_DETECTOR` has no interface / usage filter, so OpenRGB tries every HID
collection. Lightloom binds one collection per device, so the plugin accepts vendor-defined usage
pages (`0xFF00`-`0xFFFF`), as the NZXT Hue 2 plugin does. The descriptors are unverified.

## Protocol

Every packet is a 65-byte **output report** (`0x00` report id, packet id, arguments, zeros),
followed by one input report read of 17 bytes with a 15 ms timeout (`WriteAndRead`).

- **Start** (constructor): firmware request `00 02`.
- **Every frame**, channel 1 then channel 2 (`SetChannelLEDs`): port state software
  `00 38 <ch> 02`; for each run of at most 50 LEDs, three direct packets
  `00 32 <ch> <start> <count> <component> <count bytes>` (component 0 red, 1 green, 2 blue); then
  commit `00 33 FF`. A channel with no LEDs still gets the port state and the commit.
- Lightloom skips unchanged frames and resends after 1 s. The device returns to its own effect when
  it gets no commit for about 20 s (OpenRGB's comment); OpenRGB's keepalive commits every 5 s.
- **Shutdown.** OpenRGB has no exit step. Lightloom sends OpenRGB's port-state packet with
  "hardware" (`00 38 00 01`, `00 38 01 01`) and a commit (`00 33 FF`): both headers go back to
  hardware control, i.e. the effect the device has stored. Nothing is configured or stored. (Even
  without it, the device would fall back to that effect about 20 s after the last commit.)

## Left out (safety)

- **Hardware effects** (`SetChannelEffect`: reset `0x37`, begin `0x34`, effect configuration
  `0x35`): OpenRGB marks all of them `MODE_FLAG_AUTOMATIC_SAVE`, i.e. the device stores them.
  Never sent.
- **Brightness** (`0x39`): OpenRGB sets 100 % on both channels when a mode is applied (also
  Direct). Whether the device stores it with its effect configuration is unknown, so Lightloom does
  not send it. If iCUE left a lower brightness on the device, Direct colours may look dimmer.
- LED count and protocol packets (`0x3A`, `0x3B`) are empty functions in OpenRGB.
- The Commander Pro's fan and temperature commands are not part of OpenRGB's driver and not part of
  this plugin.

## Untested

Every product id. Check on hardware: the collection the plugin binds to, that the port state
"hardware" + commit at shutdown brings back the stored effect, and the brightness question above.

## Held back: Corsair Commander Core / Core XT (`0C1C`, `0C32`, `0C1D`, `0C3C`, `0C3D`, `0C3E`, `0C2A`)

OpenRGB's `CorsairCommanderCoreController` (GPL-2.0-or-later, Controllers/CorsairCommanderCoreController)
was read for this batch and **not ported**. Its direct mode only works after `InitController()`
has called `SetFanMode(false)`, which writes a 15-byte configuration (`07 01 08 01 06 ...`) to the
device's endpoint `0x1E` (data type `0x0D`); the XT's external port also gets an LED amount written
to endpoint `0x1D`. Neither OpenRGB nor anything at hand documents what these endpoints are (the
function is called "fan mode" and the Commander Core also drives fans and the pump), nor whether the
device stores them. The lighting path cannot be separated from that write, so Lightloom does not claim
these ids. Answer on hardware (USB capture of iCUE) before porting.
