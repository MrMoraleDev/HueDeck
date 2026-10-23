<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Wooting keyboards

Plugins: `plugins/wooting/` (Wooting vendor id `31E3`) and `plugins/wooting-legacy/` (legacy V1
firmware on the Atmel vendor id `03EB`, notes: [wooting-legacy.md](wooting-legacy.md));
`family.js` and `layouts.js` are shared byte for byte (a test checks it). Untested on hardware
(`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (under `Controllers/WootingKeyboardController/`) | License | Author in header | Taken |
|---|---|---|---|
| `WootingKeyboardControllerDetect.cpp` | GPL-2.0-or-later | Diogo Trindade (diogotr7), 25 Dec 2025 | Product ids (base id with nibble 0 / 1 / 2), names, product id to keyboard type, usage page to report format |
| `WootingKeyboardController.cpp`, `.h` | GPL-2.0-or-later | Chris M (Dr_No), 09 Jul 2021; Diogo Trindade (diogotr7), 25 Dec 2025 | `wooting_usb_send_feature`, `SendInitialize`, command numbers |
| `WootingV1KeyboardController/*` | GPL-2.0-or-later | Diogo Trindade (diogotr7), 25 Dec 2025 | V1 frame: LED index table, PWM register map, parts, CRC |
| `WootingV2KeyboardController/*` | GPL-2.0-or-later | Chris M (Dr_No); Diogo Trindade (diogotr7) | V2 frame, R5G6B5 conversion |
| `WootingV3KeyboardController/*` | GPL-2.0-or-later | Diogo Trindade (diogotr7), 25 Dec 2025 | V3 frame |
| `RGBController_WootingKeyboard.cpp`, `.h` | GPL-2.0-or-later | Diogo Trindade (diogotr7), 25 Dec 2025 | Matrix maps and LED names per keyboard type, frame buffer |
| `RGBController/RGBControllerKeyNames.cpp`, `.h` | GPL-2.0-or-later | Chris M (Dr_No), 25 Jan 2022 | Key name strings |
| Wooting's public RGB SDK (`wooting-rgb-sdk`, `wooting_rgb_reset_rgb`) | vendor documentation (fact only, no code) | | That command 32 (`RESET_ALL`) gives the keys their profile colours back |

## Products (`31E3`)

Each model registers three product ids: base | 2 (no gamepad emulation), | 0 (Xbox), | 1
(DirectInput); 33 ids in all.

| Base id | Ids | OpenRGB name | Keyboard type (layout) |
|---|---|---|---|
| `1100` | `1100`-`1102` | Wooting One | TKL (17 x 6, 89 LEDs) |
| `1200` | `1200`-`1202` | Wooting Two | FULL (21 x 6, 110 LEDs) |
| `1210` | `1210`-`1212` | Wooting Two Lekker Edition | FULL |
| `1220` | `1220`-`1222` | Wooting Two HE | FULL |
| `1230` | `1230`-`1232` | Wooting Two HE (ARM) | FULL |
| `1300` | `1300`-`1302` | Wooting 60HE | 60PER (14 x 5, 67 LEDs) |
| `1310` | `1310`-`1312` | Wooting 60HE (ARM) | 60PER |
| `1320` | `1320`-`1322` | Wooting 60HE+ | 60PER |
| `1340` | `1340`-`1342` | Wooting 60HEv2 | 60PER |
| `1400` | `1400`-`1402` | Wooting 80HE | 80PER (17 x 6, 94 LEDs) |
| `1510` | `1510`-`1512` | Wooting UwU RGB | 3PAD (7 x 5, 17 LEDs) |

Validate: any interface, usage page `1337` (V2 firmware, up to 2.11) or `FF55` (V3, 2.12 and
later). OpenRGB runs its detector on every collection and drives each Wooting one; Lightloom opens
one, so `Initialize()` makes the `FF55` collection active when there is one, else the `1337`
one (`getHidEndpoints()` / `set_endpoint()`; without them, the open collection's usage page from
`getHidReportFields()`). With no Wooting collection nothing is sent.

## LEDs

`RGBController_WootingKeyboard::SetupZones`: one LED per non-empty matrix cell, row by row, named
from `led_names_*` at the same cell (names kept as OpenRGB has them, including "Lighbar 1" and the
repeated "Key: JIS Specific TODO" of the 80HE). Each LED's value is its index in the 6 x 21 frame
buffer sent to the keyboard; the grid is the matrix.

## Protocol

- **Start** (`SendInitialize`, from each controller's constructor): 8-byte feature report
  `00 D0 DA 21 00 00 00 00` (colour init, command 33), then 50 ms.
- **Frame** (`DeviceUpdateLEDs`): the colours into a 126-entry frame buffer at each LED's index,
  then:
  - V2: one 257-byte output report `00 D0 DA 0B` + 126 little-endian R5G6B5 values;
  - V3: one 2046-byte output report `04 D1 DA 0B` + the same values;
  - V1: five 129-byte output reports `00 D0 DA 0B <slave> <register>` (parts 0/0, 0/96, 1/0,
    1/96, 2/0) with 96 PWM register bytes each (red at the LED's register from `pwm_mem_map`,
    green + 0x10, blue + 0x20) and a CRC-16/CCITT (init 0) of bytes 0-126, little-endian.
    `key_code_limit` is always 116 in OpenRGB, so part 4 is sent for the One too.
  OpenRGB has no keepalive here; Lightloom skips unchanged frames and resends after 1 s.
- **Shutdown**: OpenRGB has no exit step. Lightloom sends the reset-all colour command, 8-byte
  feature report `00 D0 DA 20 00 00 00 00` (command 32, `WOOTING_RESET_ALL_COMMAND` in
  OpenRGB's header, never sent by OpenRGB). Wooting's RGB SDK sends exactly this as
  `wooting_rgb_reset_rgb()` to give every key its profile colour back. It is a volatile
  colour command (nothing is stored), but it is the one packet here that OpenRGB never sends:
  check it on hardware first.

## Safety

Only commands 33 (colour init), 11 (raw colours) and 32 (reset-all colours) are sent; a test
checks every report. OpenRGB's other commands (`SINGLE_COLOR` 30, `SINGLE_RESET` 31) are unused.
No analog, actuation, profile or settings command exists in these files.

## Untested

Every product id. Open question: confirm on a keyboard that command 32 restores the profile
colours (and nothing else) on V1, V2 and V3 firmware.
