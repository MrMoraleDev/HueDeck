<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Rival (older models): Rival 100/105/106/110, 300, 600, 700/710

Plugin: `plugins/steelseries-rival/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesRivalController/SteelSeriesRivalController.cpp`, `.h` (B Horn (bahorn), 13 May 2020)
- `Controllers/SteelSeriesController/SteelSeriesRivalController/RGBController_SteelSeriesRival.cpp`, `.h` (B Horn (bahorn), 13 May 2020)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

All `REGISTER_HID_DETECTOR_I(..., 0)`: any collection of interface 0.

| Product id | OpenRGB name | Sub-protocol | Detect.cpp line |
|---|---|---|---|
| `1038:1702` | SteelSeries Rival 100 | RIVAL_100 | 489 |
| `1038:170C` | SteelSeries Rival 100 DotA 2 Edition | RIVAL_100 | 490 |
| `1038:1814` | SteelSeries Rival 105 | RIVAL_100 | 491 |
| `1038:1816` | SteelSeries Rival 106 | RIVAL_100 | 492 |
| `1038:1729` | SteelSeries Rival 110 | RIVAL_100 | 493 |
| `1038:1710` | SteelSeries Rival 300 (also registered as "Rival 300 Black Ops Edition", line 501: same id, the first registration wins) | RIVAL_300 | 494 |
| `1038:1714` | Acer Predator Gaming Mouse (Rival 300) | RIVAL_300 | 495 |
| `1038:1394` | SteelSeries Rival 300 CS:GO Fade Edition | RIVAL_300 | 496 |
| `1038:1716` | SteelSeries Rival 300 CS:GO Fade Edition (stm32) | RIVAL_300 | 497 |
| `1038:171A` | SteelSeries Rival 300 CS:GO Hyperbeast Edition | RIVAL_300 | 498 |
| `1038:1392` | SteelSeries Rival 300 Dota 2 Edition | RIVAL_300 | 499 |
| `1038:1718` | SteelSeries Rival 300 HP Omen Edition | RIVAL_300 | 500 |
| `1038:1724` | SteelSeries Rival 600 | RIVAL_600 | 505 |
| `1038:172E` | SteelSeries Rival 600 Dota 2 Edition | RIVAL_600 | 506 |
| `1038:1700` | SteelSeries Rival 700 | RIVAL_700 | 509 |
| `1038:1730` | SteelSeries Rival 710 | RIVAL_700 | 510 |

**Held back** (OpenRGB lists them, Lightloom's plugin does not claim them; see "Held back: Rival
650" below):

| Product id | OpenRGB name | Sub-protocol | Detect.cpp line |
|---|---|---|---|
| `1038:172B` | SteelSeries Rival 650 | RIVAL_650 | 507 |
| `1038:1726` | SteelSeries Rival 650 Wireless | RIVAL_650 | 508 |

The **Rival 5** (OpenRGB's separate `SteelSeriesRival5Controller`) is not ported at all: OpenRGB
says twice that "The Rival 5 saves the current state automatically", so its per-update colour
report may be a memory write on every frame, which Lightloom's rules forbid.

(The Rival 310 ids on lines 502-504 use the Sensei controller: see
[steelseries-sensei.md](steelseries-sensei.md).)

## LEDs (RGBController_SteelSeriesRival::SetupZones)

LED order is OpenRGB's, which is also the order of the packets. Grid: the one-LED zones Logo and
Scroll Wheel share row 0; the six-LED "Mouse" zone is row 1.

| Model | LEDs (name = zone id sent) |
|---|---|
| RIVAL_100 | Logo = 0 |
| RIVAL_300, RIVAL_700 | Logo = 0, Scroll Wheel = 1 |
| RIVAL_600 | Logo = 0x01, Scroll Wheel = 0x00, Left top = 0x02, Left mid = 0x04, Left bottom = 0x06, Right top = 0x03, Right mid = 0x05, Right bottom = 0x07 |

## Protocol

`send_usb_msg(data, n)` writes an output report of n + 1 bytes: report id `0x00`, then the data.

**Start** (OpenRGB's switch to Direct: `SetLightEffectAll(STEELSERIES_RIVAL_EFFECT_DIRECT = 0x01)`):

- RIVAL_100: `00 07 00 01 00 00 00 00 00 00` (10 bytes).
- RIVAL_300: `00 07 01 01 ...` and `00 07 02 01 ...` (zone id + 1).
- RIVAL_600, RIVAL_700: nothing.

**Every frame** (`DeviceUpdateLEDs`, every LED in order, `SetColor(led.value, R, G, B)`):

- RIVAL_100: `00 05 00 R G B 00 00 00 00` (10 bytes).
- RIVAL_300: `00 08 <zone+1> R G B 00 00 00 00`.
- RIVAL_600: one 7-byte output report written as is, `1C 27 00 <1 << zone> R G B` (hidapi takes
  `0x1C` as the report id).
- RIVAL_700: one 579-byte **feature** report per LED (`SetRival700Color`): report id `0x00`, then
  `05 00 <zone> R G B 00 00 00 00 00 <zone> 01` and zeros.

Lightloom skips a frame whose colours did not change, resending them after 1 s.

**Shutdown.** OpenRGB has no exit step and no "back to onboard lighting" command for these mice
(its other mode, Pulsate, is an effect): every LED is set black in Direct.

**Exit modes** (Lightloom extension, [save-shutdown.md](../host-api/save-shutdown.md)): `keep`
sends the last frame once more with the reports of a frame (never the save command `0x09`), `off`
the same black; `restore` (and SignalRGB's call without a mode) and suspend are the shutdown
above. Nothing is stored. OpenRGB sends no keepalive, so the device is expected to hold the
colours until it loses power or other software takes it over (unverified on hardware).

## Left out

- `SaveMode()` (`00 09 ...`, "Saves to the internal configuration"): a write to the mouse's
  memory; never sent.
- `GetFirmwareVersion()` (`00 10 00` + read, RIVAL_300 / RIVAL_700): only fills OpenRGB's device
  info.
- The Pulsate mode (an effect, not per-LED lighting). DPI and polling rate: Lightloom sends no
  such command (the Rival 650 reports below, which may be one, are held back).

## Held back: Rival 650 / 650 Wireless

OpenRGB's driver (`SetRival650Color`, `SetLightEffectAll`) does, for these two ids:

- start: eight times `00 00 00 01 00 00 00 00 00 00` (the switch in `SetLightEffect` has no
  RIVAL_650 case, so bytes 0 and 1 stay 0: command `0x00`, not explained);
- every update, for each of the 8 LEDs (zone ids 0x10..0x17; Logo 0x11, Scroll Wheel 0x10,
  Left 1/2/3 0x12/0x14/0x16, Right 1/2/3 0x13/0x15/0x17), four 61-byte output reports:
  `00 03 00 00 00 30 00 10 27 ... [0x16]=01 [0x1E]=04 [0x1F..0x21]=R G B [0x22]=FF [0x27]=FF
  [0x29]=54 [0x2C]=FF [0x2D]=54 [0x2E..0x30]=R G B [0x31]=56` (offsets in the 60-byte data),
  then `03 00 30 00 2C`, then `05 00 <zone> FF 00 00 00 00 5C`, then `1C 00 55 00 46`:
  32 reports per update.

Neither OpenRGB nor anything else at hand explains opcodes `0x03`, `0x1C` or the start command
`0x00`. In other Rival protocols `0x03` is the sensitivity (DPI) command, and the first two
reports read like "write 0x2C bytes at offset 0x30" of some buffer or memory (the data runs from
0x06 to 0x31). Sending that up to 30 times a second could change a non-lighting setting or
write persistent memory, which Lightloom's rules forbid. So the plugin does not claim these ids
(they stay in `fixtures/expected.json` with a `held_back` reason, and a test checks that the
plugin sends nothing for them). Open safety question: what do `0x03` / `0x1C` do, and is any of
it stored in flash? Answer that on hardware (with a USB capture of SteelSeries' own software)
before porting them.

## Untested

Every product id.
