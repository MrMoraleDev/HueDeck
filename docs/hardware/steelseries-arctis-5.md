<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# SteelSeries Arctis 5 (`1038:12AA`): ear-cup lights

Source of every fact below: HueDeck's own recordings of the headset's HID traffic
on the reference machine (`huedeck ... --record`, 2026-10-18), decoded with a
coordinate-probe effect. No code of any other project was used. The plugin is
`plugins/steelseries-arctis-5/`.

## Transport

Interface 5 (consumer usage page), **output reports**: report `0x06` is 37 bytes
long, report `0x04` 39 bytes. Nothing is read back.

## Start (once)

| Bytes (hex, zero padded) | Note |
|---|---|
| `04 40 06 10 f6 60 09 2c ff 7f` (39) | meaning unknown; sent in every recording |
| `04 40 01 11 54 9b` (39) | meaning unknown; sent in every recording |
| `06 8a 42 00 20 05` | |
| `06 81 43 01 23` | |

## One zone (the two ear cups are zones 0 and 1)

| Bytes (hex, zero padded to 37) | Meaning |
|---|---|
| `06 81 43 01 22` | |
| `06 81 43 01 23` | |
| `06 8a 42 00 20 41 00 R G B ff 32 c8 c8` | the colour (byte order R, G, B) |
| `06 8a 42 00 20 41 08 <zone> 01` | the zone the colour goes to |
| `06 8a 42 00 20 60 <zone>` | apply |
| `06 8a 42 20 05` | commit |

Every frame sends zone 0 then zone 1: 12 reports, about 2 ms each. At the end of a
session the recorded plugin sent both zones black.

## Known behaviour

About one write in several thousand fails with `ERROR_GEN_FAILURE` ("a device
attached to the system is not functioning"), with any plugin; the next frame goes
through. HueDeck counts it and carries on.
