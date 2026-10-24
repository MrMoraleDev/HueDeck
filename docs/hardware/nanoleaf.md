<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Nanoleaf panels (OpenAPI, extControl streaming)

The plugin is `plugins/nanoleaf/`. It drives Nanoleaf Light Panels (Aurora), Canvas, Shapes,
Elements and Lines through the Nanoleaf OpenAPI on the LAN: pairing and control over HTTP on
port 16021, colours streamed over UDP with the "extControl" protocol. Each panel is one LED,
placed from the panel layout the device reports.

**Status: untested.** Nobody working on HueDeck owns Nanoleaf panels. The plugin is written
from the public Nanoleaf OpenAPI documentation, with the extControl packet layouts checked
against OpenRGB's NanoleafController, and is tested only against the mocks in
`plugins/nanoleaf/tests/`.

## Discovery

- **mDNS** `_nanoleafapi._tcp.local.` (TXT `id`, `md` model, `srcvers` firmware). The
  controller id is the TXT `id`.
- **Add by IP** (`forceDiscover(ip)`): the OpenAPI has no request that answers without a
  token, so any HTTP answer to `GET http://<ip>:16021/api/v1/` (normally 401) is taken as a
  Nanoleaf device, with id `ip-<a-b-c-d>`. A device found both ways is kept once (matched by
  address).

Settings: `<id>/ip`, `<id>/token` and the list of ids (`devices/list`), so a restart finds
devices added by IP.

## Pairing (power button)

The dashboard's **Pair** calls `startLink()`. For 30 s (one try at once, then one per service
`Update()`, about every second) the plugin sends `POST http://<ip>:16021/api/v1/new`:

- **403**: the device is not in pairing mode. The status says *Hold the power button for 5-7
  seconds, until the lights flash*; the plugin keeps asking.
- **200** `{"auth_token":"..."}`: the token is saved (`<id>/token`), `paired` is `true`, and
  the device info is read.
- other answers (401, no answer): keep asking.

After 30 s the status says pairing timed out.

The device info is `GET http://<ip>:16021/api/v1/<token>/`: `name`, `model`,
`firmwareVersion`, and `panelLayout.layout.positionData` = `[{panelId, x, y, o, shapeType}]`.
Panels whose shape has no LEDs are left out: Rhythm module (shapeType 1), Power Supply (5),
Shapes Controller (12), Lines Controller Cap (19) and Power Connector (20). The device is then
announced with `{ ip, port, token, name, model, firmwareVersion, panels, extControl }`. A 401
or 403 to the info request means the token was revoked: it is deleted and `paired` is `false`.
**Forget** deletes the token and the address, suppresses the device and ignores it on mDNS for
the rest of the session.

## Layout

Panel positions are centres in layout units (about a millimetre; y points up). The plugin
puts the two closest panels 2 cells apart (unit = nearest centre distance / 2), flips y, and
grows the unit if the grid would be wider or taller than 120 cells (very large Lines setups;
panels may then share a cell). The device's size is the grid's bounding box; one LED per
panel, named `Panel <panelId>`. Example (fixture): Canvas squares at (50, 50), (150, 50) and
(150, 150) give unit 50, cells (0, 2), (2, 2), (2, 0), size 3 x 3. The layout's
`globalOrientation` and each panel's own rotation `o` are not applied.

## Streaming

**Initialize** (all synchronous):

1. `GET /api/v1/<token>/effects/select` (the current effect name) and `GET .../state`
   (on, brightness, hue, sat, ct, colorMode), kept for Shutdown;
2. if the panels were off: `PUT .../state {"on":{"value":true}}`;
3. `PUT .../effects {"write":{"command":"display","animType":"extControl","extControlVersion":"v2"}}`
   (`"v1"` for the Light Panels, model `NL22`).

**Render** samples one colour per panel and sends one UDP datagram:

- **v2** (Canvas, Shapes, Elements, Lines and newer) to `<ip>:60222`:
  `nPanels` (2 bytes, big endian), then per panel `panelId` (2 bytes BE), `R`, `G`, `B`,
  `W` (0), transition time (2 bytes BE, in 100 ms units).
- **v1** (Light Panels, `NL22`): to the `streamControlIpAddr`:`streamControlPort` the
  extControl reply names (60221 if it names none): `nPanels` (1 byte), then per panel
  `panelId` (1 byte), `nFrames` (1), `R`, `G`, `B`, `W` (0), transition time (1 byte).

The transition time is **0** (jump straight to the colour, as OpenRGB does); with 1 the
panels would fade over 100 ms, which smooths but lags. A datagram is sent when the colours
changed, and an unchanged frame once a second so the device stays in external control.
HueDeck renders every 33 ms, so at most about 30 datagrams per second. If entering extControl
fails, Render retries it (asynchronously) every 5 s and sends nothing meanwhile.

Hand-worked datagrams: `plugins/nanoleaf/fixtures/packets.json`.

**Shutdown(suspend)** (synchronous): the saved effect is selected again with
`PUT .../effects {"select":"<name>"}`. Names starting with `*` (`*Solid*`, `*Static*`,
`*Dynamic*`, `*ExtControl*`) cannot be selected; then a colour in `hs` mode is restored with
`PUT .../state {"hue":...,"sat":...,"brightness":...}`, a `ct` one with `{"ct":...,
"brightness":...}`, anything else is left showing the last colours (logged). Panels that were
off are switched off again.

## What goes on the LAN

Only to the device: HTTP on port 16021 (pairing, info, effect and state requests; the token
is part of every URL, in the clear) and UDP datagrams with colours to port 60222 (or the v1
stream port). Nothing goes to the Nanoleaf cloud.

## Not implemented

- Light Panels firmware without the OpenAPI extControl (very old firmware); Light Panels
  always use v1 here, even on firmware that also offers v2.
- `globalOrientation` and per-panel rotation; per-zone control of multi-zone panels (Lines
  segments are driven as one colour per `panelId`).
- Panels added later appear after the service restarts.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | Nanoleaf OpenAPI documentation (forum.nanoleaf.me/docs, now nanoleaf.atlassian.net/wiki/spaces/nlapid, "Nanoleaf Light Panels Open API Documentation") | Port 16021, `POST /api/v1/new` (200 with `auth_token`, 403 when not in pairing mode, holding the power button 5-7 s), `GET /api/v1/<token>/`, `effects/select`, `PUT /effects` with `select` and `write`, `state` (`on`, `brightness`, `hue`, `sat`, `ct`, `colorMode`), panel layout and shape types, extControl v1/v2, port 60222, transition time units |
| S2 | Nanoleaf OpenAPI documentation, mDNS discovery section | `_nanoleafapi._tcp`, TXT `id`, `md`, `srcvers` |
| S3 | OpenRGB `Controllers/NanoleafController/NanoleafController.cpp` (GPL-2.0-or-later, Copyright (C) 2022 Nikita Rushmanov, "part of the OpenRGB project"), commit `d4798c12`, https://gitlab.com/CalcProgrammer1/OpenRGB | Cross-check of the v1/v2 datagram layouts, v1 for model `NL22` and v2 for the others, the v1 reply's `streamControlIpAddr`/`streamControlPort`, transition time 0 |
| S4 | OpenRGB `Controllers/NanoleafController/NanoleafController.h` (same license and commit) | Model ids `NL22` (Light Panels), `NL29` (Canvas), `NL42` (Shapes), `NL59` (Lines) |

The plugin's code is HueDeck's own; the facts checked against S3/S4 are credited in the
`plugin.js` header. No SignalRGB code was used.
