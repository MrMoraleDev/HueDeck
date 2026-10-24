<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# DDP output (Distributed Display Protocol)

The plugin is `plugins/ddp/`. It sends LED colours with DDP, the 3waylabs Distributed Display
Protocol, to any receiver that accepts it on UDP port 4048: WLED, xLights, ESPixelStick,
Falcon-style pixel controllers and others.

**Status: untested.** Nobody on the project owns a DDP receiver. The packets are worked out
from the public DDP specification and checked only against the mock net
(`plugins/ddp/tests/ddp.test.mjs`).

## Adding a receiver

DDP has no discovery that this plugin uses, so receivers are added by hand:

- **Add by IP:** `discovery.forceDiscover(ip)` adds a controller with the id `ddp-<ip>`, name
  `DDP <ip>`, port 4048, and announces it at once (nothing is sent to the receiver by the
  service). The list of IPs is kept in the plugin settings (`ddp` / `controllers`, a JSON
  array) and restored at start.
- **Forget:** `discovery.forgetController(id)` stops the device and removes it from the list.

## Parameters

| Parameter | Type | Default | Meaning |
|---|---|---|---|
| `ledCount` | number 1..1500 | 60 | LEDs on the receiver's output. Changing it re-lays out the strip (`onledCountChanged`). |
| `colorOrder` | RGB, RBG, GRB, GBR, BRG, BGR | RGB | Byte order per LED. Most receivers reorder for their strip themselves; change it only when colours come out swapped. |

## Packets

The strip is one row, `setSize([ledCount, 1])`, `LED 1` … `LED n` at `[i, 0]`. Each frame's
RGB bytes are cut into packets of at most 1440 data bytes (480 LEDs). Each packet is a 10-byte
header and the data:

| Byte | Field | Value |
|---|---|---|
| 0 | flags `V V x T S R Q P` | `0x40` (version 1); `0x41` on the last packet of the frame (push) |
| 1 | sequence (low 4 bits) | 1..15, one per packet, then 1 again (0 = "not used" is skipped) |
| 2 | data type `C R T T T S S S` | `0x0B`: standard type, TTT = 001 (RGB), SSS = 011 (8 bits per element) |
| 3 | destination id | `0x01`, the default output device |
| 4-7 | data offset in bytes | big-endian |
| 8-9 | data length | big-endian |

Examples (`plugins/ddp/fixtures/expected.json`, worked out by hand): two LEDs red, green →
`41 01 0B 01 00 00 00 00 00 06 FF 00 00 00 FF 00`; 500 LEDs → `40 01 0B 01 00 00 00 00 05 A0`
+ 1440 bytes, then `41 02 0B 01 00 00 05 A0 00 3C` + 60 bytes.

OpenRGB's DDP controller sends data type `0x01` and sets push on every packet; this plugin
follows the specification (`0x0B`, push on the last packet). Receivers that only look at the
pixel count (WLED reads TTT/SSS to choose 3 or 4 bytes per pixel) handle both.

## Rate and shutdown

- The host calls `Render()` at most every 33 ms; a frame is sent when it changed, and as a
  keep-alive after 1 s without a change (WLED leaves its realtime mode after its timeout,
  2.5 s by default).
- **Shutdown** sends nothing: DDP has no end-of-stream message. The receiver keeps the last
  frame or goes back to its own program after its own timeout (WLED: its realtime timeout).

## On the network (privacy)

Only UDP datagrams with LED colours to `<ip>:4048`, while the device runs. The service sends
nothing. Nothing leaves the LAN (the host refuses non-local addresses).

## Not implemented

- DDP discovery (query/status/config packets, ids 250/251): not needed to stream and not all
  receivers answer; the receiver's LED count must be entered by hand.
- Timecode, RGBW (`0x1B`) and other data types, destination ids other than 1.
- More than 1500 LEDs (a sanity cap).

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | 3waylabs, "DDP - Distributed Display Protocol", http://www.3waylabs.com/ddp/ | Header layout, flag bits, data type encoding, destination ids, port 4048, 1440-byte data recommendation |
| S2 | OpenRGB `Controllers/DDPController/DDPController.h` (GPL-2.0-or-later, commit d4798c12) | Cross-check only (port 4048, flag values, `0x0B`, 1440 bytes); no code taken |

No SignalRGB code is used. The 3waylabs page could not be fetched while this plugin was
written (connection refused); the header was written from the specification as known and
cross-checked against S2's constants.
