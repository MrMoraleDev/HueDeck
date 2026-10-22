<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Razer DeathAdder V2 Pro (`1532:007C` wired, `1532:007D` receiver): lighting

Source of every fact below: Lightloom's own recordings of the mouse's HID traffic on
the reference machine (`lightloom ... --record`, 2026-10-18), decoded with a
coordinate-probe effect. No code of any other project was used.

> **Plugin retired (2026-10-22).** The single-device plugin `plugins/razer-deathadder-v2-pro/`
> was removed in favour of the Razer family plugin `plugins/razer/` (`docs/hardware/razer.md`),
> which drives `007C` and `007D` with exactly the packets below (its `RECORDED_PROFILE` path) and
> was tested on this mouse on 2026-10-22. This page stays as the protocol notes of Lightloom's own
> recording; the recording is `plugins/razer/fixtures/deathadder-v2-pro-recorded.json` (serial
> number replaced by a placeholder). Close Razer Synapse 4 (`RazerAppEngine.exe`) before driving
> the mouse: with it running the light flickers, and Lightloom reports it as a conflict.

## Transport

Interface 0, the mouse collection (usage page `0x01`, usage `0x02`), **feature
reports of 91 bytes** with report id `0x00`. Windows owns the mouse collection, so
Lightloom opens it for feature reports only.

## Report layout

| Byte | Meaning |
|---|---|
| 0 | report id `00` |
| 1 | status: `00` in a request; in the reply `02` success, `01` busy, `03` failure, `05` not supported |
| 2 | transaction id: `1f` for this mouse (the recorded probe tried `1f`, `2f`, ... `9f`) |
| 3, 4 | `00 00` |
| 5 | `00` |
| 6 | number of argument bytes |
| 7 | command class |
| 8 | command id (high bit set: a query) |
| 9..88 | arguments |
| 89 | checksum: XOR of bytes 3..88 |
| 90 | `00` |

The reply to a request is read with a feature-report read of the same report id;
it echoes the transaction id, class and id with the status in byte 1.

## Commands used

| Class, id | Arguments | Meaning |
|---|---|---|
| `00 82` | `00 00` | serial number (the reply carries it as text): used to check the transaction id |
| `0f 02` | `00 00 08 00 01 00` | switch the lighting to frames sent by the host |
| `0f 03` | `00 00 00 00 00 R G B` | one frame: the colour of the single LED zone (row 0, columns 0..0); no reply is read |
| `00 04` | `00 00` | hand the lighting back to the mouse (sent at the end) |

At the end of a session the recorded plugin sent a black frame and then `00 04`.

## Not covered

Battery, DPI stages, polling rate and button settings are never touched.
