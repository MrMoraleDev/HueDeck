<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Rival 310 / Sensei TEN / Sensei 310 ("Sensei" protocol)

Plugin: `plugins/steelseries-sensei/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesSenseiController/SteelSeriesSenseiController.cpp`, `.h` ("Based on SteelSeries Rival controller", B Horn (bahorn), 13 May 2020)
- `Controllers/SteelSeriesController/SteelSeriesSenseiController/RGBController_SteelSeriesSensei.cpp`, `.h` (same)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

All `REGISTER_HID_DETECTOR_I(..., 0)` with `DetectSteelSeriesSensei`: any collection of
interface 0.

| Product id | OpenRGB name | Detect.cpp line |
|---|---|---|
| `1038:1720` | SteelSeries Rival 310 | 502 |
| `1038:171E` | SteelSeries Rival 310 CS:GO Howl Edition | 503 |
| `1038:1736` | SteelSeries Rival 310 PUBG Edition | 504 |
| `1038:1832` | SteelSeries Sensei TEN | 515 |
| `1038:1834` | SteelSeries Sensei TEN CS:GO Neon Rider Edition | 516 |
| `1038:1722` | SteelSeries Sensei 310 | 517 |

## Protocol

LEDs: zone 0 "Logo", zone 1 "Scroll Wheel" (one grid row).

**Start.** Direct has no command of its own (`DeviceUpdateMode` only calls `DeviceUpdateLEDs`):
nothing is sent.

**Every frame** (`DeviceUpdateZoneLEDs(0)`, then `(1)`; Direct -> `SetColor(zone, R, G, B)`): one
66-byte output report per zone, report id `0x00` then the 65-byte buffer
`5B 00 <zone> 00 ...` with `[0x13] = 01` (static colour flag), `[0x1B] = 01` (number of colours),
`[0x1C..0x1E] = R G B` and `[0x1F..0x21] = R G B` (the original software repeats the colour;
offsets in the buffer, after the report id). Unchanged frames are resent after 1 s only.

**Shutdown.** OpenRGB has no exit step; its other modes (Breathing, Rainbow) are effects, not the
mouse's own saved lighting: both zones are set black in Direct.

## Left out

- `Save()` (`00 59 ...`, "Saves to the internal configuration"): never sent.
- Breathing and Rainbow effects (`SetLightEffect`, command 0x5B with durations).
- DPI: OpenRGB has no such command here and HueDeck sends none.

## Untested

Every product id.
