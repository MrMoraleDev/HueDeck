<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Govee Wi-Fi lights (LAN API)

The plugin is `plugins/govee/`. It drives Govee Wi-Fi lights through Govee's public LAN API:
UDP JSON messages on the local network, no Govee account and no cloud.

**Status: untested.** Nobody working on Lightloom owns a Govee light. The messages are written
from Govee's LAN API document and, for the optional per-segment mode, from OpenRGB's code.
Every test runs against mocks.

## Before you start

In the **Govee Home** app, open the light's settings and turn on **LAN Control**. Only models
on Govee's LAN API list have that switch. A light without it never answers the scan.

## Discovery (service context)

| What | Where |
|---|---|
| Scan request `{"msg":{"cmd":"scan","data":{"account_topic":"reserve"}}}` | multicast to `239.255.255.250:4001`, at start and every 60 s |
| Scan reply `{"msg":{"cmd":"scan","data":{"ip","device","sku","bleVersionHard",...}}}` | from the light to our UDP port **4002** (the service listener) |

Each reply becomes a controller with id = `data.device` (the light's MAC-like id), name
`Govee <sku>`, `ip`, `port` 4003 and `sku`. It is announced once. It is announced again only
when its IP or SKU changes. A light that has not answered for three rounds (about 3 min) is
suppressed.

**Add by IP** (`forceDiscover`) sends the same scan by unicast to `<ip>:4001`, from a socket the
service opens, and repeats it every round. The IP is stored in the plugin settings
(`forced`/`ips`). Until the light answers, the dashboard shows a placeholder ("waiting for a
scan reply"). **Forget** removes the light and its stored IP.

## Device session

All commands go to `<ip>:4003` as JSON over UDP, from a socket bound to an ephemeral port.

* **Initialize:** `devStatus` (the reply is kept for Shutdown), `turn` `{"value":1}`, then
  `brightness` `{"value":100}`. The canvas colour carries the intensity.
* **Render, single colour (default):** the light is one LED (`Size [1,1]`).
  `colorwc` `{"color":{"r","g","b"},"colorTemInKelvin":0}` is sent at most 10 times a second
  and only when the colour changed. The last colour is re-sent every 30 s as a keep-alive,
  like OpenRGB's keep-alive thread.
* **Render, per-segment mode** (parameter *Per-segment streaming*, off by default, plus
  *Segments*, 1..255): the layout becomes `Segments × 1`. Each frame is OpenRGB's `razer`
  command: `{"msg":{"cmd":"razer","data":{"pt":base64(BB len_hi len_lo B0 01 count RGB…
  xor)}}}`, with `len = 2 + 3·count` and `xor` over all the preceding bytes. Segment mode is
  entered with `BB 00 01 B1 01 0A` (re-sent with the 30 s keep-alive) and left with
  `BB 00 01 B1 00 0B`. This command is **not** in Govee's public LAN document. Segment counts
  OpenRGB lists: H619A 20, H70B1 20, H6046 20 (two bars), H612F 12, H6022 132, H607C 174.
* **Shutdown:** segment mode is left. When a `devStatus` reply was received, the plugin restores
  the colour (or the white temperature, through `colorTemInKelvin`), then the brightness, then
  the power. Without that reply the light keeps the last colour. Suspend does the same.

## Ports and what goes on the LAN

| Port | Direction | Content |
|---|---|---|
| UDP 4001 | out (multicast, and unicast to added IPs) | scan requests |
| UDP 4002 | in (bound by the service) | scan replies |
| UDP 4003 | out | turn / brightness / colorwc / devStatus / razer |

Nothing leaves the LAN. The scan carries no account data: `account_topic` is the literal
`"reserve"`.

## Limits and what is not implemented

* **devStatus replies may never reach the device session.** Third-party implementations
  report that Govee lights answer on UDP port 4002, which the service context already holds.
  The device session only sees replies sent back to its own socket. If the light answers
  on 4002, the restore does not happen and the light keeps the last colour (logged).
  Restoring reliably needs a way to route a light's 4002 datagrams from the service
  to its device session (host API).
* The SKU is not used to pick the segment count. The user sets *Segments*.
* Scenes, music modes, DIY effects and the cloud API are not used.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | Govee, "LAN API" developer document / WLAN guide, https://app-h5.govee.com/user-manual/wlan-guide | ports 4001/4002/4003, multicast group, `scan`, `turn`, `brightness`, `colorwc`, `devStatus`, the LAN Control switch |
| S2 | OpenRGB `Controllers/GoveeController/GoveeController.cpp` and `GoveeController.h` (GPL-2.0-or-later, commit `d4798c12`, © 2023 Adam Honse) | `razer` frames (`SendRazerData`, `SendRazerEnable`, `SendRazerDisable`, XOR checksum), the scan's `account_topic` |
| S3 | OpenRGB `Controllers/GoveeController/RGBController_Govee.cpp` (GPL-2.0-or-later, commit `d4798c12`, © 2023 Adam Honse) | per-SKU segment counts, the 30 s keep-alive that re-sends the razer enable |
| S4 | homebridge-govee wiki "LAN Control", https://github.com/homebridge-plugins/homebridge-govee/wiki/LAN-Control | confirms that replies arrive on port 4002 |
