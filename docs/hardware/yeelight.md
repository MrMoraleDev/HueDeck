<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Yeelight Wi-Fi lights

The plugin is `plugins/yeelight/`. It drives Yeelight bulbs and strips through the Yeelight
WiFi Light Inter-Operation Specification: an SSDP-like discovery, then JSON commands over TCP
port 55443, with no account and no cloud.

**Status: untested.** Nobody working on Lightloom owns a Yeelight. Everything is written from
the public specification, and the tests run against mocks.

## Before you start

In the **Yeelight** app, open the light's settings and turn on **LAN Control**. Without it the
light neither answers discovery nor accepts connections.

## Discovery (service context)

* `M-SEARCH * HTTP/1.1` / `HOST: 239.255.255.250:1982` / `MAN: "ssdp:discover"` /
  `ST: wifi_bulb` is sent to `239.255.255.250:1982` at start and every 60 s.
* Search replies (`HTTP/1.1 200 OK`) and the lights' own `NOTIFY` advertisements are parsed the
  same way: they are header lines with `Location: yeelight://ip:port`, `id`, `model`,
  `fw_ver`, `support`, `power`, `bright`, `rgb`, `name` and so on. They become a controller
  with `{ id, name (or "Yeelight <model>"), ip, port, model, support: [...] }`. It is
  announced again only when the IP, port, model, name or support list changes.
* A light silent for three rounds (about 3 min) is suppressed.
* **Add by IP** creates a controller with the generated id `forced-<ip>`, port 55443 and an
  unknown support list (treated as a colour light with music mode). It is announced at once,
  and the IP is stored in the plugin settings (`forced`/`ips`). When discovery later finds a
  light at that IP, the discovered one replaces it. **Forget** removes the light and the stored IP.

## Device session

1. **Control connection** `tcp.connect(ip, 55443)`. Commands are
   `{"id":N,"method":...,"params":[...]}\r\n`, with the id counting up.
   `get_prop ["power","bright","rgb","color_mode","ct"]` is sent first. The reply arrives on
   the socket's `message` callback, possibly split across reads, and is kept for Shutdown.
2. **Music mode.** The plugin calls `server = tcp.createSocket()`, then
   `server.listen(0, controller.ip)`, a Lightloom extension: it binds the LAN address facing
   the light and accepts only that light. It then sends
   `set_music [1, address, port]` on the control connection. Once the light connects back,
   the plugin sends `set_power ["on","sudden",0]` once. After that, each frame sends
   `set_rgb [rgb,"sudden",0]` and/or `set_bright [b,"sudden",0]` on the music connection,
   only on change and at most every 33 ms. Music mode has no command quota.
3. **Fallback.** This happens when `listen()` returns null, the light lacks `set_music`, it
   does not connect within 5 s, or it closes the music connection. The control connection
   is then used for at most **one command a second** (lights accept 60 commands a minute),
   and the commands that are still needed follow on later turns.
4. **Colour mapping.** The brightest channel is scaled to 255 for `set_rgb`, and
   `set_bright = round(max × 100 / 255)`, at least 1. Black is sent as
   `set_power ["off","sudden",0]`; the next colour turns the light back on.
5. **White-only lights** (no `set_rgb` in `support`, e.g. model `mono`) get `set_bright` and
   power only.
6. **Shutdown** (and suspend) closes the music connection and sends `set_music [0]`, but
   only if music mode was requested. It then restores what `get_prop` reported, on the
   control connection:
   * the light is turned on first if the last frame was black and it was on before;
   * then `set_ct_abx [ct]` when `color_mode` was 2, else `set_rgb [rgb]`;
   * then `set_bright`;
   * then `set_power off` if the light was off before.

   All sockets are closed. Without a `get_prop` reply the light keeps the last colour.
7. If the control connection cannot be opened, the plugin retries every 30 s.

## Ports and what goes on the LAN

| Port | Direction | Content |
|---|---|---|
| UDP 1982 | out (multicast) / in | M-SEARCH, replies and NOTIFY adverts |
| TCP 55443 | out | control connection (get_prop, set_music, fallback commands, restore) |
| ephemeral TCP, LAN address only | in, from the light only | music connection (colour stream) |

## Limits and what is not implemented

* Restoring in fallback mode can briefly go over the 60 commands/minute quota (up to four
  restore commands right after a minute of 1/s commands). The light then answers with an
  error, which is logged.
* HSV mode (`color_mode` 3) is restored through its `rgb` value.
* Background lights (`bg_*` methods, e.g. on ceiling lights), flows, scenes and `set_name`
  are not used.
* `name` from discovery is used as sent. Non-ASCII names may be base64 on some firmware.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | Yeelight, "Yeelight WiFi Light Inter-Operation Specification" (PDF), https://www.yeelight.com/download/Yeelight_Inter-Operation_Spec.pdf | discovery (M-SEARCH, replies, NOTIFY, header fields), command framing, `get_prop`, `set_rgb`, `set_bright`, `set_power`, `set_ct_abx`, `set_music`, the 60 commands/minute quota, music mode, the LAN Control switch |
