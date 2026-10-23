<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ASUS Aura mice with a direct mode (`0B05`)

Plugin: `plugins/asus-aura-mouse/`. Untested on hardware (`tested` is empty).

Per-zone lighting of the ASUS ROG mice whose OpenRGB entry has a native direct mode
(`direct = true` in `AsusAuraMouseDevices.h`). The older Aura mice are not claimed (see "Held
back").

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file below carries
`SPDX-License-Identifier: GPL-2.0-or-later`.

| File (under `Controllers/AsusAuraUSBController/`) | License | Authors (header) | Taken |
|---|---|---|---|
| `AsusAuraUSBControllerDetect.cpp` | GPL-2.0-or-later | none named | Product ids, names, `REGISTER_HID_DETECTOR_IP(..., 0, 0xFF01)` |
| `AsusAuraMouseController/AsusAuraMouseController.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 23 Oct 2020 | `SendDirect` (report layout, 5 LEDs per report, zone-to-LED fan-out). `SaveMode`, `SendUpdate` and `GetVersion` are read but not used |
| `AsusAuraMouseController/AsusAuraMouseDevices.h` | GPL-2.0-or-later | Chris M (Dr_No), 11 Jul 2021 | Which mice have `direct`, their zones and `aura_mouse_led_maps` (LED count, LEDs per zone) |
| `AsusAuraMouseController/RGBController_AsusAuraMouse.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 25 Oct 2020 | Zone names and order, Direct update path (`DeviceUpdateMode` sends nothing for Direct) |

## Products

All interface 0, usage page `0xFF01` (`REGISTER_HID_DETECTOR_IP`).

| PID | OpenRGB name | Zones (LED indices) |
|---|---|---|
| `1A70`, `1A72` | ASUS ROG Gladius III Wireless AimPoint USB / 2.4Ghz | Logo (0) |
| `1A18`, `1A1A` | Asus ROG Chakram X USB / 2.4GHz | Logo (7), Scroll Wheel (8), Underglow (0-6) |
| `1977`, `1979` | Asus ROG Spatha X USB / 2.4GHz | Logo (0), Scroll Wheel (1), Underglow (2-4) |
| `1A88` | ASUS ROG Strix Impact III | Logo (0), Scroll Wheel (1) |
| `1A66`, `1A68` | ASUS ROG Keris Wireless AimPoint USB / 2.4Ghz | Logo (0) |

OpenRGB's zones are one LED each (one colour per zone, which `SendDirect` copies onto all LEDs of
the zone); HueDeck keeps that: one cell per zone, in a row.

## Protocol

65-byte output reports, report id `0x00` first.

- **Start**: nothing. (OpenRGB reads the firmware version, `00 12` + read, for its device
  information only; Direct needs no mode command.)
- **Every frame** (`SendDirect`): the zone colours are copied onto the LED list, then for every 5
  LEDs: `00 51 29 <count> 00 <first LED> R G B ...`. HueDeck skips an unchanged frame, resending
  it after 1 s.
- **Shutdown**: OpenRGB has no exit step. Its other modes are hardware effects (`00 51 28`), and
  the mouse's own saved effect cannot be selected without them, so HueDeck sets every zone black.

## Safety: left out

- `SaveMode` (`00 50 03`): saves to the mouse. Never sent.
- `SendUpdate` (`00 51 28`): hardware modes. Never sent.
- Nothing touches DPI, polling rate, buttons or power settings.

## Held back

- **The Aura mice without a direct mode** (Gladius II family, Gladius III / Core / Wireless,
  Chakram / Core, Pugio / Pugio II, Strix Impact / Impact II family, Keris / Keris Wireless, TUF
  Gaming M3 / M3 Gen II / M5): OpenRGB drives them only through hardware modes (`00 51 28` with a
  mode, brightness and colour per zone). Using the Static mode as a per-frame direct mode would
  send a mode write up to 30 times a second; nobody has checked whether these mice store it, so
  HueDeck does not claim them. (OpenRGB also registers `189E` twice, as Gladius II Wireless and
  Strix Impact II Gundam, with different interfaces.)
- **ROG Spatha X Dock**: OpenRGB opens `1979` a second time as the dock (a fake product id
  `0xFFFF`, zone "Dock", hardware modes only). HueDeck drives only the mouse on `1979`.
- The ROG Strix Evolve and ROG Spatha (wired / wireless, `185B`, `181C`, `1824`) use OpenRGB's
  older "Mouse Gen1" controller; not ported.

## Untested

Every product id.
