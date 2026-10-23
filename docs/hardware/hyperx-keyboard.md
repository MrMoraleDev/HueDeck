<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HyperX keyboards: Alloy Elite RGB, Alloy FPS RGB, Alloy Elite 2, Alloy Origins, Origins 60 / 65, Eve 1800, Origins 2 65

Plugins: `plugins/hyperx-keyboard/` (Kingston vendor id `0951`) and `plugins/hyperx-keyboard-hp/`
(HP vendor id `03F0`, notes: [hyperx-keyboard-hp.md](hyperx-keyboard-hp.md)). A plugin has one
vendor id, so the family is split in two directories that share `family.js` and `layouts.js`
byte for byte (a test checks it). Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (under `Controllers/HyperXKeyboardController/`) | License | Author in header | Taken |
|---|---|---|---|
| `HyperXAlloyEliteController/HyperXAlloyEliteController.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 30 Jan 2020 | Direct reports (`SendDirect`, `SendDirectExtended`, `SetLEDsDirect`), key and extended byte tables, pauses |
| `HyperXAlloyEliteController/RGBController_HyperXAlloyElite.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 02 Feb 2020 | Zones, LED names, matrix map, keepalive period |
| `HyperXAlloyFPSController/HyperXAlloyFPSController.cpp`, `.h`, `RGBController_HyperXAlloyFPS.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 30 Jan / 02 Feb 2020 | Same, without the extended report |
| `HyperXAlloyElite2Controller/HyperXAlloyElite2Controller.cpp`, `.h`, `RGBController_HyperXAlloyElite2.cpp`, `.h` | GPL-2.0-or-later | KundaPanda (vojdo), 02 Apr 2021 | Start report, skip table, packing, LED names, matrix, keepalive |
| `HyperXAlloyOriginsController/HyperXAlloyOriginsController.cpp`, `.h`, `RGBController_HyperXAlloyOrigins.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 11 Jul 2020 | Start report, skip table, packing, LED names, matrix |
| `HyperXAlloyOrigins60and65Controller/*` | GPL-2.0-or-later | Derek Huber, 18 Mar 2023 | Start report, packing, both layouts, product id to layout |
| `HyperXEve1800Controller/*` | GPL-2.0-or-later | none named | Start and colour output reports, 10 zones |
| `HyperXOrigins2_65Controller/*` | GPL-2.0-or-later | Ricardo Amorim, 28 Mar 2026 | Start and numbered colour output reports, LED names, matrix |
| `*/…ControllerDetect.cpp` (all eight) | GPL-2.0-or-later | none named / as above | Product ids, names, interface / usage page (the `_WIN32` branch) |
| `HyperXAlloyOriginsCoreController/*` | GPL-2.0-or-later | Volodymyr Nazarchuk (Vavooon) and others | Read only, to decide to hold it back |
| `RGBController/RGBControllerKeyNames.cpp`, `.h` | GPL-2.0-or-later | Chris M (Dr_No), 25 Jan 2022 | Key name strings (`KEY_EN_*`) |

`layouts.js` is generated from these tables by the porting session's script; `family.js` is
written by hand from the C++.

## Products

| Product id | OpenRGB name | Variant | Validate (Windows registration) |
|---|---|---|---|
| `0951:16BE` | HyperX Alloy Elite RGB | ALLOY_ELITE | interface 2, usage page `FF01` |
| `0951:16DC` | HyperX Alloy FPS RGB | ALLOY_FPS | interface 2, usage page `FF01` |
| `0951:1711` | HyperX Alloy Elite 2 | ALLOY_ELITE_2 | interface 3, usage page `FF90` |
| `0951:1734` | HyperX Alloy Origins 60 | ALLOY_ORIGINS_60 | interface 3, any collection |
| `0951:16E5` | HyperX Alloy Origins | ALLOY_ORIGINS | interface 3, any collection |
| `03F0:…` | HP ids | | see [hyperx-keyboard-hp.md](hyperx-keyboard-hp.md) |

**Held back:** HyperX Alloy Origins Core (`0951:16E6`, `03F0:098F`). OpenRGB builds its LED
list at run time with its `KeyboardLayoutManager` (TKL ANSI or ABNT2 overlay, chosen by a
layout-variant query: output report `00 10`, then an input report whose byte 56 is the variant),
and maps each key to one of 94 colour slots in a 380-byte G/R/B-planar buffer sent as `00 A2
<seq>` output reports. Porting the layout manager was out of scope for this batch; the ids are
not claimed.

For an interface-only registration OpenRGB opens every collection of the interface and the one
that takes the reports drives the keyboard. HueDeck opens one: when the interface has several,
`Initialize()` makes active the first whose report descriptor (`getHidReportFields()`) declares
the report the keyboard takes (a 65-byte feature report with id 0, or a 65-byte output report
with id `0x44`).

## LED layout

LED order is OpenRGB's (`led_names`), which is also the wire order. Grid rule:

- a matrix zone: every LED takes the first matrix cell that names it (the Alloy Origins matrix
  lists F12, LED 71, twice; the first cell, in the function row, is kept);
- LEDs OpenRGB names `""` (`KEY_EN_UNUSED`, the Origins 60 / 65 / Origins 2 65 placeholders) are
  not exposed and are sent black;
- a named LED of a matrix zone that no matrix cell points at goes into an extra row under the
  matrix (only the Origins 2 65's "Right Space": its matrix points at LED 68, an unused slot);
- the Alloy Elite's linear "RGB Strip" zone (18 LEDs) and its "Media Keys" zone (4 LEDs) get a
  row each under the matrix.

| Variant | Wire LEDs | Exposed | Grid |
|---|---|---|---|
| ALLOY_ELITE | 128 | 128 | 23 x 8 |
| ALLOY_FPS | 106 | 106 | 23 x 6 |
| ALLOY_ELITE_2 | 128 | 128 | 22 x 8 |
| ALLOY_ORIGINS | 107 | 107 | 23 x 6 |
| ALLOY_ORIGINS_60 | 71 | 63 | 14 x 5 |
| ALLOY_ORIGINS_65 | 77 | 69 | 15 x 5 |
| EVE_1800 | 10 | 10 | 10 x 1 ("Zone 1" … "Zone 10") |
| ORIGINS_2_65 | 74 | 70 | 15 x 6 |

## Protocol (Direct mode only)

OpenRGB's constructors send nothing; its Direct mode is the default and the first frame takes
the keyboard over. HueDeck's `Initialize()` sends nothing either.

- **Alloy Elite RGB**: four 264-byte feature reports per frame, `07 16 <channel> A0`, channel 1
  red, 2 green, 3 blue (106 key values at the bytes of the `keys` table), 5 ms apart, then
  channel 4 (extended): bytes `0x08..0x93` carry the R, G, B of the 18 light-bar and 4 media
  LEDs at the `extended_red/grn/blu` tables. OpenRGB does not clear its extended buffer, so the
  bytes those tables leave unset carry stack garbage there; HueDeck sends 0.
- **Alloy FPS RGB**: the first three of those reports, 10 ms apart.
- **Alloy Elite 2**: 65-byte feature reports: start `00 04 F2`, then groups `81 R G B`, 16 per
  report, with an empty group inserted at every index of `SKIP_INDICES`, then empty reports up
  to 9 in all.
- **Alloy Origins**: start `00 04 F2 … [9]=09`, then 9 reports of 16 groups `81 R G B` (a black
  colour is first inserted at each index of `skip_idx`, as `std::vector::insert`); a report with
  nothing left carries one black group.
- **Alloy Origins 60 / 65**: start `00 04 F2 … [9]=05`, then 5 such reports.
- **Eve 1800**: 65-byte output reports with report id `0x44`: start `44 01 04`, then `44 02 00
  00` and the 10 zones as `R G B`.
- **Origins 2 65**: start `44 01 04`, then four reports `44 02 <0..3> 00` of up to 20 `R G B`.

**Frame rate / keepalive.** OpenRGB's keepalive threads resend whenever 50 ms passed without an
update (the Alloy Elite / FPS comments: "requires a steady stream of packets in order to not
revert out of direct mode"); HueDeck sends a frame on every `Render()` for those. The Alloy
Elite 2's thread waits 1 s: HueDeck skips unchanged frames and resends after 1 s.

**Shutdown.** OpenRGB has no exit step and these keyboards have no "back to the stored profile"
command that does not write the profile: HueDeck sends one black Direct frame. Keyboards that
need the steady stream fall back to their own lighting once frames stop.

## Safety: left out

- Alloy Elite `SetMode` / `SendEffect` (`07 02`), `SetLEDs` / `SendColor` / `SendExtendedColor`
  (`07 06`) and `SelectProfile` (`07 01`): they write the keyboard's stored profile (OpenRGB
  flags those modes `MODE_FLAG_AUTOMATIC_SAVE`). Never sent; the hardware effects are not ported.
- Brightness commands: Eve 1800 `40 01 00 00 <b>`, Origins Core `00 A7 … 01 <b>` (sent by
  OpenRGB only on a mode change). Not sent: HueDeck applies brightness to the colours.
- A test checks every report sent starts with an allowed Direct header.

## Untested

Every product id. The vendor-software process names reported as conflicting
(`NGenuity2.exe`, `HyperX NGENUITY.exe`) are a best guess, not checked on a machine with
NGENUITY installed.
