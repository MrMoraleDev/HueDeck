<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ASUS ROG Balteus mousemats (`0B05:1891`, `1890`)

Plugin: `plugins/asus-aura-mousemat/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file below carries
`SPDX-License-Identifier: GPL-2.0-or-later`.

| File (under `Controllers/AsusAuraUSBController/`) | License | Authors (header) | Taken |
|---|---|---|---|
| `AsusAuraUSBControllerDetect.cpp` | GPL-2.0-or-later | none named | Product ids, names, `REGISTER_HID_DETECTOR_PU(..., 0xFF06, 1)` |
| `AsusAuraMousematController/AsusAuraMousematController.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 10 Jan 2022 | `UpdateLeds` (report layout). `UpdateDevice`, `SaveMode`, `GetVersion` read but not used |
| `AsusAuraMousematController/RGBController_AsusAuraMousemat.cpp` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 10 Jan 2022 | Zone ("Mousemat", 15 LEDs), LED names, Direct path |
| `AsusAuraMousematController/RGBController_AsusAuraMousemat.h` | GPL-2.0-or-later | Mola19, 06 Apr 2021 | Mode constants (to know what is left out) |

## Products

| PID | OpenRGB name | Validate |
|---|---|---|
| `1891` | ASUS ROG Balteus | usage page `0xFF06`, usage 1 (any interface) |
| `1890` | ASUS ROG Balteus Qi | same |

## Protocol

One 65-byte output report per frame, report id `0xEE`:
`EE C0 81 00 00` + 15 x `00 R G B` (LEDs "Mousemat LED 0" .. 14 in order; HueDeck: one row of 15
cells). No start command (OpenRGB reads the firmware version, `EE 12 00` + read, for information
only; Direct needs no mode command). HueDeck skips unchanged frames, resending after 1 s.

**Shutdown**: OpenRGB has no exit step; the other modes are hardware effects (`EE 51 28`) and the
mat's saved effect cannot be selected without one, so HueDeck sets the LEDs black.

## Safety: left out

`SaveMode` (`EE 50 03`, save to the mat) and `UpdateDevice` (`EE 51 28`, hardware effects): never
sent.

## Untested

Both product ids.
