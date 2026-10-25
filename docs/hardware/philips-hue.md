<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Philips Hue bridges (Entertainment API, CLIP v1 fallback)

The plugin is `plugins/philips-hue/`. It drives the colour lamps of a Philips Hue bridge. Every
colour lamp is its own subdevice with one LED, so each lamp is placed on the canvas on its own.
The bridge itself has no LEDs (`SubdeviceController()` is `true`).

**Status: untested.** Nobody working on Lightloom owns a Hue bridge. The plugin is written from
Philips' public developer documentation and checked only against the mocks in
`plugins/philips-hue/tests/` (`hue.test.mjs`: discovery, pairing, REST; `entertainment.test.mjs`:
the Entertainment path against a fake bridge). The host's DTLS and HTTPS are tested on loopback
(`crates/lightloom-plugin-host/tests/network_secure.rs`). The manifest's `tested` list stays empty.

## Two ways to drive the lamps

| | Entertainment API | CLIP v1 REST |
|---|---|---|
| when | the bridge serves CLIP v2, Lightloom holds its `clientkey`, and an entertainment area (configured in the Hue app) has lamps | always available |
| set up | CLIP v2 over HTTPS (port 443) | none |
| colours | one HueStream v2 frame per `Render()` (about 30 per second) over DTLS 1.2 PSK, UDP 2100, for every lamp of the area | one `PUT` per 100 ms for the whole bridge, round-robin |
| lamps | the area's lamps | every lamp outside the area, and every lamp when streaming cannot start |

Placement on the canvas does not depend on the path: the subdevices are always the CLIP v1
colour lamps (`light-<id>`), and each entertainment channel shows the colour of the lamp it
belongs to. A gradient lamp has several channels; they all show the lamp's one colour.

Bridges still serve CLIP v1 on `http://<bridge>/api/...`; if a future firmware stops doing that,
the REST path stops working.

## Discovery

- **mDNS** `_hue._tcp.local.`: the TXT records carry `bridgeid` and `modelid`. A reply with a
  `bridgeid` becomes a controller at once, without a request; one without it is probed like
  *Add by IP*.
- **Add by IP** (`forceDiscover(ip)`): `GET http://<ip>/api/config` (allowed without a
  username) answers `name`, `bridgeid`, `apiversion`, `modelid`, ...; the address is accepted
  when `bridgeid` is there.

The controller id is the bridge id, lower case (`001788fffe1a2b3c`). The plugin saves
`<bridgeid>/ip` and the list of known bridge ids (`bridges/list`), so a restart finds bridges
that were added by IP.

## Pairing (link button)

The dashboard's **Pair** calls `startLink()`. For 30 s (one try at once, then one per service
`Update()`, about every second) the plugin sends

```
POST http://<bridge>/api
{"devicetype":"lightloom#pc","generateclientkey":true}
```

- `[{"error":{"type":101,...}}]` ("link button not pressed"): keep trying, the status says
  *Press the link button on the bridge*;
- `[{"success":{"username":"<u>","clientkey":"<32 hex digits>"}}]`: the username is saved as
  setting `<bridgeid>/username` and the clientkey (the Entertainment PSK; bridges with API
  1.22 and later return it) as `<bridgeid>/clientkey`; `paired` becomes `true`, and the lamp
  list is read;
- any other error stops pairing and shows the bridge's description.

After 30 s without success the status says pairing timed out. A bridge paired by an earlier
version of the plugin has no clientkey: it keeps working over REST; pair again (Forget, then
Pair) to stream.

The lamp list is `GET http://<bridge>/api/<u>/lights`. Lamps with `state.xy` or a type of
`Color light` / `Extended color light` are kept (colour temperature and dimmable lamps are
not), as `{ id, name, type, modelid, gamut }`: `gamut` is `capabilities.control.colorgamut`,
else the triangle of `capabilities.control.colorgamuttype` (A, B or C), else `null`. With a
clientkey the entertainment area is read next (below); then the bridge is announced. On
restart a bridge with a stored username is announced once its lamps are read; a bridge that
does not answer is retried after 30 s. An `unauthorized user` error (type 1, or HTTP
401/403) clears the stored username and clientkey: `paired` is `false` again and an announced
bridge is suppressed.

**Forget** removes the username, the clientkey, the area choice, the address and the index
entry, suppresses the bridge and ignores it on mDNS for the rest of the session (*Add by IP*
brings it back).

## Entertainment area (service, CLIP v2 over HTTPS)

With a clientkey, four requests, each with the header `hue-application-key: <username>`:

1. `GET https://<bridge>/clip/v2/resource/entertainment_configuration`: the areas. The one
   whose `id` is the setting `<bridgeid>/area` is used, else the first. Each channel
   (`channel_id`) lists `members`, entertainment services (`service.rid`).
2. `GET .../resource/entertainment`: each entertainment service's `renderer_reference` is the
   CLIP v2 light that shows it.
3. `GET .../resource/light`: each light's `id_v1` (`/lights/<n>`) is its CLIP v1 id, the
   subdevice it belongs to.
4. `GET https://<bridge>/auth/v1`: the `hue-application-id` response header, the PSK identity
   of CLIP v2 streaming.

The announced controller then carries `clientkey`, `appId` and
`entertainment: { id, name, channels: [{ id, light }] }` (at most 20 channels; channels with no
CLIP v1 lamp are dropped). Any of the first three failing (a bridge without CLIP v2 answers 404,
an old one nothing on 443) leaves `entertainment` null: REST only, with a line in the service
log. The status shows `N colour lamps; streams to "<area>"`.

**HTTPS.** Hue bridges present a self-signed certificate (newer ones one signed by Signify's
own CA) whose common name is the bridge id. Every https request of the plugin sets
`xhr.tlsName` to the bridge id, so the host accepts only a certificate with that name (or the
one pinned for the host after a first such match); see [network-lightloom.md](../host-api/network-lightloom.md#https)
and [security.md](../security.md). The plugin never uses trust on first use.

## Streaming (device context)

**Initialize**, when the controller has an area with known lamps and a 32-hex-digit clientkey:

1. `PUT https://<bridge>/clip/v2/resource/entertainment_configuration/<area id>`
   `{"action":"start"}` (synchronous). Not 200: REST for every lamp, logged.
2. `device.addFeature("dtls")`, then `dtls.createConnection(<bridge>, 2100, identity, clientkey)`
   with identity = the application id. If that handshake fails, once more with the username
   (Philips' Entertainment API v1 used the username as PSK identity; which one CLIP v2
   bridges accept is the open question of this plugin, so it tries both). A handshake not done
   in 10 s, or both identities refused: REST for every lamp, the area is stopped.

**Render**: once the connection is established, one frame per call, all channels:

```
"HueStream"                     9 bytes ASCII
0x02 0x00                       version 2.0
seq                             1 byte, counts up (the bridge ignores it)
0x00 0x00                       reserved
0x00                            colour space: RGB
0x00                            reserved
<entertainment area id>         36 bytes ASCII (the UUID)
per channel: id, R, G, B        1 + 3 x 2 bytes, 16-bit big endian (8-bit value v sent as v * 257)
```

Colours go as the canvas has them: the bridge does the colour conversion for each lamp. The
bridge ends a stream that gets no frame for 10 s; a frame goes out on every `Render()`, changed
colours or not.
While handshaking, the area's lamps get nothing; lamps outside the area get REST commands as
below. A stream the bridge closes, or a DTLS error later, falls back to REST for every lamp
(the area is stopped asynchronously).

**Shutdown**: `dtls.CloseConnection()`, then `PUT .../entertainment_configuration/<id>`
`{"action":"stop"}` (synchronous), then the REST restore of every lamp (below).

## Colours over REST

Each lamp's subdevice is 1 x 1 with one LED. Its colour becomes a CIE xy point following
Philips' *RGB to xy Color conversion*:

1. each channel `v = c / 255`, gamma: `v > 0.04045 ? ((v + 0.055) / 1.055)^2.4 : v / 12.92`;
2. Wide RGB D65: `X = 0.664511 r + 0.154324 g + 0.162028 b`,
   `Y = 0.283881 r + 0.668433 g + 0.047685 b`, `Z = 0.000088 r + 0.072310 g + 0.986039 b`;
3. `x = X / (X + Y + Z)`, `y = Y / (X + Y + Z)`;
4. outside the lamp's gamut triangle: the closest point on its three edges. Gamut tables:
   A red (0.704, 0.296) green (0.2151, 0.7106) blue (0.138, 0.08); B (0.675, 0.322)
   (0.409, 0.518) (0.167, 0.04); C (0.692, 0.308) (0.17, 0.7) (0.153, 0.048). With no gamut
   known the point is sent as is and the bridge clamps it.

`xy` is rounded to 4 decimals. **Brightness (Lightloom choice):** `bri` is the brightest channel
scaled to 1..254, not the `Y` of step 2 that the guide suggests. The bridge already treats
`bri` as relative to the brightest the lamp can do at that xy, and `Y` would show full blue
at `bri` 12 (about 5 %).

```
PUT http://<bridge>/api/<u>/lights/<id>/state
{"on":true,"xy":[x,y],"bri":b,"transitiontime":1}      black: {"on":false}
```

Worked examples (by hand, with the arithmetic) are in `plugins/philips-hue/fixtures/xy.json`,
e.g. red on gamut C is the red corner `[0.692,0.308]`, white is `[0.3227,0.329]`.

## Rate limit (REST)

Philips asks for about 10 light commands per second per bridge (more and the bridge queues and
drops). Lightloom renders every 33 ms; the plugin sends at most **one PUT per 100 ms per bridge**,
round-robin over the lamps whose colour moved by 3 or more on any channel since the last
command (or turned black / back on), and never has more than one request in flight per lamp.
Streamed lamps are skipped. A failed command is logged once and resent later. With many lamps
each lamp's update rate drops accordingly (10 lamps changing: about one update per lamp per
second); the Entertainment API is the way around that.

## Start and stop

- **Initialize:** `setName`, `setSize([1, 1])` and no LEDs on the bridge itself, one subdevice
  per lamp (`light-<id>`, named after the lamp, 1 x 1, LED at (0, 0)); then one synchronous
  `GET /api/<u>/lights` keeps each lamp's `state`; then streaming starts when possible.
- **Shutdown:** streaming stops (above); then one synchronous PUT per lamp whose state was
  read: `{"on":false}` when it was off, else `{"on":true,"bri":...}` plus `ct` (colour mode
  `ct`), `hue`+`sat` (mode `hs`) or `xy`. A lamp whose state could not be read keeps its last
  colour.

## What goes on the LAN

Only to the bridge:

- port 80, plain HTTP: `GET /api/config` (no credentials), the pairing `POST /api`,
  `GET /api/<u>/lights`, `PUT .../lights/<id>/state`. The username is sent in the clear in
  every URL (CLIP v1 has no other way); anyone on the LAN who sees it can control the lamps.
- port 443, HTTPS with the certificate checked as above: the four CLIP v2 reads and the area's
  start/stop, the username in the `hue-application-key` header.
- UDP 2100, DTLS 1.2 PSK: the colour frames. The clientkey never leaves the machine (it is the
  PSK); the identity (application id or username) is sent in the clear in the handshake.

Nothing goes to the Hue cloud.

## Not implemented

- A dashboard choice of the entertainment area: the setting `<bridgeid>/area` (an area id) is
  read but only settable through the settings store today; without it the first area is used.
- Channel positions, gradient segments with their own colours, and the XY colour space of
  HueStream (RGB only).
- Entertainment API v1 (CLIP v1 groups of type `Entertainment`, HueStream v1): bridges old
  enough to lack CLIP v2 use REST.
- Groups, scenes, colour-temperature-only and dimmable lamps.
- Lamps added to the bridge later appear after the service restarts (or after Forget and a
  new pairing).

## Needs a bridge to confirm

- The PSK identity CLIP v2 bridges accept (the plugin tries the application id, then the
  username), and that `TLS_PSK_WITH_AES_128_GCM_SHA256` with an empty extensions block is
  what the bridge negotiates.
- The certificate's common name being the lower-case bridge id on current firmware.
- The HueStream v2 header and the `start` / `stop` actions as documented.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | Philips Hue developer documentation, "Getting started" (developers.meethue.com/develop/get-started-2/) | Link-button pairing, `POST /api` with `devicetype`, the username |
| S2 | Philips Hue CLIP API v1, "Lights API" (developers.meethue.com/develop/hue-api/lights-api/) | `GET /api/<u>/lights`, light `type`, `state` (`on`, `bri`, `xy`, `ct`, `hue`, `sat`, `colormode`), `PUT .../state`, `transitiontime` |
| S3 | Philips Hue CLIP API v1, "Configuration API" (developers.meethue.com/develop/hue-api/7-configuration-api/) | `GET /api/config` without a username (`bridgeid`, `name`, `modelid`, `apiversion`), `devicetype` limits, `generateclientkey` |
| S4 | Philips Hue CLIP API v1, "Error messages" (developers.meethue.com/develop/hue-api/error-messages/) | Error 1 unauthorized user, 101 link button not pressed |
| S5 | Philips Hue developer documentation, "RGB to xy Color conversion" (developers.meethue.com/develop/application-design-guidance/color-conversion-formulas-rgb-to-xy-and-back/) | Gamma, Wide RGB D65 matrix, gamut clamp, gamut tables A/B/C |
| S6 | Philips Hue developer documentation, "Hue system performance" / API rate guidance (developers.meethue.com) | About 10 light commands per second per bridge |
| S7 | Philips Hue developer documentation, "Hue Entertainment API" (developers.meethue.com/develop/hue-entertainment/) | DTLS 1.2 PSK on UDP 2100, `TLS_PSK_WITH_AES_128_GCM_SHA256`, clientkey as PSK, HueStream v2 frame, 10 s stream timeout, area `start` / `stop` |
| S8 | Philips Hue developer documentation, "CLIP API v2" (developers.meethue.com/develop/hue-api-v2/) | `https`, the `hue-application-key` header, `entertainment_configuration`, `entertainment` (`renderer_reference`), `light` (`id_v1`), `GET /auth/v1` and `hue-application-id`, the bridge certificate named after the bridge id |
| S9 | Philips Hue developer documentation, "Discovery" (developers.meethue.com/develop/application-design-guidance/hue-bridge-discovery/) | mDNS `_hue._tcp`, TXT `bridgeid` / `modelid` |
| S10 | SignalRGB plugin contract, `docs/host-api/network-services.md` §1.7 (static analysis; no SignalRGB code) | The `dtls` feature's member names |

No OpenRGB or SignalRGB code was used for this plugin.
