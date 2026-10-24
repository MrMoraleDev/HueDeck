<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Philips Hue bridges (CLIP v1 REST API over HTTP)

The plugin is `plugins/philips-hue/`. It drives the colour lamps of a Philips Hue bridge
through the bridge's LAN REST API (CLIP v1) over plain HTTP, port 80. Every colour lamp is
its own subdevice with one LED, so each lamp is placed on the canvas on its own. The bridge
itself has no LEDs (`SubdeviceController()` is `true`).

**Status: untested.** Nobody working on Lightloom owns a Hue bridge. The plugin is written from
Philips' public developer documentation and checked only against the mocks in
`plugins/philips-hue/tests/`.

## Why CLIP v1 over HTTP

Lightloom has no TLS or DTLS stack (see [network-lightloom.md](../host-api/network-lightloom.md):
`http://` only, no `dtls` feature). That rules out two Hue interfaces for now:

- **CLIP v2** (`https://<bridge>/clip/v2/...`) is served over HTTPS only.
- **Entertainment API** streaming (the low-latency path, up to about 25 updates per second for
  a whole entertainment area) runs over DTLS 1.2 with a pre-shared key on UDP port 2100, and
  is set up through CLIP v2.

Both are future work once the host has TLS and DTLS-PSK. Bridges still serve CLIP v1 on
`http://<bridge>/api/...`; if a future bridge firmware stops doing that, this plugin stops
working and needs CLIP v2.

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
{"devicetype":"lightloom#pc"}
```

- `[{"error":{"type":101,...}}]` ("link button not pressed"): keep trying, the status says
  *Press the link button on the bridge*;
- `[{"success":{"username":"<u>"}}]`: the username is saved as setting `<bridgeid>/username`,
  `paired` becomes `true`, and the lamp list is read;
- any other error stops pairing and shows the bridge's description.

After 30 s without success the status says pairing timed out.

The lamp list is `GET http://<bridge>/api/<u>/lights`. Lamps with `state.xy` or a type of
`Color light` / `Extended color light` are kept (colour temperature and dimmable lamps are
not), as `{ id, name, type, modelid, gamut }`: `gamut` is `capabilities.control.colorgamut`,
else the triangle of `capabilities.control.colorgamuttype` (A, B or C), else `null`. Then the
bridge is announced. On restart a bridge with a stored username is announced once its lamps
are read; a bridge that does not answer is retried after 30 s. An `unauthorized user`
error (type 1, or HTTP 401/403) clears the stored username: `paired` is `false` again and an
announced bridge is suppressed.

**Forget** removes the username, the address and the index entry, suppresses the bridge and
ignores it on mDNS for the rest of the session (*Add by IP* brings it back).

## Colours

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

## Rate limit

Philips asks for about 10 light commands per second per bridge (more and the bridge queues and
drops). Lightloom renders every 33 ms; the plugin sends at most **one PUT per 100 ms per bridge**,
round-robin over the lamps whose colour moved by 3 or more on any channel since the last
command (or turned black / back on), and never has more than one request in flight per lamp.
A failed command is logged once and resent later. With many lamps each lamp's update rate
drops accordingly (10 lamps changing: about one update per lamp per second); the Entertainment
API is the way around that.

## Start and stop

- **Initialize:** `setName`, `setSize([1, 1])` and no LEDs on the bridge itself, one subdevice
  per lamp (`light-<id>`, named after the lamp, 1 x 1, LED at (0, 0)); then one synchronous
  `GET /api/<u>/lights` keeps each lamp's `state`.
- **Shutdown:** one synchronous PUT per lamp whose state was read: `{"on":false}` when it was
  off, else `{"on":true,"bri":...}` plus `ct` (colour mode `ct`), `hue`+`sat` (mode `hs`) or
  `xy`. A lamp whose state could not be read keeps its last colour.

## What goes on the LAN

Only to the bridge, port 80, plain HTTP: `GET /api/config` (no credentials), the pairing
`POST /api`, `GET /api/<u>/lights`, and `PUT .../lights/<id>/state`. The username is sent in
the clear in every URL (CLIP v1 has no other way); anyone on the LAN who sees it can control
the lamps. Nothing goes to the Hue cloud.

## Not implemented

- CLIP v2 and the Entertainment API (no TLS / DTLS-PSK in the host; see above).
- Groups, scenes, colour-temperature-only and dimmable lamps.
- Lamps added to the bridge later appear after the service restarts (or after Forget and a
  new pairing).

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | Philips Hue developer documentation, "Getting started" (developers.meethue.com/develop/get-started-2/) | Link-button pairing, `POST /api` with `devicetype`, the username |
| S2 | Philips Hue CLIP API v1, "Lights API" (developers.meethue.com/develop/hue-api/lights-api/) | `GET /api/<u>/lights`, light `type`, `state` (`on`, `bri`, `xy`, `ct`, `hue`, `sat`, `colormode`), `PUT .../state`, `transitiontime` |
| S3 | Philips Hue CLIP API v1, "Configuration API" (developers.meethue.com/develop/hue-api/7-configuration-api/) | `GET /api/config` without a username (`bridgeid`, `name`, `modelid`, `apiversion`), `devicetype` limits |
| S4 | Philips Hue CLIP API v1, "Error messages" (developers.meethue.com/develop/hue-api/error-messages/) | Error 1 unauthorized user, 101 link button not pressed |
| S5 | Philips Hue developer documentation, "RGB to xy Color conversion" (developers.meethue.com/develop/application-design-guidance/color-conversion-formulas-rgb-to-xy-and-back/) | Gamma, Wide RGB D65 matrix, gamut clamp, gamut tables A/B/C |
| S6 | Philips Hue developer documentation, "Hue system performance" / API rate guidance (developers.meethue.com) | About 10 light commands per second per bridge |
| S7 | Philips Hue developer documentation, "Hue Entertainment" and "CLIP API v2" (developers.meethue.com) | Why they are not used: DTLS-PSK on UDP 2100, HTTPS only |
| S8 | Philips Hue developer documentation, "Discovery" (developers.meethue.com/develop/application-design-guidance/hue-bridge-discovery/) | mDNS `_hue._tcp`, TXT `bridgeid` / `modelid` |

No OpenRGB or SignalRGB code was used for this plugin.
