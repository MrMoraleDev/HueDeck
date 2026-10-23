<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Cooler Master ARGB monitors (GM27-FQS, GM34-CWQ) and GD160 ARGB gaming desk

Plugin: `plugins/coolermaster-monitor-desk/`. Untested on hardware (`tested` is empty). The other
Cooler Master devices are listed in [coolermaster-masterkeys.md](coolermaster-masterkeys.md).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB):

| File | License | Taken |
|---|---|---|
| `Controllers/CoolerMasterController/CoolerMasterControllerDetect.cpp` | GPL-2.0-or-later | Product ids and names, detector macro (interface 0, usage page `0xFF00`, usage 1) |
| `Controllers/CoolerMasterController/CMMonitorController/CMMonitorController.cpp`, `.h` (Morgan Guimard (morg), 18 Sep 2023) | GPL-2.0-or-later | `SetSoftwareModeEnabled`, `SendDirect`, packet sizes; the effect / custom-colour commands (to leave them out) |
| `Controllers/CoolerMasterController/CMMonitorController/RGBController_CMMonitorController.cpp`, `.h` (same author) | GPL-2.0-or-later | 47 LEDs, Direct is the first mode, effect modes flagged "automatic save" |
| `Controllers/CoolerMasterController/CMGD160Controller/CMGD160Controller.cpp`, `.h` (Logan Phillips (Eclipse), 16 Oct 2025) | GPL-2.0-or-later | `SetControlMode` (software / hardware), `SendColorData`, sides, sizes; `ResetDevice` (to leave it out) |
| `Controllers/CoolerMasterController/CMGD160Controller/RGBController_CMGD160Controller.cpp`, `.h` (same author) | GPL-2.0-or-later | Front and back zones of 96 LEDs, Direct packet parameters (`0x07, 0x01, 0xFF, true`) |

## Products

| Product id | OpenRGB name | LEDs / grid | Detect.cpp line |
|---|---|---|---|
| `2516:01BB` | Cooler Master GM27-FQS ARGB Monitor | 47, one row (47 x 1) | 445 |
| `2516:01BF` | Cooler Master GM34-CWQ ARGB Monitor | 47, one row (47 x 1) | 446 |
| `2516:01A9` | Cooler Master GD160 ARGB Gaming Desk | Front LED 1-96 (row 0), Back LED 1-96 (row 1): 96 x 2 | 451 |

All `REGISTER_HID_DETECTOR_IPU(..., 0, 0xFF00, 1)`.

## Protocol

65-byte output reports, report id `0x00`, no reads. The monitor has one "side" (`0x02`), the desk
two (`0x01` front, `0x02` back).

**Start.** Nothing: Direct is OpenRGB's first mode and switching to it sends nothing.

**Every frame.** The first one turns software mode on, per side: `80 07 02 <side> 00 01`. Then,
per side, the colours (R, G, B per LED) in a 436-byte buffer, sent as seven packets:

- `00 07 02 <side> <a> <b>` + buffer bytes 0-57, with `<a> <b>` = `01 80` on the monitors
  (`SendDirect`) and `01 FF` on the desk (`SendColorData(colors, 0x07, 0x01, 0xFF, true)`);
- `01` .. `05` + 63 buffer bytes each;
- `86` + the last 63 bytes.

HueDeck skips an unchanged frame and resends it after 1 s (colours only; software mode stays on).

**Shutdown.** OpenRGB has no exit step; leaving Direct for any other mode starts with software
mode off. HueDeck sends just that, `80 07 02 <side> 00 00` per side ("False enables hardware
mode"), and the device shows its own stored effect again.

## Left out

- Hardware effects (`80 0B 02 ...`, `80 0F 02 ...`, flagged "automatic save" by OpenRGB) and the
  "Custom" mode (`10 02 <side> 80 ...`), which stores colours in the device. Never sent.
- The GD160's `ResetDevice` at discovery (`80 11`, `80 0B 01 02`, `80 18 01 02`, "in case it
  somehow landed in a bad / unresponsive state"): what these commands do is not documented, so
  HueDeck does not send them. If the desk ignores direct colours on hardware, this is the first
  thing to look at.

## Untested

All three product ids.
