<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HyperX Pulsefire mice: Surge, FPS Pro, Core, Dart, Raid, Haste

Plugins: `plugins/hyperx-mouse/` (Kingston vendor id `0951`) and `plugins/hyperx-mouse-hp/` (HP
vendor id `03F0`, notes: [hyperx-mouse-hp.md](hyperx-mouse-hp.md)); `family.js` is shared byte for
byte (a test checks it). Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (under `Controllers/HyperXMouseController/`) | License | Author in header | Taken |
|---|---|---|---|
| `HyperXMouseControllerDetect.cpp` | GPL-2.0-or-later | none named | Product ids, names, interface / usage page / usage (the `_WIN32` branch for the Haste) |
| `HyperXPulsefireSurgeController/*` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 25 Jul 2020 | `SendDirect`, zones, keepalive |
| `HyperXPulsefireFPSProController/*` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 26 Dec 2020 | `SendDirect`, zone, keepalive |
| `HyperXPulsefireHasteController/*` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 19 Aug 2020 | `SendDirectSetup`, `SendDirectColor`, zone, keepalive |
| `HyperXPulsefireDartController/*` | GPL-2.0-or-later | Santeri Pikarinen (santeri3700), 26 Dec 2020 | `SendDirect`, Direct mode values, LED ids, order |
| `HyperXPulsefireRaidController/*` | GPL-2.0-or-later | Morgan Guimard (morg), 06 Apr 2022 | `SendColors`, zones, keepalive |

## Products

| Product id | OpenRGB name | Variant | Validate |
|---|---|---|---|
| `0951:16D3` | HyperX Pulsefire Surge | SURGE | interface 1, usage page `FF01` |
| `0951:16D7` | HyperX Pulsefire FPS Pro | FPS_PRO | interface 1, usage page `FF01` |
| `0951:16DE` | HyperX Pulsefire Core | FPS_PRO | interface 1, usage page `FF01` |
| `0951:16E1` | HyperX Pulsefire Dart (Wireless) | DART | interface 2, usage page `FF00` |
| `0951:16E2` | HyperX Pulsefire Dart (Wired) | DART | interface 1, usage page `FF13` |
| `0951:16E4` | HyperX Pulsefire Raid | RAID | interface 1, usage page `FF01`, usage `01` |
| `0951:1727` | HyperX Pulsefire Haste | HASTE | interface 3, usage page `FF90` |
| `03F0:…` | HP ids | | see [hyperx-mouse-hp.md](hyperx-mouse-hp.md) |

## LEDs

| Variant | LEDs (OpenRGB order) | Grid |
|---|---|---|
| SURGE | "LED Strip LED 1" … "LED Strip LED 32" (row 0), "Logo" (row 1) | 32 x 2 |
| FPS_PRO, HASTE | "Logo" | 1 x 1 |
| DART, RAID | "Scroll Wheel", "Logo" | 2 x 1 |

## Protocol (Direct mode only)

Nothing is sent at `Initialize()` (OpenRGB's constructors send nothing; Direct is the default
mode).

- **Surge**: one 264-byte feature report `07 14 00 A0`, strip red / green / blue planes at
  `0x08` / `0x28` / `0x48` (32 bytes each), logo R G B at `0x6C`.
- **FPS Pro / Core**: 264-byte feature report `07 0A R G B 00 00 00 A0`.
- **Haste**: 65-byte feature reports `00 04 F2 … [8]=02`, then `00 81 R G B … [8]=02`.
- **Dart**: per LED a 65-byte output report `00 D2 <led> 00 08 R G B R G B 64 00` (static mode
  `00`, brightness `0x64`, speed 0; scroll wheel `10`, then logo `00`), each followed by 2 ms.
- **Raid**: 264-byte feature report `07 0A` + scroll wheel R G B + logo R G B, `[8]=A0`, then 10 ms.

Surge, FPS Pro and Haste: OpenRGB's keepalive resends after 50 ms without an update, so HueDeck
sends a frame on every `Render()`. Raid: its keepalive waits 1 s ("or it will reset to default
(flash)"); Dart: no keepalive; for both HueDeck skips unchanged frames and resends after 1 s.

**Shutdown.** OpenRGB has no exit step and no "back to the stored lighting" command: one black
Direct frame. (The Raid may go back to its own lighting once frames stop.)

## Safety: left out

- Dart `Save()` (`00 DE FF`, "Save current settings to the on-board memory"): never sent; a test
  checks it.
- Dart Breathing / Spectrum Cycle / Reactive modes (hardware effects): not ported.
- Surge `SelectProfile` (`07 07`) and `SetProfileBrightness` (`07 03`): profile commands, never
  sent (OpenRGB does not call them either).
- No DPI, polling rate or button command exists in these files; none is sent.

## Untested

Every product id. The NGENUITY process names are a best guess.
