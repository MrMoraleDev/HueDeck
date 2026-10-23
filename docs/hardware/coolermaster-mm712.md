<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Cooler Master MM712

Plugin: `plugins/coolermaster-mm712/`. Untested on hardware (`tested` is empty). The other
Cooler Master devices are listed in [coolermaster-masterkeys.md](coolermaster-masterkeys.md).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB):

| File | License | Taken |
|---|---|---|
| `Controllers/CoolerMasterController/CoolerMasterControllerDetect.cpp` | GPL-2.0-or-later | Product id and name, detector macro (interface 3, usage page `0xFF0A`, usage 2) |
| `Controllers/CoolerMasterController/CMMM712Controller/CMMM712Controller.cpp`, `.h` (Chris M (Dr_No), 14 Feb 2021; Frans Meulenbroeks, 08 Dec 2024) | GPL-2.0-or-later | `SendInitPacket`, `SetDirectMode`, `SetLedsDirect`; the save and mode commands (to leave them out) |
| `Controllers/CoolerMasterController/CMMM712Controller/RGBController_CMMM712Controller.cpp`, `.h` (same authors) | GPL-2.0-or-later | One LED ("Logo LED"), Direct mode, brightness forced to 255 |
| `Controllers/CoolerMasterController/CMMM712Controller/MM712protocol.txt` | **no license header** | **not used** |

## Product

| Product id | OpenRGB name | Detect.cpp line |
|---|---|---|
| `2516:0169` | Cooler Master MM712 | 424 |

`REGISTER_HID_DETECTOR_IPU(..., 3, 0xFF0A, 2)`. One LED, a 1 x 1 grid.

## Protocol

65-byte output reports, report id `0x00`.

- **Start**: `44 81 02` + an input report read (250 ms) (`SendInitPacket`), then `5A 81 02`
  (direct mode on: OpenRGB's switch to Direct, `SetDirectMode(true)`).
- **Every frame** (`SetLedsDirect`): `5A 81 02`, then `5A 81 03 R G B`; no reads. OpenRGB resends
  "direct on" before every colour because its `current_mode` only ever holds the mode read from
  the mouse, never Direct. HueDeck skips an unchanged frame and resends it after 1 s.
- **Shutdown**: `5A 81 01` (`SetDirectMode(false)`, what OpenRGB sends first when leaving Direct):
  the mouse returns to its own mode.

## Left out

- `54 81 01` (`SaveStatus`; in OpenRGB even sent with `0x54` as the report id): saves. Never sent.
- `4C 81 04 ...` (mode parameters), `4C 81 08 <mode>` (select mode), `44 81 01 <profile>`
  (`SetProfile`): mode / profile writes. Never sent. OpenRGB's constructor also reads the mode
  (`4C 81 07`), writes the same mode back (`4C 81 08`) and reads its colour (`4C 81 03`); that round
  trip only fills OpenRGB's user interface and is left out.
- DPI, polling rate, buttons.

## Untested

The product id. Open question: whether direct mode without the constructor's mode round trip
behaves the same (it should: OpenRGB sends `5A 81 02` before every colour anyway).
