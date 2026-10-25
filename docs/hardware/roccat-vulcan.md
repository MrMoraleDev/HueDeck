<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Roccat Vulcan family keyboards (`1E7D`, and Turtle Beach `10F5`)

Plugins: `plugins/roccat-vulcan/` (vendor `1E7D`) and `plugins/roccat-vulcan-turtle-beach/`
(vendor `10F5`, see [roccat-vulcan-turtle-beach.md](roccat-vulcan-turtle-beach.md)). Both use the
same `vulcan.js` (protocol) and `layouts.js` (key layouts, generated from OpenRGB's tables); a test
checks that the two copies are identical. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| Source | License | Taken |
|---|---|---|
| `Controllers/RoccatController/RoccatControllerDetect.cpp` | GPL-2.0-or-later | Product ids and names, detector macro (`REGISTER_HID_DETECTOR_IP`, interface 1 + usage page), the two collections `DetectRoccatVulcanKeyboardControllers` opens |
| `Controllers/RoccatController/RoccatVulcanKeyboardController/RoccatVulcanKeyboardController.cpp`, `.h` (Mola19, 17 Dec 2021) | GPL-2.0-or-later | Info read, `EnableDirect`, `WaitUntilReady`, `SendColors` (all packet layouts), `ClearResponses` / `AwaitResponse`, the Vulcan II Mini slot map, product ids |
| `.../RGBController_RoccatVulcanKeyboard.cpp` (Mola19, 17 Dec 2021) | GPL-2.0-or-later | Layout table per product, layout selection, zones (Vulcan II Max linear zones), the start sequence (`DeviceUpdateMode`) |
| `.../RoccatVulcanKeyboardLayouts.h` (Mola19, 29 Sep 2022) | GPL-2.0-or-later | Matrix maps, LED names and wire ids (generated into `layouts.js`) |
| `RGBController/RGBControllerKeyNames.cpp` | GPL-2.0-or-later | Key name strings (mapped to Lightloom's key names) |

Every file listed carries `SPDX-License-Identifier: GPL-2.0-or-later`.

## Products

| PID | OpenRGB name | Control collection | LED collection | Layout table |
|---|---|---|---|---|
| `3124` | Roccat Magma | if 1, page `FF01` | if 3, page `FF00` | Magma (5 zones) |
| `69A0` | Roccat Magma Mini | if 1, `FF01` | if 3, `FF00` | Magma |
| `314C` | Roccat Pyro | if 1, `FF01` | if 3, `FF00` | Pyro |
| `307A` | Roccat Vulcan 100 Aimo | if 1, `000B` | if 3, `0001` | 120 Aimo |
| `3098` | Roccat Vulcan 120-Series Aimo | if 1, `000B` | if 3, `0001` | 120 Aimo |
| `2FEE` | Roccat Vulcan TKL | if 1, `000B` | if 3, `0001` | TKL |
| `30F7` | Roccat Vulcan Pro | if 1, `FF01` | if 3, `FF00` | Pyro |
| `311A` | Roccat Vulcan TKL Pro | if 1, `FF01` | if 3, `FF00` | TKL |
| `2F4E` | Roccat Vulcan II | if 1, `FF01` | if 3, `FF00` | Vulcan II (UK only) |
| `2EE2` | Roccat Vulcan II Max | if 1, `FF01` | if 3, `FF00` | Vulcan II Max (US, JP) |
| `2F09` | Roccat Vulcan II Mini | if 1, `FF01` | if 3, `FF00` | Vulcan II Mini |

`Validate()` accepts both collections (interface and usage page, any usage, as OpenRGB matches
them); `Initialize()` finds them with `getHidEndpoints()` and switches with `set_endpoint()`. If
several collections match the LED one, Lightloom prefers the one whose report descriptor declares the
65-byte output report (OpenRGB takes the first enumerated).

## Protocol

**Info** (control collection): GET_FEATURE `0x09` (9 bytes; newer family: Pyro, Magma, Magma Mini,
Vulcan Pro, Vulcan TKL Pro, Vulcan II / II Mini / II Max, Turtle Beach Vulcan II / II TKL Pro) or
`0x0F` (8 bytes; the others), read twice as in the C++. Byte 2 is the firmware version (`116` =
1.16); byte 6 the layout (0 US, 1 DE, 2 UK, 3 FR, 6 JP) when at least 7 bytes came back. DE / UK / FR
use the UK table; Magma always US.

**Direct mode on / off**: SET_FEATURE `0E 05 <on> 00 00` (newer family) or `15 00 <on>`; then
`WaitUntilReady`: GET_FEATURE `0x04` (4 bytes for Pyro, Magma, Magma Mini, Vulcan Pro, TKL Pro;
else 3) until byte 1 is 1, at most 100 reads 25 ms apart.

**Colour frame** (`SendColors`, LED collection): `ceil(packet_length / 64)` output reports of 65
bytes, report id `0x00`, then `A1 <n>`; the first carries the length (1 byte if <= 255, else 2:
big-endian when firmware major >= 1 and minor >= 16, never on the Vulcan II Mini, else
little-endian). Per LED (wire id = `led.value`): protocol 1 (older models, 436 bytes, 12-row
columns) packs R, G, B planes per column continuously over the packets; protocol 2 does it per
packet after the header (`packet_length`, column length: Magma 64/5, Pyro 378/1, Vulcan Pro and TKL
Pro 384/12, Vulcan II 396/1, II Max 567/1, II Mini 345/1 through the slot map); the Turtle Beach TKL
Pro uses three planes of `max id + 1` bytes. Then the input reports are drained (read, timeout 0)
and one more is awaited for 20 ms. Lightloom skips unchanged frames and resends them after 1 s.

## Deviations from OpenRGB (deliberate)

- **SendMode is never sent.** On the way into Direct, OpenRGB sends `SendMode(Direct)`: a 443-byte
  `0x0D` (or `0x11`, 26..567 bytes) feature report with mode, speed, brightness and a checksum,
  which is the lighting setting the keyboard keeps in its profile (the same report saves Static and
  Wave, which OpenRGB flags as automatically saved). Lightloom's rules forbid writing stored settings,
  so Lightloom only switches direct mode on (`EnableDirect(true)` + ready wait). **Open question:**
  whether the keyboard shows direct frames without that report. If it does not, the plugin will
  simply light nothing; it cannot harm the keyboard.
- **Shutdown** is `EnableDirect(false)` + ready wait (the switch OpenRGB sends when it hands the
  keyboard to a hardware mode): the keyboard shows its own stored lighting again. No save.
- **Missing layout tables.** OpenRGB's `std::map::operator[]` creates an empty layout (no LEDs) when
  a table has none for the reported layout (Vulcan II reporting US, Vulcan II Max reporting UK / DE /
  FR, any JP keyboard but the II Max). Lightloom uses US instead, else the table's only layout.
- **Names.** LEDs get Lightloom's key names; OpenRGB's own text without `Key: ` where Lightloom has none
  (JP keys, the Vulcan II Max secondary LEDs). The Turtle Beach Vulcan II TKL Pro table names LED 56
  (id `0x3A`, the cell right of ISO `<`) "Y" twice; it is named "Z". `KEY_EN_UNUSED` LEDs are named
  `LED 0x<id>`.
- The Vulcan II Max US table has one LED (id `0x06`) in no matrix cell: it is sent black.
- Buffer bounds are checked for every product (the C++ checks only the TKL Pro); a generator check
  confirms no table writes outside the buffers, so nothing the C++ sends is dropped.

## Left out

- `SendMode` (see above), the Vulcan II Mini static-mode keepalive (`0x13`), the hardware modes
  (Static, Rainbow Wave, Default), brightness (only carried by SendMode).

## Untested

Every product id. Fixtures come from a reference model of the C++ (`fixtures/expected.json`,
`$comment`), not from hardware.
