<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# WLED controllers (UDP realtime)

The plugin is `plugins/wled/`. It drives [WLED](https://kno.wled.ge) controllers (ESP8266 /
ESP32 boards running the WLED firmware, and WLED-compatible builds) on the local network with
WLED's UDP realtime protocol.

**Status: untested.** Nobody on the project owns a WLED controller. The packets are worked out
from WLED's public documentation and checked only against the mock net
(`plugins/wled/tests/wled.test.mjs`).

## Discovery

- **mDNS:** the service browses `_wled._tcp.local.` (WLED announces it). Each answer's IPv4
  address is a candidate.
- **Add by IP:** `discovery.forceDiscover(ip)` adds an IPv4 address to a list kept in the plugin
  settings (`wled` / `forced`, a JSON array), so it is tried again after a restart. A forced
  address that does not answer shows as a "not answering" row and is retried every 30 s.
- **Check:** for every candidate the service sends one asynchronous `GET http://<ip>/json/info`
  (3 s timeout). The answer is accepted when it has a numeric `leds.count` and a string `ver`.
  A `brand` other than `WLED` is accepted as WLED-compatible and logged. A candidate that fails
  is not asked again for 30 s.
- **Controller data** (announced to the device context): `id` = the MAC from `info.mac`
  (12 hex digits, lower case), else the IP; `name` = `info.name`; `ip`; `ledCount` =
  `info.leds.count`, capped at 1500 with a log line; `udpPort` = `info.udpport` (default 21324);
  `version` = `info.ver`; `rgbw` (`info.leds.rgbw`) and `lightCapabilities` (`info.leds.lc`) for
  information only. A controller with 0 LEDs is listed but not announced.
- The same MAC at a new address is announced again with the new IP (the device session
  restarts). An mDNS service that stops answering (HueDeck reports it after 95 s) is suppressed
  and removed unless it was added by IP.
- **Forget:** `discovery.forgetController(id)` removes it from the forced list and the dashboard.
  A controller found by mDNS comes back at its next mDNS answer.
- No pairing: WLED's realtime UDP input needs none.

## Streaming

One strip: `setSize([ledCount, 1])`, one LED per cell (`LED 1` … `LED n` at `[i, 0]`). Each
frame samples `device.color(i, 0)` and sends WLED UDP realtime datagrams to
`<ip>:<udpPort>` (`device.addFeature("udp")`, `udp.send`):

| LEDs | Protocol | Datagram |
|---|---|---|
| ≤ 490 | DRGB (byte 0 = `2`) | `02`, timeout, then R G B per LED (2 + 3n bytes) |
| > 490 | DNRGB (byte 0 = `4`) | `04`, timeout, start index high, low, then up to 489 × R G B |

- **Timeout** (byte 1) is `2`: WLED returns to its own effect 2 s after the last datagram, so
  it recovers by itself if HueDeck stops or crashes.
- **Rate:** the host calls `Render()` at most every 33 ms. A frame is sent only when the
  colours changed, or as a keep-alive after 1 s without a change.
- **RGBW strips:** RGB datagrams are sent; WLED derives the white channel itself
  (its "Auto white" setting). DRGBW (protocol 3) is not used.
- **Shutdown** (exit or suspend): if anything was sent, the last colours once more with timeout
  `1`; WLED then goes back to its own effect. No JSON API write is made: WLED's saved state,
  presets and configuration are never touched.

Examples (`plugins/wled/fixtures/expected.json`, worked out by hand): two LEDs red, green →
`02 02 FF 00 00 00 FF 00`; the shutdown packet → `02 01 FF 00 00 00 FF 00`; 500 LEDs → `04 02 00 00`
+ 489 LEDs, then `04 02 01 E9` + 11 LEDs.

## On the network (privacy)

| Direction | What | When |
|---|---|---|
| mDNS query `_wled._tcp.local.` | sent by HueDeck's host | while *Network lights* is on |
| `GET http://<ip>/json/info` (TCP 80) | one per candidate; retries every 30 s for failures | discovery |
| UDP to `<ip>:21324` (or the controller's `udpport`) | LED colours only | while the device runs |

Nothing leaves the LAN (the host refuses non-local addresses). The manifest lists UDP 21324;
a controller that reports a different `udpport` is reached on that port.

## Not implemented

- WLED's notifier sync, the JSON API state (`/json/state`) and presets: HueDeck only streams
  colours and leaves WLED's own state alone.
- Segments and 2D matrices: the strip is laid out as one row; a matrix still works but the
  canvas mapping is linear.
- DRGBW: not needed, WLED fills W from RGB.
- More than 1500 LEDs: a sanity cap (4 datagrams per frame); raise it if a real setup needs it.
- E1.31, Art-Net and DDP into WLED: see the `ddp` and `e131` plugins for those protocols.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | WLED documentation, "UDP Realtime Control", https://kno.wled.ge/interfaces/udp-realtime/ | Protocol bytes 2 (DRGB) and 4 (DNRGB), timeout byte, LED limits per datagram, port 21324 |
| S2 | WLED documentation, "JSON API", https://kno.wled.ge/interfaces/json-api/ | `/json/info` fields: `ver`, `leds.count`, `leds.rgbw`, `leds.lc`, `name`, `udpport`, `brand`, `mac` |
| S3 | WLED documentation, mDNS service `_wled._tcp` | Discovery |

No SignalRGB or OpenRGB code is used.
