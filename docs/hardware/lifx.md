<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# LIFX Wi-Fi bulbs (LAN protocol)

The plugin is `plugins/lifx/`. It drives LIFX bulbs through LIFX's public LAN protocol: binary
messages over UDP port 56700, with no account and no cloud.

**Status: untested.** Nobody working on HueDeck owns a LIFX bulb. Every packet is worked out by
hand from the public documentation. The tests compare against those hand-made goldens in
`plugins/lifx/fixtures/expected.json`.

## Message format

Every message starts with a 36-byte header, little endian:

| Bytes | Field | Value here |
|---|---|---|
| 0-1 | size | 36 + payload |
| 2-3 | protocol 1024, addressable (bit 12), tagged (bit 13) | `00 14`, or `00 34` for discovery broadcasts |
| 4-7 | source | `0x6B656468` (`68 64 65 6B`), never 0 or 1 |
| 8-15 | target | the bulb's MAC + `00 00`; zeros for discovery |
| 16-21 | reserved | 0 |
| 22 | res_required / ack_required | 0 (Get messages always answer; streamed Sets need no ack) |
| 23 | sequence | counts up per message, wraps at 255 |
| 24-31 | reserved | 0 |
| 32-33 | type | message type |
| 34-35 | reserved | 0 |

## Discovery (service context)

* **GetService (2)** is broadcast to `255.255.255.255:56700` at start and every 30 s. It is
  also sent by unicast to every IP added by hand.
* A **StateService (3)** reply with service 1 (UDP) gives the bulb's MAC (header bytes 8..13)
  and port. The controller id is `lifx-<mac hex>`.
* The service then asks **GetLabel (23)** and **GetVersion (32)** from its own socket. The bulb
  is announced when **StateLabel (25)** arrives, named after its label. If no label arrives,
  it is announced after 3 s as `LIFX <last 6 hex digits of the MAC>`. **StateVersion (33)**
  stores the product number.
* A bulb that has not answered for three rounds (about 95 s) is suppressed. **Add by IP**
  stores the IP in the plugin settings (`forced`/`ips`). **Forget** removes it.

## Device session

* One bulb is one LED (`Size [1,1]`).
* **Initialize** sends **Get (101)**. The **LightState (107)** reply (HSBK, power, label) comes
  back to the session's socket before a later Render, and is kept for Shutdown.
  **SetLightPower (117)** then turns the bulb on (level 65535, duration 0).
* **Render** sends **SetColor (102)** with HSBK. Hue is degrees × 65535 / 360; saturation and
  brightness are scaled to 0..65535; kelvin is 3500; the transition is 50 ms. It is sent at
  most once every 50 ms (LIFX's guidance is no more than 20 messages a second per device) and
  only when the colour changed.
* **Shutdown** (and suspend) restores the captured HSBK with SetColor (duration 0), then the
  power with SetLightPower. Without a LightState reply the bulb keeps the last colour.

## Ports and what goes on the LAN

| Port | Direction | Content |
|---|---|---|
| UDP 56700 | out (broadcast and unicast) | GetService, GetLabel, GetVersion, Get, SetColor, SetLightPower |
| ephemeral UDP | in | StateService, StateLabel, StateVersion, LightState replies |

The only identifier the plugin sends is its fixed source id.

## Not implemented

* **Multizone (Z, Beam, strips) and matrix (Tile, Candle) devices:** each one gets a single
  colour for the whole device. SetColor applies to all zones. Per-zone streaming
  (SetExtendedColorZones 510, Set64 715) is future work.
* Acknowledgements and retries: streamed colours are not acknowledged, and a lost one is
  replaced by the next change.
* Waveforms, effects, infrared and HEV are not used.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | LIFX LAN Protocol, "Packet contents" / header description, https://lan.developer.lifx.com/docs/packet-contents | header layout, flags, source and sequence rules |
| S2 | LIFX LAN Protocol, "Changing a device", https://lan.developer.lifx.com/docs/changing-a-device | SetPower (21), SetColor (102), SetLightPower (117) payloads |
| S3 | LIFX LAN Protocol, "Querying the device for data" and "Information messages", https://lan.developer.lifx.com/docs | GetService/StateService, GetLabel/StateLabel, GetVersion/StateVersion, Get/LightState layouts |
| S4 | LIFX developer FAQ / LAN docs, https://lan.developer.lifx.com/docs | at most 20 messages per second per device |
