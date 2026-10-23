<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ASUS ROG Throne headset stands (`0B05:18D9`, `18C5`, `1994`)

Plugin: `plugins/asus-aura-headset-stand/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file below carries
`SPDX-License-Identifier: GPL-2.0-or-later`.

| File (under `Controllers/AsusAuraUSBController/`) | License | Authors (header) | Taken |
|---|---|---|---|
| `AsusAuraUSBControllerDetect.cpp` | GPL-2.0-or-later | none named | Product ids, names, `REGISTER_HID_DETECTOR_I(..., 0)` |
| `AsusAuraHeadsetStandController/AsusAuraHeadsetStandController.cpp`, `.h` | GPL-2.0-or-later | Mola19, 06 Apr 2021 | `UpdateLeds` (two reports). `UpdateDevice`, `SaveMode`, `GetVersion` read but not used |
| `AsusAuraHeadsetStandController/RGBController_AsusAuraHeadsetStand.cpp`, `.h` | GPL-2.0-or-later | Mola19, 06 Apr 2021 | Zones ("Underglow", 17 LEDs; "Logo", 1), LED names, Direct path |

## Products

| PID | OpenRGB name | Validate |
|---|---|---|
| `18D9` | ASUS ROG Throne | interface 0, any collection |
| `18C5` | ASUS ROG Throne QI | same |
| `1994` | ASUS ROG Throne QI GUNDAM | same |

## Protocol

Two 65-byte output reports per frame, report id `0x00`:
`00 C0 81 00 00` + 15 x `00 R G B` (LEDs 0..14), then `00 C0 81 01 00` + 3 x `00 R G B` (LEDs
15..17). LED order: "Underglow LED 0" .. 16, then "Logo LED". HueDeck grid 17 x 2: the underglow
on row 0, the logo centred on row 1. No start command (OpenRGB's `GetVersion`, `00 12 00` + read,
is information only). HueDeck skips unchanged frames, resending after 1 s.

**Shutdown**: OpenRGB has no exit step; the other modes are hardware effects (`00 51 28`), so
HueDeck sets the LEDs black.

## Safety: left out

`SaveMode` (`00 50 55`) and `UpdateDevice` (`00 51 28`): never sent. The Qi charger and the USB /
audio functions of the stand are not touched.

## Untested

All three product ids.
