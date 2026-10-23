<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Roccat mice: Kone Aimo, Kone Pro, Kone Pro Air, Kone XP, Burst Core / Pro (`1E7D`)

Plugin: `plugins/roccat-mouse/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB),
all `SPDX-License-Identifier: GPL-2.0-or-later`:

| Source | License | Taken |
|---|---|---|
| `Controllers/RoccatController/RoccatControllerDetect.cpp` | GPL-2.0-or-later | Product ids, names, `REGISTER_HID_DETECTOR_IPU` interface / usage page / usage |
| `RoccatKoneAimoController/RoccatKoneAimoController.cpp`, `.h`, `RGBController_RoccatKoneAimo.cpp` (Thibaud M (enlight3d), 17 Nov 2020) | GPL-2.0-or-later | Init, 46-byte colour report, channel layout, LED names |
| `RoccatKoneProController/RoccatKoneProController.cpp`, `.h`, `RGBController_RoccatKonePro.cpp` (Garrett Denham (GardenOfWyers), 12 Jan 2024) | GPL-2.0-or-later | `SwitchControl`, `SendDirect`, LED names |
| `RoccatBurstController/RoccatBurstController.cpp`, `.h`, `RGBController_RoccatBurst.cpp` (Morgan Guimard (morg), 24 Feb / 01 Jun 2022) | GPL-2.0-or-later | `SwitchControl`, `SendDirect`, LED counts and names |
| `RoccatKoneXPController/RoccatKoneXPController.cpp`, `.h`, `RGBController_RoccatKoneXP.cpp` (Mola19, 12 Jun 2024) | GPL-2.0-or-later | `EnableDirect`, `SendDirect`, `WaitUntilReady`, zones |
| `RoccatKoneProAirController/RoccatKoneProAirController.cpp`, `.h`, `RGBController_RoccatKoneProAir.cpp` (Plunti, 10 Jun 2024) | GPL-2.0-or-later | `SendRGB` (direct), LED names |
| Burst Pro Air, Kova, Sense Aimo, Elo controller files | GPL-2.0-or-later | Read only, to decide to hold them back |

## Products

| PID | OpenRGB name | Variant | Collection (interface, usage page, usage) | LEDs |
|---|---|---|---|---|
| `2E27` | Roccat Kone Aimo | KONE_AIMO | 0, `000B`, `0000` | 11 |
| `2E2C` | Roccat Kone Aimo 16K | KONE_AIMO | 0, `000B`, `0000` | 11 |
| `2C88` | Roccat Kone Pro | KONE_PRO | 3, `FF01`, `0001` | 2 |
| `2C8E` | Roccat Kone Pro Air | KONE_PRO_AIR | 2, `FF00`, `0001` | 2 |
| `2C92` | Roccat Kone Pro Air (Wired) | KONE_PRO_AIR | 1, `FF13`, `0001` | 2 |
| `2C8B` | Roccat Kone XP | KONE_XP | 3, `FF01`, `0001` | 20 |
| `2DE6` | Roccat Burst Core | BURST_CORE | 3, `FF01`, `0001` | 1 |
| `2DE1` | Roccat Burst Pro | BURST_PRO | 3, `FF01`, `0001` | 2 |

LED grid: a zone with several LEDs gets its own row, consecutive one-LED zones share a row.

## Protocol

- **KONE_AIMO**: start: GET_FEATURE report `0x00` (3 bytes, "swarm does it"), SET_FEATURE
  `0E 06 01 01 00 FF`. Frame: 46-byte feature report `0D 2E`, then per channel `c` (wheel 0, left
  strip 1-4, right strip 5-8, lower left 9, lower right 10) `R G B 00` at byte `2 + 4c`; then a
  3-byte GET_FEATURE of report `0x0D`. Shutdown: black.
- **KONE_PRO, BURST_CORE, BURST_PRO**: start: `0E 06 01 01 00 FF` (`SwitchControl(true)`). Frame:
  11-byte feature report `0D 0B` + `R G B` per LED. Shutdown: `0E 06 01 00 00 FF`
  (`SwitchControl(false)`, OpenRGB's "switch to built-in mode"): the mouse's own lighting.
- **KONE_XP**: start: `0E 06 01 01 00 FF` (`EnableDirect(true)`), ready poll (GET_FEATURE `0x04`, 4
  bytes, byte 1 == 1). Frame: 122-byte feature report `0D 7A` + `R G B FF FF FF` per LED (20).
  Shutdown: `0E 06 01 00 00 FF` + ready poll: the mouse's own lighting.
- **KONE_PRO_AIR**: frame: 16-byte output report `00 10 10 0B 00 09 64 64 64 0B` + `R G B` per LED
  (`SendRGB(true, ...)`: byte 2 `0x10` where the hardware-mode path uses `0x50` after a settings write; mode 9, brightness 0x64, speed 0x0B). Nothing at start.
  Shutdown: black.

Unchanged frames are skipped and resent after 1 s.

## Deviations from OpenRGB (deliberate)

- **Kone XP**: OpenRGB's switch to Direct is `SetMode(profile)` (feature report `0x06`, 174 bytes:
  the whole stored profile with DPI levels, polling rate, idle settings and colours), ready wait,
  `EnableDirect(true)`, ready wait; it also reads version / active profile and sends a profile
  select (`04 <profile> 80 FF`) at start. HueDeck sends only `EnableDirect(true)` and one ready
  wait, capped at 10 polls 200 ms apart (OpenRGB: 100). **Open question:** whether direct frames
  show without the profile write.
- **Kone Aimo**: the C++ reads the `0x0D` answer into its colour buffer, so the header of later
  frames is whatever the mouse answered; HueDeck keeps `0D 2E`.

## Safety

Never sent: the profile ("flash") report `0x06` (`SetMode` of Kone Pro, Burst, Kone XP), the Kone
XP profile select, the Kone Pro Air settings read/write (`00 90 ...` / `00 10 50 14 ...`) and its
stored-mode RGB report (`10 50 0B 01 ...`), DPI, polling rate. Tests check every feature report
starts with `0E 06`, `0D 0B`, `0D 2E` or `0D 7A` and every output report with `00 10 10`.

## Held back (not claimed)

| PID | OpenRGB name | Why |
|---|---|---|
| `2CA6` | Roccat Burst Pro Air | Every colour update and mode change is a SET_FEATURE with report id `0x06`, the id the Burst / Kone Pro use for the profile (flash) write; OpenRGB marks its saving as unknown (`@save :warning:`) |
| `2CEE` | Roccat Kova | No direct mode: every colour change reads and rewrites the stored profile (report `0x06`, which also holds DPI and polling rate) |
| `343A`, `343B` | Roccat Sense Aimo Mid / XXL (mousemats) | Direct (`03 ...`) needs the mode report `0x02` first, which OpenRGB marks as automatically saved; no separate software-control switch |
| `3A34` | Roccat Elo 7.1 (headset) | The start sequence writes output reports `FF 01`, `FF 02`, `FF 03 00 01` that OpenRGB does not explain (on a USB audio device they could be audio settings) |

## Untested

Every product id.
