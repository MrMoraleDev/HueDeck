<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Apex (original) / Apex Fnatic / Apex 350

Plugin: `plugins/steelseries-apex-old/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesOldApexController/SteelSeriesOldApexController.cpp`, `.h` (based on findings in ApexCtl by Audrius/tuxmark5 et al.; David Lee (RAMChYLD), 15 Nov 2020; based on work by B Horn (bahorn), 13 May 2020)
- `Controllers/SteelSeriesController/SteelSeriesOldApexController/RGBController_SteelSeriesOldApex.cpp`, `.h` (same)
- `Controllers/SteelSeriesController/color32.h` (David Lee (RAMChYLD), 15 Nov 2020)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

| Product id | OpenRGB name | Endpoint (OpenRGB detector) | Detect.cpp line |
|---|---|---|---|
| `1038:1202` | SteelSeries Apex (OG)/Apex Fnatic | interface 0, any collection (`_I`) | 559 |
| `1038:1206` | SteelSeries Apex 350 | interface 0, any collection (`_I`) | 560 |

## Protocol

LEDs: five one-LED zones in packet order, one grid row: QWERTY, TenKey, FunctionKeys, MXKeys,
Logo.

**Start.** OpenRGB's only mode, Direct, has no command of its own.

**Every frame** (`SetColorDetailed`): one 33-byte output report, report id `0x00` then
`07 00` ("all zones") and per zone R G B A from byte 2 of the buffer (A = the Direct mode value
`0x08`, which OpenRGB says acts as the brightness). Unchanged frames are resent after 1 s only.

**Shutdown.** No exit step and no other mode in OpenRGB: all zones black (A stays `0x08`).

**Exit modes** (Lightloom extension, [save-shutdown.md](../host-api/save-shutdown.md)): `keep`
sends the last frame once more with the report of a frame, `off` the same black; `restore` (and
SignalRGB's call without a mode) and suspend are the shutdown above. Nothing is stored. OpenRGB
sends no keepalive, so the device is expected to hold the colours until it loses power or other
software takes it over (unverified on hardware).

## Left out

Nothing lighting-related; no key or macro command exists in this driver.

## Untested

Both product ids.
