<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Cooler Master MasterMouse: MM530, MM711, MM720, MM730

Plugin: `plugins/coolermaster-mastermouse/`. Untested on hardware (`tested` is empty). The other
Cooler Master devices are listed in [coolermaster-masterkeys.md](coolermaster-masterkeys.md).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB):

| File | License | Taken |
|---|---|---|
| `Controllers/CoolerMasterController/CoolerMasterControllerDetect.cpp` | GPL-2.0-or-later | Product ids and names, detector macro (interface 1, usage page `0xFF00`, usage 1) |
| `Controllers/CoolerMasterController/CMMMController/CMMMController.cpp`, `.h` (Chris M (Dr_No), 14 Feb 2021; Dracrius, 12 Mar 2022) | GPL-2.0-or-later | Byte offsets per model, `SendInitPacket`, `SendUsingZonesPacket`, `SendUpdate`, `SendApplyPacket`, `SetLedsDirect`, the save command (to leave it out) |
| `Controllers/CoolerMasterController/CMMMController/RGBController_CMMMController.cpp`, `.h` (same authors) | GPL-2.0-or-later | Direct mode values (0xB0, speed 0, brightness 0xFF), LEDs, `applyBrightness` |
| `Controllers/CoolerMasterController/CMMM711Controller/CMMM711Controller.cpp`, `.h`, `RGBController_CMMM711Controller.cpp`, `.h` (Chris M (Dr_No), 14 Feb 2021) | GPL-2.0-or-later | MM711 init, `SetLedsDirect`, LEDs |

## Products

| Product id | OpenRGB name | Model | LEDs (OpenRGB order) | Detect.cpp line |
|---|---|---|---|---|
| `2516:0065` | Cooler Master MM530 | 5xx (command `0x2C`) | Scroll Wheel, Buttons, Logo | 421 |
| `2516:0101` | Cooler Master MM711 | MM711 | Scroll Wheel LED, Logo LED | 423 |
| `2516:0141` | Cooler Master MM720 | 7xx (command `0x2B`) | Scroll Wheel, Logo | 425 |
| `2516:0165` | Cooler Master MM730 | 7xx (command `0x2B`) | Scroll Wheel, Logo | 426 |

The MM531 (`0097`) line is commented out in OpenRGB's detector and is not claimed. The MM712 has
another protocol: [coolermaster-mm712.md](coolermaster-mm712.md). One linear zone, one row.

## Protocol

Every command is a 65-byte output report (report id `0x00`), followed by an input report read
(`hid_read_timeout`, 250 ms).

**Start.** The constructor's `SendInitPacket`: `41 80`. Then OpenRGB's switch to its Direct mode
(`SendUpdate(0xB0, speed 0, colour 0, brightness 0xFF)`), MM530/MM720/MM730 only:

1. `51 30 00 00 00` (`SendUsingZonesPacket`);
2. `51 <2C|2B> 00 00 B0 00 20 FF FF FF` + six zero colour bytes + `FF` from byte 17 to 64;
3. `51 28 00 00 B0` (`SendApplyPacket`, "post apply for zoned modes").

The MM711's Direct switch sends nothing in OpenRGB (its apply is commented out), so Lightloom sends
only `41 80`.

**Every frame** (`SetLedsDirect`): `51 A8 00 00` and the colours at fixed offsets:

| Model | bytes 5-7 | bytes 8-10 | bytes 11-13 |
|---|---|---|---|
| MM530 | Buttons | Logo | Scroll Wheel |
| MM720 / MM730 | Scroll Wheel | Logo | 0 (no buttons LED) |
| MM711 | Scroll Wheel | Logo | 0 |

OpenRGB scales the colours by the Direct mode's brightness, 0xFF of 0xFF: unchanged. Lightloom
skips an unchanged frame and resends it after 1 s.

**Shutdown.** OpenRGB has no exit step. Its only way back to a hardware mode rewrites the mode's
speed and colours, which Lightloom does not know, so the LEDs are set black (one `51 A8` report).
Nothing is saved: the mouse's stored lighting returns at the next power-up.

## Left out

- `50 55` (`SendSavePacket`): saves to the mouse. Never sent.
- The constructor's status reads (`52 2B/2C`, `52 A8`, `52 28`): they only fill OpenRGB's user
  interface.
- Hardware effect modes, the multilayer packet (`51 A0`), DPI, polling rate.

## Untested

All four product ids. Open questions: whether the MM711 shows the `51 A8` colours without an
explicit switch to the custom mode (OpenRGB leaves `SendApplyPacket(0xB0)` commented out);
what the mice do with the custom mode after Lightloom stops (black until the next power-up).
