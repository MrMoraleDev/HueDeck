<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries QCK Prism Cloth mouse mats

Plugin: `plugins/steelseries-qck/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesQCKMatController/SteelSeriesQCKMatController.cpp`, `.h` (Edbgon, 22 May 2021)
- `Controllers/SteelSeriesController/SteelSeriesQCKMatController/RGBController_SteelSeriesQCKMat.cpp`, `.h` (Edbgon, 22 May 2021)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

All `REGISTER_HID_DETECTOR_I(..., 0)`: any collection of interface 0.

| Product id | OpenRGB name | Detect.cpp line |
|---|---|---|
| `1038:150A` | SteelSeries QCK Prism Cloth Medium | 530 |
| `1038:150D` | SteelSeries QCK Prism Cloth XL | 531 |
| `1038:151E` | SteelSeries QCK Prism Cloth XL Destiny Ed. | 532 |
| `1038:1520` | SteelSeries QCK Prism Cloth XL Destiny 2 Lightfall Ed. | 533 |
| `1038:1514` | SteelSeries QCK Prism Cloth XL CS:GO Neon Rider Ed. | 534 |
| `1038:151C` | SteelSeries QCK Prism Cloth XL CS:GO Neo Noir Ed. | 535 |
| `1038:1516` | SteelSeries QCK Prism Cloth 3XL | 536 |
| `1038:1518` | SteelSeries QCK Prism Cloth 4XL | 537 |
| `1038:151A` | SteelSeries QCK Prism Cloth 5XL | 538 |

## Protocol

LEDs: one zone "Mousemat" with "Mat Bottom LED" (colour 0) and "Mat Top LED" (colour 1), one grid
row in this order. The packet always carries both.

**Start.** OpenRGB's only mode, Direct, sends the colours and nothing else.

**Every update** (`SetColors`):

1. a 525-byte **feature** report: `00 0E 00 02 00 R0 G0 B0 FF 32 C8 00 00 00 01 00 00 R1 G1 B1 FF
   32 C8 00 00 01 01 00 01` then zeros (report id `0x00`; bytes 5..7 the bottom colour, 0x11..0x13
   the top colour);
2. a 65-byte output report `00 0D` then zeros.

OpenRGB does not say what the `0x0D` report does: it may only apply the colours, or it may also
store them in the mat's memory. If it stores them, sending it at frame rate under an animated
effect would be a memory write on every frame (about 108000 an hour). HueDeck therefore sends an
update only when a colour changes, **at most once per second** (a change inside the second waits;
the next frame after it sends the colours of that moment), and never as a keepalive: a still
picture costs no writes at all, a running effect at most 3600 pairs an hour and looks stepped on
the mat. The residual risk (up to one possible memory write per second while an effect runs) is
stated in the manifest description too.

**Shutdown.** No exit step and no other mode in OpenRGB: both LEDs are set black (one pair). If
`0x0D` stores the colours, the mat stays dark after HueDeck exits, until other software sets it.

## Left out

Nothing lighting-related.

## Untested

Every product id. Whether the `0x0D` report that follows every colour report only applies the
colours or also stores them in the mat's memory is not known (open safety question: check on
hardware, for example whether the last colours survive a replug, before relaxing the one-second
limit).
