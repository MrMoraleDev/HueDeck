<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Rival 3 (wired)

Plugin: `plugins/steelseries-rival-3/`. Untested on hardware (`tested` is empty). The Rival 3
Wireless uses another protocol: see [steelseries-aerox.md](steelseries-aerox.md).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesRival3Controller/SteelSeriesRival3Controller.cpp`, `.h` (B Horn (bahorn), 29 Aug 2021)
- `Controllers/SteelSeriesController/SteelSeriesRival3Controller/RGBController_SteelSeriesRival3.cpp`, `.h` (B Horn (bahorn), 29 Aug 2021)
- `Controllers/SteelSeriesController/SteelSeriesMouseController/SteelSeriesMouseController.cpp`, `.h` (Chris M (Dr_No), 09 Jun 2022)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

| Product id | OpenRGB name | Endpoint (OpenRGB detector) | Detect.cpp line |
|---|---|---|---|
| `1038:1824` | SteelSeries Rival 3 (Old Firmware) | interface 3, any collection | 511 |
| `1038:184C` | SteelSeries Rival 3 | interface 3, any collection | 512 |

## Protocol

Output reports, report id `0x00`. LEDs (table `rival_3`): Front = zone 1, Middle = 2, Rear = 3,
Logo = 4 (one grid row, this order).

**Start** (switch to Direct, `DeviceUpdateMode`: `SetLightEffectAll(value - speed)` = `0x04 - 0`):
`00 06 00 04` (4 bytes).

**Every frame** (`DeviceUpdateLEDs`): per LED `00 05 00 <zone> R G B <brightness>` (8 bytes),
brightness being the Direct mode's `STEELSERIES_MOUSE_BRIGHTNESS_MAX` (`0x64`; Lightloom's colours
already carry the user's brightness); then `DeviceUpdateMode` again: `00 06 00 04`. Unchanged
frames are resent after 1 s only.

**Shutdown.** OpenRGB has no exit step; its other modes (Breathing, Spectrum Cycle, Rainbow
Breathing) are effects, not the mouse's saved lighting: every LED is set black in Direct.

**Exit modes** (Lightloom extension, [save-shutdown.md](../host-api/save-shutdown.md)): `keep`
sends the last frame once more with the reports of a frame (never the save command `0x09`), `off`
the same black; `restore` (and SignalRGB's call without a mode) and suspend are the shutdown
above. Nothing is stored. OpenRGB sends no keepalive, so the device is expected to hold the
colours until it loses power or other software takes it over (unverified on hardware).

## Left out

- `Save()` (`00 09`, `SteelSeriesMouseController::Save`): never sent.
- `GetFirmwareVersion()` (`00 10 00` + read): only fills OpenRGB's device info.
- The effect modes. DPI and polling rate: never touched.

## Untested

Both product ids.
