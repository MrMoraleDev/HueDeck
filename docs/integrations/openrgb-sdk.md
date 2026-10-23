# OpenRGB SDK server

Lightloom can serve the OpenRGB SDK network protocol. Programs that speak it can then see Lightloom's devices and set their LEDs:
- OpenRGB in client mode, and OpenRGB plugins
- Artemis
- Aurora
- openrgb-python
- OpenRGB.NET

The server is off by default.

## Turning it on

- **Dashboard:** Settings → OpenRGB SDK → "Let OpenRGB SDK clients drive Lightloom's devices".
- **API:** `PATCH /v1/settings {"openrgbSdk": true}`.

The setting is saved, and the server starts or stops within about a second.

- **Address:** `127.0.0.1:6742`, loopback only. The protocol has no authentication, so it is never offered to the network.
- **Port in use:** another program may already listen on 6742. Usually that is OpenRGB's own SDK server. Lightloom then logs a warning once and tries again every 30 s. Windows lets a second program bind 127.0.0.1 next to a wildcard listener, so Lightloom also checks the listener table and treats a port another program listens on as busy. To use Lightloom's server, stop OpenRGB's server.
- **Connections:** at most 32 clients at once.

## What clients see

**Controllers.** There is one controller per running Lightloom device that has LEDs. Stopped, disabled and absent devices are left out.

| Field | Value |
|---|---|
| type | From the plugin's `DeviceType()`: motherboard 0, ram 1, gpu 2, aio 3, lightingcontroller 4, keyboard 5, mouse 6, mousepad 7, headphones 8, speakers 12, case 15, microphone 16. Anything else is 17 (Accessory). Clients older than OpenRGB 0.9 show the larger numbers as Unknown. |
| name | the device name |
| vendor | the plugin name |
| description | `<name> (Lightloom plugin <plugin>)` |
| version | `Lightloom <version>` |
| serial | the device key (`vid:pid:serial`) |
| location | `Lightloom: <device key>` |
| modes | one mode, `Direct`: per-LED colours, colour mode per-LED, active |
| zones | one per canvas endpoint with LEDs (see below) |
| LEDs | `<zone> LED <n>` |
| colours | black. Lightloom does not report what the canvas shows. |

**Zones.** Each canvas endpoint that has LEDs becomes one zone, in the dashboard's endpoint order:
1. the device body, named `Main`;
2. each subdevice;
3. each component on a channel, named `<channel>: <component>`.

A zone is Linear (Single when it has one LED), with no matrix map and no segments. Its minimum, maximum and current size are all the same.

LED order within a zone:
- **Device body:** `LedPositions()` order.
- **Subdevice:** its grid cells, row by row.
- **Component:** the component's own LED order. Its LED mapping still applies on the wire.

## Driving LEDs

UPDATELEDS, UPDATEZONELEDS and UPDATESINGLELED write *direct colours* for the zones they touch. Direct colours replace the canvas, and the endpoint's forced colour, for those endpoints only. Every other endpoint keeps following the effect.

How long direct colours last:
- They stay while the client is connected, even if it sends a colour only once (OpenRGB's GUI does that).
- When the client disconnects, or the server is turned off, its endpoints go back to the canvas on the next frame.
- If the server stops refreshing them without closing the connection (it hangs), they go back after 5 s.
- When two clients drive the same zone, the last writer wins.

Brightness still applies to direct colours: global × layout × endpoint brightness, the same as for canvas colours.

**Limits.** Payloads above 1 MiB, and packets without the `ORGB` magic, close the connection. Malformed LED packets are ignored. There is no rate limit: the device plugins already render at the canvas frame rate, so faster updates are simply coalesced.

## Protocol coverage

- **Versions:**
  - Protocol versions 0 to 4 are supported.
  - The server answers REQUEST_PROTOCOL_VERSION with 4, and uses the lower of the client's version and 4.
  - A client that never negotiates gets version 0: no vendor string, and no DEVICE_LIST_UPDATED.
  - Versions 5 and 6 are not served (zone and controller flags, alternative LED names, unique ids, ACKs and the v6 managers). Clients that support older servers negotiate down to 4.

| Packet | Behaviour |
|---|---|
| 0 REQUEST_CONTROLLER_COUNT | Count, and a fresh index snapshot for this client. |
| 1 REQUEST_CONTROLLER_DATA | Controller data at the requested version (capped at 4). The reply header carries the requested index. An unknown index gets no reply. |
| 40 REQUEST_PROTOCOL_VERSION | Replies 4. |
| 50 SET_CLIENT_NAME | Logged. |
| 100 DEVICE_LIST_UPDATED | Sent (version 1 and later) when the controller list or a zone size changes; checked every second. Until the client asks for the count again, its LED writes are ignored, because its indexes are stale. |
| 140 RESCAN, 201 PLUGIN_SPECIFIC | Ignored. |
| 150 PROFILE_LIST | An empty list. |
| 151–153 SAVE/LOAD/DELETE_PROFILE | Not supported; ignored. Profiles live in Lightloom (layouts, effects). |
| 200 PLUGIN_LIST | An empty list. |
| 1000 RESIZEZONE | Refused and logged. A zone's size comes from Lightloom: assign components to channels in the dashboard's component editor. A resize would restart the device session and change the canvas layout, which an SDK client should not do behind the user's back. |
| 1050 UPDATELEDS | Controller-wide colours, split across its zones. |
| 1051 UPDATEZONELEDS | One zone's colours. |
| 1052 UPDATESINGLELED | One LED; the index is controller-wide. |
| 1100 SETCUSTOMMODE | Accepted: Direct is always active. |
| 1101 UPDATEMODE, 1102 SAVEMODE | Accepted for mode 0 (Direct). Nothing is saved to any device. |
| other ids | Ignored. Their payload is consumed. |

## How it is built

- `crates/lightloom-api/src/openrgb/`:
  - `wire.rs`: framing and encoding.
  - `mod.rs`: the supervisor that follows the setting, the accept loop and the per-client sessions.
- `crates/lightloom-engine/src/direct.rs`, `DirectLeds`: the shared direct-colour table. The daemon owns it in `engine::Shared` and exposes it through `Backend::direct()`. Device sessions read it once per frame, via `SessionOptions::direct`, `DeviceContext::apply_direct` and `Children::apply_direct`.
- LED writes do not go through the engine thread, so they never rebuild the snapshot.

The server was written from protocol facts (`docs/research/m6-windows-integrations.md` §2). No OpenRGB code or documentation text was copied.

## Not tested yet

- No real SDK client has been tried yet: not OpenRGB 1.x in client mode, not Artemis, not openrgb-python. The tests use byte goldens built from the documented layouts, and a TCP session against a fake daemon.
- Whether third-party clients accept a controller with only a `Direct` mode still needs checking on the user's machine.
