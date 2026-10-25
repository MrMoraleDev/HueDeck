<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Apex M750

Plugin: `plugins/steelseries-apex-m/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesApexMController/SteelSeriesApexMController.cpp`, `.h` (Florian Heilmann (FHeilmann), 12 Oct 2020)
- `Controllers/SteelSeriesController/SteelSeriesApexBaseController.cpp`, `.h` (Florian Heilmann (FHeilmann), 19 Oct 2020)
- `Controllers/SteelSeriesController/SteelSeriesApexController/RGBController_SteelSeriesApex.cpp`, `.h` (Eric Samuelson (edbgon), 05 Jul 2020)
- `Controllers/SteelSeriesController/SteelSeriesApexRegions.h` (Joseph East (dripsnek), 2021)
- `RGBController/RGBControllerKeyNames.cpp` (Chris M (Dr_No), 25 Jan 2022): key names
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

Not used: `SteelSeriesApexController/SteelSeriesApexController.cpp` / `.h` (the newer Apex
driver), whose header says it is based on SignalRGB plugins.

## Products

| Product id | OpenRGB name | Endpoint (OpenRGB detector) | Detect.cpp line |
|---|---|---|---|
| `1038:0616` | SteelSeries Apex M750 | interface 2, any collection (`REGISTER_HID_DETECTOR_I`) | 558 |

## LEDs

OpenRGB's Apex RGBController has 112 LEDs (`led_names` in `SteelSeriesApexRegions.h`) on the
22 x 6 `MATRIX_MAP_ANSI`; the M750 has no SKU (`GetSerial()` answers only for the APEX protocol),
so no regional patch applies. The wire table `keys_m` (132 slots, 22 x 6) names, per slot, the
OpenRGB LED whose colour goes there; 104 slots are used.

Lightloom's LEDs are those 104 keys, in slot order, named after OpenRGB's LED (without the
"Key: " prefix) and placed at that LED's matrix cell: grid `[22, 6]`.

**Numeric pad: deliberate deviation from OpenRGB.** `keys_m` was written against an older
`led_names` list, from before the five Japanese keys (`KEY_JP_RO` .. `KEY_JP_MUHENKAN`, now
89..93) were inserted ahead of the numeric pad. Its values 89..105 are the numeric-pad keys of
that old list, exactly as the slot labels in `keys_m` say (NUMLK=89, #/=90, #*=91, #-=92, #+=93,
#ENTR=94, #1..#9=95..103, #0=104, #.=105); in today's list those keys are 94..110. OpenRGB
therefore sends, for example, the colour of "Number Pad 5" (today's 104) to the slot labelled #0,
and the colours of the Japanese keys to NUMLK, #/, #*, #-, #+.

Lightloom feeds each of those 17 slots from the key its label names (value + 5): the slot labelled
#0 (slot 19) takes the "Number Pad 0" cell `[18, 5]`, and so on. The wire format and the slot
order are unchanged; only which canvas cell feeds a numeric-pad slot differs from OpenRGB.
`tests/packets.test.mjs` checks every numeric-pad slot. The reference model that made
`fixtures/expected.json` applies the same correction and asserts that each corrected LED's name
matches its `keys_m` label.

## Protocol

513-byte **feature** reports, report id `0x00`.

**Start** (`EnableLEDControl()`, called by OpenRGB's constructor): one buffer, not cleared
between the three reports:

1. `00 00 00 00 01 00 85`
2. `00 00 00 00 03 01 00 FF`
3. `00 00 00 00 01 00 85 FF` (byte 7 keeps the `FF` of the second report)

OpenRGB's Direct mode sends nothing else (`SetMode` is empty on the M750; Direct has no
brightness).

**Every frame** (`SetLEDsDirect`): `00 00 00 01 8E 01 03 06 16`, then 132 slots of R G B at
offset `9 + slot * 3`; slots without a key get `FF 32 00`. Unchanged frames are resent after 1 s
only.

**Shutdown.** OpenRGB has no exit step, and its "Onboard" mode sends nothing on the M750: all keys
are set black.

**Exit modes** (Lightloom extension, [save-shutdown.md](../host-api/save-shutdown.md)): `keep`
sends the last frame once more with the reports of a frame, `off` the same black; `restore` (and
SignalRGB's call without a mode) and suspend are the shutdown above. Nothing is stored. OpenRGB
sends no keepalive, so the device is expected to hold the colours until it loses power or other
software takes it over (unverified on hardware).

## Left out

Nothing lighting-related. No profile, macro or key function command is sent.

## Untested / unknown

- The product id.
- The three start reports of `EnableLEDControl()` (`01 00 85`, `03 01 00 FF`, `01 00 85 FF`,
  513-byte feature reports): OpenRGB does not explain them, and feature reports on a keyboard
  can carry configuration. Verify on hardware (keys, macros and profiles unchanged afterwards)
  before marking the product tested.
- The numeric-pad correction above (the labels make it very likely; check one key).
