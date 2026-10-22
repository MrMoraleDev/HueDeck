<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Aerox 3 / 5 / 9 and Rival 3 Wireless

Plugin: `plugins/steelseries-aerox/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesAerox3Controller/SteelSeriesAerox3Controller.cpp`, `.h` (Chris M (Dr_No), 09 Jun 2022)
- `Controllers/SteelSeriesController/SteelSeriesAerox5Controller/SteelSeriesAerox5Controller.cpp`, `.h` (Bobby Quantum (BobbyQuantum), 19 May 2024)
- `Controllers/SteelSeriesController/SteelSeriesAeroxWirelessController/SteelSeriesAeroxWirelessController.cpp` (Ensar S (esensar), 09 Sep 2024; Clari (clari7744) & JAO1988 (JAO1988), 02 Oct 2026), `.h` (Ensar S (esensar), 09 Sep 2024)
- `Controllers/SteelSeriesController/SteelSeriesRival3Controller/RGBController_SteelSeriesRival3.cpp`, `.h` (B Horn (bahorn), 29 Aug 2021): the RGBController OpenRGB uses for all of these
- `Controllers/SteelSeriesController/SteelSeriesMouseController/SteelSeriesMouseController.cpp`, `.h` (Chris M (Dr_No), 09 Jun 2022)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

All `REGISTER_HID_DETECTOR_IPU(..., 3, 0xFFC0, 1)`: interface 3, usage page `0xFFC0`, usage
`0x0001`.

| Product id | OpenRGB name | Controller / proto | LEDs | Detect.cpp line |
|---|---|---|---|---|
| `1038:1838` | SteelSeries Aerox 3 Wireless | wireless / AEROX_3_WIRELESS (receiver) | Front, Middle, Rear | 474 |
| `1038:183A` | SteelSeries Aerox 3 Wireless Wired | wireless / AEROX_3_WIRELESS_WIRED | Front, Middle, Rear | 475 |
| `1038:1878` | SteelSeries Aerox 3 CS2 Dragon Lore Edition Wireless | wireless / AEROX_3_WIRELESS (receiver) | Front, Middle, Rear | 476 |
| `1038:187A` | SteelSeries Aerox 3 CS2 Dragon Lore Edition Wireless Wired | wireless / AEROX_3_WIRELESS_WIRED | Front, Middle, Rear | 477 |
| `1038:1890` | SteelSeries Aerox 3 Wireless Gen 2 | wireless / AEROX_3_GEN2_WIRELESS (receiver) | Front, Middle, Rear | 478 |
| `1038:1836` | SteelSeries Aerox 3 Wired | Aerox3 | Front, Middle, Rear | 479 |
| `1038:1852` | SteelSeries Aerox 5 Wireless | wireless / AEROX_5_WIRELESS (receiver) | Front, Middle, Rear | 480 |
| `1038:1854` | SteelSeries Aerox 5 Wireless Wired | wireless / AEROX_5_WIRELESS_WIRED | Front, Middle, Rear | 481 |
| `1038:185C` | SteelSeries Aerox 5 Destiny 2 Edition Wireless | wireless / AEROX_5_DESTINY_WIRELESS (receiver) | Front, Middle, Rear | 482 |
| `1038:185E` | SteelSeries Aerox 5 Destiny 2 Edition Wireless Wired | wireless / AEROX_5_DESTINY_WIRELESS_WIRED | Front, Middle, Rear | 483 |
| `1038:1860` | SteelSeries Aerox 5 Diablo IV Edition Wireless | wireless / AEROX_5_DIABLO_WIRELESS (receiver) | Front, Middle, Rear | 484 |
| `1038:1862` | SteelSeries Aerox 5 Diablo IV Edition Wireless Wired | wireless / AEROX_5_DIABLO_WIRELESS_WIRED | Front, Middle, Rear | 485 |
| `1038:1850` | SteelSeries Aerox 5 Wired | Aerox5 | Front, Middle, Rear, Reactive | 486 |
| `1038:1858` | SteelSeries Aerox 9 Wireless | wireless / AEROX_9_WIRELESS (receiver) | Front, Middle, Rear, Reactive | 487 |
| `1038:185A` | SteelSeries Aerox 9 Wireless Wired | wireless / AEROX_9_WIRELESS_WIRED | Front, Middle, Rear, Reactive | 488 |
| `1038:1830` | SteelSeries Rival 3 Wireless | wireless / RIVAL_3_WIRELESS | Scroll Wheel | 513 |

LED zone ids: Front 0, Middle 1, Rear 2, Reactive 3; the Rival 3 Wireless "Scroll Wheel" is 1.
One grid row, in this order. (`STEELSERIES_AEROX_9_PID` = `0x185A` in `SteelSeriesDevices.h` is
not used by any detector line; `0x185A` is registered as the Aerox 9 Wireless in wired mode.)

## Protocol

Output reports with report id `0x00`. OpenRGB's RGBController (`RGBController_SteelSeriesRival3`):
Direct is mode value `0x04`, speed 0, brightness `0x64`; every update is `SetColor` per LED
followed by `SetLightEffectAll(0x04)`; the switch to Direct is `SetLightEffectAll(0x04)` alone.
`current_brightness` is never initialised in the three controllers, so HueDeck sends the
brightness report with the first colour update (and again only if it changes, which it does not:
HueDeck's colours already carry the user's brightness).

**Aerox 3 Wired (`SteelSeriesAerox3Controller`)**, 65-byte reports:
- colour: `00 21 <1 << zone> ...` with R G B at offset `3 + zone * 3`;
- brightness (when changed): `00 23 64`;
- `SetLightEffectAll`: nothing.

**Aerox 5 Wired (`SteelSeriesAerox5Controller`)**, 65-byte reports, each followed by 2 ms:
- zone 0: `00 21 01 R G B`; zone 1: `00 21 02 00 00 00 R G B`; zone 2: `00 21 04 00 00 00 00 00 00 R G B`;
- zone 3 (Reactive): `00 26 01 00 R G B`;
- brightness (when changed): `00 23 64` (OpenRGB passes a 3-byte buffer with length 65; the 62
  bytes past it are undefined in the C++, HueDeck sends zeros);
- `SetLightEffectAll`: nothing.

**Wireless controller (`SteelSeriesAeroxWirelessController`)**, 64-byte reports:
- colour: `00 21 01 <zone> R G B`; Aerox 9 zone 3 (Reactive): `00 26 01 00 R G B`; Rival 3
  Wireless: `00 13 <zone> R G B`;
- brightness (when changed): `00 23 <brightness / 10>` = `00 23 0A`;
- `SetLightEffectAll(0x04)`: nothing (only effect `0x05` sends `00 22 FF`; ignored on the Rival 3
  Wireless);
- through the 2.4 GHz receiver (the ids marked "receiver", `IsWireless()`): every command byte
  gets bit `0x40` (`0x21` -> `0x61`, `0x23` -> `0x63`, `0x26` -> `0x66`) and every write is
  followed by an input-report read (64 bytes) whose content is not looked at. OpenRGB waits up
  to 250 ms (`STEELSERIES_AEROX_WIRELESS_TIMEOUT`) on its own per-device thread; HueDeck's
  `Render()` runs on the frame loop (about 33 ms per frame), so it waits **20 ms** at most. A
  frame is up to five commands (four zones plus the first brightness report), so a sleeping or
  out-of-range mouse costs at most about 100 ms per frame instead of 1.25 s. An answer that
  arrives later stays in the input buffer and is drained by the next read; nothing reads it.

Unchanged frames are resent after 1 s only.

**Shutdown.** OpenRGB has no exit step; the other modes are hardware effects (Rainbow Breathing),
not the mouse's own saved lighting: every LED is set black in Direct.

## Left out

- `SendInit()`: OpenRGB's constructors send the feature report `00 2D 00 ...` (65 / 64 bytes),
  commented "This sets sensitivity and allows software mode?? ... buffer[4] to [8] = dpi / 50".
  It writes the DPI (sensitivity) table, which is not lighting: **never sent**. Whether the mice
  take per-LED colours without it is not known: **the plugin may not work at all** on these
  mice. The plugin cannot detect this (the mice send no answer that says so). This is the first
  check on hardware; if colours do not show, a lighting-only way into software mode must be found
  before the plugin is usable (the manifest description says so too).
- `Save()` (`00 11 00`): never sent.
- `GetFirmwareVersion()` (feature `00 90 00` + reads): only fills OpenRGB's device info.
- The Rainbow Breathing effect (`00 22 FF`).

## Untested

Every product id. First check: do colours show at all without the `0x2D` report (see Left out)?
