<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# NZXT Kraken X53/X63/X73 ("X3", `1E71:2007`): lighting

Source of every fact below: Lightloom's own recordings of the HID traffic of the
reference machine's Kraken X3 (`lightloom ... --record`, 2026-10-18) while another
plugin drove it, decoded with a coordinate-probe effect (each canvas pixel encodes
its position, so every colour byte shows which LED it belongs to). No code of any
other project was used. The plugin is `plugins/nzxt-kraken-x3/`.

## Transport

One HID interface (interface 0, vendor usage page), **output reports of 64 bytes**,
first byte = report id. Nothing is read back for lighting.

## Packets

| Packet | Bytes (hex, then zero padding to 64) | Meaning |
|---|---|---|
| colours, first half | `22 10 <ch> 00` + 20 LEDs × `G R B` | LEDs 0..19 of channel `<ch>` |
| colours, second half | `22 11 <ch> 00` + 20 LEDs × `G R B` | LEDs 20..39 |
| apply | `22 a0 <ch> 00 01 00 00 28 00 00 80 00 32 00 00 01` | show the colours just sent |
| logo | `2a 04 04 04 00 32 00 G R B`, and `01 00 01 03` at bytes 56..59 (constant, meaning unknown) | the logo LED in the pump cap |

Channels: `01` = the RGB header on the pump (up to 40 LEDs: fans, strips),
`02` = the pump ring (8 LEDs). Colour byte order is **G, R, B**.

Order of one frame as recorded: channel 1 (`10`, `11`, apply), channel 2 (`10`,
`11`, apply), logo; about 2 ms apart, 14 ms per frame. At the end of a session the
recorded plugin sent channel 2, channel 1 and the logo with all colours zero.

## Pump ring order

The 8 ring LEDs go round **clockwise**, LED 0 just right of 12 o'clock (decoded
from the probe: the colours of LEDs 0..7 came from top-right, right-top,
right-bottom, bottom-right, bottom-left, left-bottom, left-top, top-left).

## Not covered

Pump and fan duty (other report ids) are left alone: Lightloom's plugin only lights.
