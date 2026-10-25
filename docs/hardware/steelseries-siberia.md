<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Siberia 350

Plugin: `plugins/steelseries-siberia/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesSiberiaController/SteelSeriesSiberiaController.cpp`, `.h` (E Karlsson (pilophae), 18 Jun 2020)
- `Controllers/SteelSeriesController/SteelSeriesSiberiaController/RGBController_SteelSeriesSiberia.cpp`, `.h` (E Karlsson (pilophae), 18 Jun 2020)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

| Product id | OpenRGB name | Endpoint (OpenRGB detector) | Detect.cpp line |
|---|---|---|---|
| `1038:1229` | SteelSeries Siberia 350 | interface 3, any collection (`REGISTER_HID_DETECTOR_I`) | 522 |

## Protocol

One LED, "Headset LED" (zone "Headset"). `send_usb_msg(data, size)` writes a 16-byte output
report: `01` (report id), `00` ("magic"), the command byte, the payload length (`size - 1`), the
payload, zeros.

**Start.** OpenRGB's only mode, Static, just sends the colour: nothing of its own.

**Every frame** (`SetColor(R, G, B)`), four reports:

| Bytes (hex, padded to 16) | OpenRGB comment |
|---|---|
| `01 00 95 02 80 BF` | Command 1 |
| `01 00 80 02 52 20` | Command 2 |
| `01 00 83 03 R G B` | Command 3 (Set color) |
| `01 00 93 02 03 80` | Command 4 |

The sequence is sent only when the colour changes, with no keepalive: commands 1, 2 and 4 are
not explained in OpenRGB and may be register writes of the headset's controller (possibly not
only lighting), so they go out as rarely as possible.

**Shutdown.** No exit step and no other mode in OpenRGB: the light is set black.

**Exit modes** (Lightloom extension, [save-shutdown.md](../host-api/save-shutdown.md)): `keep`
sends the last frame once more with the four commands of a frame, `off` the same black; `restore`
(and SignalRGB's call without a mode) and suspend are the shutdown above. Nothing is stored.
OpenRGB sends no keepalive, so the device is expected to hold the colours until it loses power or
other software takes it over (unverified on hardware).

## Left out

Nothing lighting-related. No audio setting is touched (OpenRGB sends none).

## Untested

The product id. What commands 1, 2 and 4 do is not explained in OpenRGB; check on hardware that
they touch no audio setting before marking the product tested.
