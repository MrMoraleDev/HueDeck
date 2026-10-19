<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Apex 3 and Apex 3 TKL

Plugin: `plugins/steelseries-apex-3/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesApex3Controller/SteelSeriesApex3Controller.cpp`, `.h` (Chris M (Dr_No), 23 Feb 2022)
- `Controllers/SteelSeriesController/SteelSeriesApex3Controller/RGBController_SteelSeriesApex3.cpp`, `.h` (Chris M (Dr_No), 23 Feb 2022)
- `Controllers/SteelSeriesController/SteelSeriesApexTZoneController/SteelSeriesApexTZoneController.cpp`, `.h` (Edbgon, 06 Oct 2021)
- `Controllers/SteelSeriesController/SteelSeriesApex8ZoneController/SteelSeriesApex8ZoneController.cpp`, `.h` (Chris M (Dr_No), 23 Feb 2022; Paul K. Gerke, 27 Oct 2022)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

| Product id | OpenRGB name | Controller | Endpoint (OpenRGB detector) | Detect.cpp line |
|---|---|---|---|---|
| `1038:161A` | SteelSeries Apex 3 | ApexTZone (10 zones) | interface 3, any collection (`_I`) | 543 |
| `1038:1622` | SteelSeries Apex 3 TKL | Apex8Zone (8 zones) | interface 1, usage page `0xFFC0`, usage `0x0001` (`_IPU`) | 544 |

`Validate()` uses `device.productId()` to apply the right line; the host binds the product id
before it calls `Validate()` (`crates/lightloom-plugin-host/src/session.rs`, steps 2 and 3). An
unknown product id accepts no endpoint.

## Protocol

LEDs: one linear zone "Keyboard", LEDs "LED 0" .. "LED n-1", one grid row. Output reports, report
id `0x00`.

OpenRGB's switch to Direct (`RGBController_SteelSeriesApex3::DeviceUpdateMode`) compares
`color_mode` (`MODE_COLORS_PER_LED` = 1) with the flag `MODE_FLAG_HAS_PER_LED_COLOR` (`1 << 5`);
they never match, so it calls `SetColor(modes[active].colors, mode, brightness)` with the Direct
mode's empty colour list. Lightloom reproduces that call in `Initialize()`.

**Apex 3 (`SteelSeriesApexTZoneController::SetColor`)**, 33-byte reports, one buffer sent twice:

1. `00 0A 00 <brightness>` (brightness = Direct's maximum, `0x64`);
2. the same buffer with byte 1 = `0x0B` and the ten colours R G B from byte 3 (with no colours,
   as in the switch to Direct, byte 3 keeps the brightness).

Both reports go out on every update.

**Apex 3 TKL (`SteelSeriesApex8ZoneController::SetColor`)**, 65-byte reports:

1. `00 <0x21 + mode> FF` then the eight colours R G B from byte 3 (mode 0 = Direct);
2. `00 23 <brightness>` (`0x10`, Direct's maximum) only when it differs from the last one sent;
   `current_brightness` is never initialised in OpenRGB, so it is sent with the first update.

Unchanged frames are resent after 1 s only. Lightloom's colours already carry the user's
brightness, so the brightness byte stays at the maximum.

**Shutdown.**
- Apex 3 TKL: OpenRGB's "Rainbow Wave" mode, `00 22 FF` (mode 1), the keyboard's power-on
  lighting according to the notes in `SteelSeriesApex8ZoneController.h`.
- Apex 3: OpenRGB has no onboard-effect command for it: the ten zones are set black.

## Left out

- `SteelSeriesApexTZoneController::Save()` (`00 06 00 08`, `00 09`): a write to the keyboard's
  memory; never sent.
- `SteelSeriesApex8ZoneController::GetBrightness()` (`00 A3` + read): unused by OpenRGB's
  RGBController.
- Key functions, macros: never touched (OpenRGB has none here).

## Untested

Both product ids.
