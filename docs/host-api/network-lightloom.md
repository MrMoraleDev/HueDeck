<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Network plugins in Lightloom

What Lightloom implements of the SignalRGB network-plugin contract described in
[network-services.md](network-services.md), and where it differs. Plugins written against this
page run in `lightloomd`; first-party ones live in `plugins/<name>/` and are tested with
`plugins/_testkit/mock-net.mjs`.

## Privacy and safety rules

* **Off until turned on.** Network discovery runs only while the dashboard setting *Network
  lights* (`settings.network`, config `[network] enabled`) is on. Turning it off stops every
  service and network device session; their sockets close with them.
* **LAN only.** Every destination (UDP, TCP, HTTP) must be a loopback, private (RFC 1918,
  `fc00::/7`), link-local, local multicast (`224.0.0.0/24`, `239.0.0.0/8`, `ff02::/16`) or
  broadcast address. Anything else fails like an unreachable host (`send` returns `-1`, an
  XHR ends with status 0) and is logged once. Host names are resolved first and the check
  applies to the address that is connected to. There is no proxy and no `https`.
* **Sockets live with the plugin.** A socket exists from `createSocket()` until `close()` or
  the end of the service/session that made it. UDP sockets bind `0.0.0.0` (ephemeral port
  unless the plugin asks for one) because replies may arrive on any interface. TCP servers
  never bind `0.0.0.0`: `listen(port, peerIp)` (Lightloom extension) binds the local address of
  the interface that faces `peerIp` and accepts connections from `peerIp` only (Yeelight
  music mode is the one first-party user).
* **Dry run** (`lightloomd --dry-run`): device sessions record UDP/TCP sends and non-GET HTTP
  requests without sending them; discovery (service context) runs normally.
* **Time limits.** Synchronous XHR: the request's `timeout` (default 3 s, at most 10 s).
  TCP connect: at most 2 s. Time blocked in network calls does not count against the
  JavaScript budget of the call, but the wall-clock cap still applies.

## The two contexts

As in SignalRGB, one plugin file runs in two kinds of context.

### Service context (one per plugin, thread `lightloom-service <Name()>`)

1. The module is evaluated; `Type()` must be `"network"` and `DiscoveryService` an exported
   function or class.
2. `discovery = new DiscoveryService()` is bound to the global `discovery`. Lightloom reads
   `IconUrl`, `MDns` (array of DNS-SD types such as `"_wled._tcp.local."`),
   `UdpBroadcastAddress` (default `255.255.255.255`), `UdpBroadcastPort` and `UdpListenPort`.
3. Discovery sockets are opened: a broadcast socket that sends to
   `UdpBroadcastAddress:UdpBroadcastPort`, and a listener bound to `UdpListenPort` when set.
   **Deviation:** the broadcast socket uses an ephemeral local port (SignalRGB binds
   `UdpBroadcastPort` locally) and joins no multicast group: replies to a scan come back to
   the port it was sent from, or to the listen port.
4. `Initialize()` is called once, then `Update()` about every second, `Discovered(value)` for
   every datagram on the discovery sockets and every mDNS answer, `Removal(value)` when an
   mDNS service stops answering. Socket events and asynchronous XHRs are delivered every
   50 ms between those calls.

`Discovered(value)` shapes:

| source | value |
|---|---|
| UDP datagram | `{ id, ip, port, response, buffer }`: `response` is the payload as text, `buffer` its bytes. **Deviation:** `id` is the sender's IP address (SignalRGB looks up its MAC in the ARP table). |
| mDNS | `{ id, name, type, hostname, port, ip, ...txt }`: `name` is the instance name, `hostname` ends with a dot, `ip` the first IPv4 address, TXT records flattened (keys lowercase). `id` is the TXT `id` when present, else the instance name. |
| *Add by IP* (dashboard) | Lightloom calls `discovery.forceDiscover(ip)` when the plugin defines it, else `Discovered({ id: ip, ip, port: 0, response: "", manual: true })`. |

mDNS is browsed with one-shot queries (RFC 6762 §5.1, sent from an ephemeral port, so the
answers come back by unicast and Lightloom never binds port 5353): at start, after 2 s and 6 s,
then every 30 s. A service that has not answered for 95 s is reported through `Removal`.

The `service` global:

| member | Lightloom |
|---|---|
| `log(msg, {pretty})` | plugin log (dashboard event log) |
| `controllers` | array of `{ id, obj }` wrappers |
| `addController(obj)` / `removeController(obj)` / `updateController(obj)` | the list the dashboard shows; `obj` (or a wrapper) must have an `id`; more than 50 logs a warning |
| `getController(id)` / `hasController(id)` | lookup |
| `announceController(obj)` | creates the network device `net:<plugin>:<id>`: the controller's enumerable data is deep-copied (functions dropped) into a new device context. Announcing the same id again with different data restarts that device's session. |
| `suppressController(obj)` | stops the device's session (its `Shutdown(false)` runs) and removes it |
| `getSetting(id, key)` / `saveSetting(id, key, value)` / `removeSetting(id, key)` | per-plugin settings in the daemon database (`plugin_settings`, keyed by `Name()`). `id` and `key` must not contain `/` or `..` (throws). Values are strings; other values are stored as JSON text; `undefined` deletes. |
| `broadcast(payload)` | sends a string or byte array to `UdpBroadcastAddress:UdpBroadcastPort` from the broadcast socket |
| `resolve(hostname, cb)` | system resolver; `cb({ protocol: "IPV4" \| "IPV6", ip })` per LAN address, before `resolve` returns |
| `callDiscoveryFunction`, `callControllerFunction`, `resolveMacAddress`, `saveSnapshot`, `showSettings`, `hideSettings`, `ignoreSslErrors` | not provided (API gap) |

The QML UI of SignalRGB plugins is not loaded. The dashboard's *Network* section shows, per
plugin, `service.controllers` with these plain fields of `obj` when present: `name`, `ip`,
`status` (short text), `paired` (boolean). It offers:

* **Pair** when `obj.startLink` is a function and `obj.paired !== true`: calls
  `obj.startLink()` (Hue/Nanoleaf convention: then press the button on the device).
* **Forget** when `discovery.forgetController` is a function: calls
  `discovery.forgetController(id)`.
* **Add by IP**: see above.

### Device context (one per announced controller, a device session)

The normal device session ([hid-usb.md](hid-usb.md), [components-channels.md](components-channels.md))
without HID: no `Validate`, `Initialize()`, `Render()` at most every 33 ms, `Shutdown(false)`.
Globals: `device`, `controller` (the announced data), the `ControllableParameters` globals
(stored per device key like any device), `XMLHttpRequest`. `device.addFeature("udp")` installs
`udp.send(ip, port, bytes, endianness)` (8192 bytes at most; `endianness` ignored for byte
arrays); `device.addFeature("dtls")` is not provided (Hue Entertainment needs it). Socket
events and asynchronous XHRs are delivered before each `Render()`.

Device key `net:<plugin slug>:<controller id>`, VID:PID `0000:0000`; the device list shows the
controller's `ip`.

## Modules

`@SignalRGB/udp` and `@SignalRGB/tcp` in both contexts; `@SignalRGB/base64` as before.

```js
const s = udp.createSocket();       // tcp.createSocket() for TCP
s.on("message", (msg) => {});       // { data: string, buffer: number[], address, port, timestamp }
s.on("error", (code, message) => {});
s.on("listening", () => {});        // after bind()
s.on("connection", (client) => {}); // TCP: connected; listening socket: the accepted client
s.on("close", () => {});
s.bind(port);                       // UDP; 0 = ephemeral
s.connect(ip, port);                // UDP: default peer; TCP: connects (true/false, at most 2 s)
s.send(stringOrBytes);              // to the peer; bytes sent or -1
s.write(bytes, ip, port);           // UDP: to ip:port; bytes sent or -1
s.listen(port, peerIp);             // TCP, Lightloom extension: { address, port } or null
s.close(); s.disconnect();
s.address(); s.remoteAddress();     // { address, port, protocol }
s.state === s.ConnectedState;       // UnconnectedState .. ListeningState
```

Unknown event names log `on(): unknown event type`. A datagram larger than 8192 bytes is
refused (`-1`).

## XMLHttpRequest

`open(method, url, async = true)`, `setRequestHeader`, `send(body)`, `timeout`, `readyState`,
`status` (0: unreachable, refused or timed out), `statusText`, `responseText`, `response`
(parsed when `responseType` is `"json"`), `getResponseHeader`, `getAllResponseHeaders`,
`onreadystatechange`, `onload`, `onerror`, `ontimeout`, `abort`. Synchronous requests
complete inside `send()`; asynchronous ones complete at the next event delivery. `http://`
only; bodies up to 4 MiB.

## Where it lives

| piece | code |
|---|---|
| sockets, LAN rule, event queue, natives | `crates/lightloom-plugin-host/src/net/mod.rs`, `net/policy.rs` |
| HTTP client | `net/http.rs` |
| mDNS one-shot browser | `net/mdns.rs` |
| service thread, `service.*` natives, settings trait | `src/service.rs` |
| network device sessions (`SessionDevice::network`) | `src/session.rs` |
| JS side (sockets, XHR, `service`, `pumpNet`) | `js/host.js` |
| daemon: setting, services, announced devices, `plugin_settings` table | `crates/lightloom-daemon/src/engine.rs`, `store.rs` |
| API | `GET /v1/network`, `POST /v1/network/{plugin}/add` `{ip}`, `POST /v1/network/{plugin}/controllers/{id}/pair` and `/forget`, `PATCH /v1/settings {network}` |
| dashboard | `ui/src/components/Network.svelte` (Devices tab) |

## Not provided (yet)

* `device.addFeature("dtls")` and `https://`: Lightloom has no TLS stack. Philips Hue therefore
  uses the CLIP v1 REST API over HTTP; the Entertainment API (DTLS-PSK on UDP 2100) and CLIP
  v2 are future work.
* The sender's MAC address as the `id` of UDP discoveries (ARP lookup).
* `Hosts`, the QML UI (`<plugin>.qml`, `Interface()`), `callDiscoveryFunction` /
  `callControllerFunction`, `resolveMacAddress`, `saveSnapshot`, `showSettings` /
  `hideSettings`; addon installation from git repositories.
* `discovery.Shutdown()` is never called (as in SignalRGB).
* mDNS over IPv6, and on more than the default multicast interface.
* Datagrams arrive at the socket they are addressed to: a device that answers a device
  session's request on the service's fixed `UdpListenPort` (Govee `devStatus` on 4002) reaches
  the service, not the session.
