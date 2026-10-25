<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Custom LEDs: colouring single LEDs

A device, or one of its subdevices or channel components, can show colours you choose
LED by LED (keys of a keyboard, for example) instead of the effect, as OpenRGB allows.

## In the dashboard

Devices → **Customise LEDs** on a connected device opens the LED painter:

- The LEDs are drawn at their cells (`LedPositions()` on the `Size()` grid; subdevices'
  `setSubdeviceLeds`, components' `LedCoordinates`). Hovering an LED shows its name.
  Keyboards (`DeviceType()` `keyboard`, or LEDs named like keys) are drawn as labelled key
  caps; the plugin's positions are used as they are, no layout is built in.
- **Paint**: click or drag with the colour picker's colour. **Select**: drag a box
  (Shift adds, Ctrl removes). **Fill all** / **Fill selection**, **Clear** (black), **Undo**
  (also Ctrl+Z).
- **Follow effect / Custom LEDs** switches the part shown in the selector. Painting
  switches it to Custom LEDs; going back to the effect keeps the colours for next time.
  The device card shows a "Custom LEDs" badge while a part is in custom mode.
- **Zones**: select LEDs, give them a name, *Save selection as zone*. A zone can be
  selected again or painted at once (*Colour*).

Painting is local first and the dashboard sends the changes in batches (at most one request
every 40 ms, one at a time), so a drag changes the device within a frame or two.

## How it works

- The colours are kept per endpoint uid (the device key, `<device key>:<subdevice id>`, or a
  component's id) and per **slot**: the LED index for devices and components, the grid cell
  (`y * width + x`) for subdevices.
- An endpoint in custom mode puts its colours in the static layer of the daemon's
  `DirectLeds` (`crates/lightloom-engine/src/direct.rs`). Every session reads it once per
  frame, so the plugin's `device.color(x, y)`, channel and subdevice colours answer from the
  per-LED map instead of the canvas. Plugins do not change. The brightness product
  (global × layout × device) still applies; the low-battery pulse still wins.
- An OpenRGB SDK client writing the same endpoint wins while it is connected; the custom
  colours come back when it goes away.
- Persistence: table `custom_leds` (`crates/lightloom-daemon/src/store/leds.rs`), one row per
  endpoint with the mode, the colours (`#rrggbb`) and the zones. Edits are written 750 ms
  after the last edit of a burst, before an export, and on shutdown. They are part of the
  setup export (`customLeds`).
- Editing needs the device running (the slot count comes from its session); the stored
  colours of a device that is away come back with it.

## API

| Route | What |
|---|---|
| `GET /v1/devices/{key}/leds` | every endpoint's LEDs (`slot`, `x`, `y`, `name`), `custom`, `colors`, `zones` |
| `PATCH /v1/devices/{key}/leds/{uid}` | `{colors?, fill?, set?: {slot: colour}, zones?: {name: colour}, mode?: "custom" \| "effect"}`, applied in that order |
| `PUT /v1/devices/{key}/leds/{uid}/zones/{name}` | `{leds: [slots]}`: save a zone |
| `DELETE /v1/devices/{key}/leds/{uid}/zones/{name}` | delete a zone |

See `docs/api/openapi.yaml`. A device that is not running answers 409; a wrong slot, colour
or mode 400; an unknown endpoint or zone 404.

## Not done

- No lasso selection (box only), no per-key effects or hardware modes.
- Endpoints with more than 6000 LEDs are not drawn one by one (fill, clear and zones work).
- Not tried on a real keyboard yet.
