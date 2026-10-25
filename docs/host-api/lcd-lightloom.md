<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# LCD faces in Lightloom

This page describes how Lightloom implements SignalRGB's LCD model (`effects-engine.md` §4, and the
Gap resolutions in `hid-usb.md` and `components-channels.md`). It covers the `@SignalRGB/lcd`
module, the face renderer and the face format.

## Pieces

| Piece | Where |
|---|---|
| Frame encoders, crops and slices, `LCD.initialize` validation, the provider trait and its mock | `crates/lightloom-lcd` |
| `@SignalRGB/lcd` in device sessions (the `lcd*` natives) | `crates/lightloom-plugin-host/src/lcd.rs`, `js/host.js` (`lcdModule`) |
| One face renderer per LCD device, face choice, sensors | `crates/lightloom-daemon/src/lcd.rs` |
| The face browser | `lightloom-renderer --face --width W --height H` (`crates/lightloom-renderer`) |
| Face selection API | `PUT /v1/devices/{key}/lcd`, `GET /v1/lcd/faces`, `DeviceView.lcd` (`docs/api/openapi.yaml`) |
| Preview API | `GET /v1/devices/{key}/lcd/preview` (PNG of the latest face frame) |
| Dashboard | `ui/src/components/LcdFacePicker.svelte` (on a device card while its LCD runs: face, controls, frame rate), `LcdPreview.svelte` |
| Starter faces (CC0) | `faces/clock.html`, `faces/sensor.html` |

## Flow

1. The plugin calls `LCD.initialize({width, height, circular})`. The session validates the
   settings and attaches to the daemon's provider with the device key and the size.
2. The daemon starts a face renderer for that device. This is its own
   `lightloom-renderer --serve --face` process: a second CEF browser next to the canvas one,
   with its own shm ring, at exactly `width` x `height` and at the device's own frame rate
   (`lcdFramerate`, default 30, 1-60). Its frames land in a per-device frame store. The
   supervision is the canvas renderer's (`lightloom_render::CefRenderer`): restart with backoff,
   hold the last frame, republish while the page paints nothing.
3. Every `LCD.getFrame({format, quality})` encodes the latest face frame in Rust and hands the
   bytes to the plugin as a plain array.
4. A second `initialize` with another size re-attaches. The end of the session (stop, unplug,
   sleep, daemon exit) detaches, and the face renderer stops.

A face renderer process per device keeps one LCD's page crash away from the canvas and from the
other LCDs. It also reuses the canvas renderer's tested supervision instead of adding a
multi-browser host.

**Mock path.** When `lightloom-renderer` is missing, or no face is installed, a rainbow test
pattern of the right size stands in (`renderer: "mock"` in the device view). Tests use it, and
`lightloom_lcd::MockLcdProvider` (a fixed bordered ramp) needs no daemon at all. A session
without any provider (CLI runs) shows black frames of the right size.

## Module API (device sessions)

`LCD.initialize(settings)`:
- `settings` must be an object, `width`/`height` numbers, and `circular` (or `circularPreview`) a
  boolean. These are SignalRGB's messages.
- Each side must be whole, 1-2048, and the area at most 2 Mpx ("Invalid dimensions").
- The same settings again keep the running face.
- Frames stay rectangular. A round screen only sets `circular` in the view. The starter faces
  draw inside the inscribed circle.

`LCD.initialized` is a read-only boolean.

`LCD.getFrame({format, quality?})`: formats are case-insensitive, and `JPG` is accepted:

| format | bytes |
|---|---|
| `JPEG` | JPEG file, `quality` 1-100 (default 75) |
| `PNG` | PNG file, 8-bit RGB |
| `BMP` | BMP file: 54-byte header, 24-bit BGR, bottom-up, rows padded to 4 bytes |
| `RGBA` | R G B A per pixel (A = 255), top-down |
| `RGB` | R G B per pixel, top-down, each row padded to a multiple of 4 bytes (documented by the corpus) |
| `RGB565` | little-endian `u16` per pixel, `r5<<11 \| g6<<5 \| b5`, top-down, rows padded to 4 bytes |
| `NZXT::RGB565`, `NZXT::Q565` | **not supported**: `LcdError` `UNSUPPORTED_FORMAT` |

The raw layouts follow Qt `QImage` memory (32-bit aligned scanlines): `Format_RGB888`,
`Format_RGBA8888` and `Format_RGB16`, native-endian, which is little-endian on every platform
SignalRGB ships on. The corpus confirms the `RGB` padding. For `RGB565` the padding is inferred
from the same layout, and it only changes odd widths. The two NZXT formats appear in the
validator only. No public document gives their byte order or quantisation, so Lightloom returns
a typed error rather than bytes in a guessed layout. Adding them needs a capture of NZXT's
software talking to a Kraken Elite/Z3 LCD.

Frames are CEF paints composited over black, so the output is always opaque.

`LCD.getSlicedFrame({columns | cols, rows, format, quality?})` returns an array of encoded cells,
row by row:
- A cell is `width / columns` x `height / rows`, using integer division, and must be at least
  1 px.
- Leftover edge pixels belong to no cell.
- The errors are SignalRGB's: "Invalid grid dimensions ..." and "... results in invalid cell
  size".

`LCD.getImageBuffer(x, y, w, h, {outputWidth?, outputHeight?, format?})` crops a region and
scales it (bilinear):
- The region must start inside the canvas and is clipped to it.
- The output size defaults to the region's size and must be 1-1024 per side.
- `format` defaults to `RGB`.

Neither of these two members has a corpus call site. Their behaviour is read from SignalRGB's
error strings.

**Errors.** Every failure throws an `LcdError` (`name === "LcdError"`). Its `code` is one of
`NOT_INITIALIZED`, `INVALID_SETTINGS`, `INVALID_FORMAT`, `UNSUPPORTED_FORMAT`, `INVALID_GRID`,
`OUT_OF_BOUNDS` or `ENCODE_FAILED`. Encoding counts as host work: it suspends the plugin's
JavaScript budget, as blocking I/O does. The session report has `lcd` counters (size, frames,
bytes, last format, errors).

The dry-run (`lightloom compat dry-run`) keeps the recording stub, whose members answer `[]`.

## Faces

A face is an effect-format HTML page (`<title>`, `<meta description>`, `<meta publisher>`,
`<meta property ...>` controls) with an `onReady()` entry:
- The renderer sizes the view to the LCD (`window.innerWidth`/`innerHeight`).
- It injects `window.engine` (with `getSensorValue`) and the control globals with their defaults.
- After load it assigns the controls again, calls each `on<prop>Changed()` once, then
  `onReady()` (instead of a lightscript's `onEngineReady()`).

The face library is built like the effect library:
- `lightloom:<stem>` for the files in `faces/` shipped next to the daemon (or the source tree
  in development builds).
- `user:<stem>` for `<config>/faces`.

Faces that call `getSensorValue` get the sensor poller's values (`docs/sensors.md`) while they
show.

**Choosing a face.** The face and frame rate are chosen per device (`PUT /v1/devices/{key}/lcd`
with `{face, framerate}`, where `face: ""` means the default). The choice is kept in the
database (`lcd.faces`) and restarts a running face. The default face is `lightloom:clock`.

**Face controls.** A face's `<meta property>` controls work like an effect's: `GET /v1/lcd/faces`
lists them (`controls`, the effect-meta shape), and `PUT /v1/devices/{key}/lcd` with
`{values: {property: value}}` sets them for the face the device shows (after the request's `face`).
Values are checked against the control's type (an unknown control or a value that does not fit is
a 400) and kept per device and per face in the same `lcd.faces` record (`values: {faceId: {...}}`),
so switching faces and back finds them again. A new value reaches the running page at once (the
control global, then `on<prop>Changed()`), without a restart; a face that starts gets its stored
values instead of the defaults. `DeviceView.lcd.values` holds the values of the face shown. The
dashboard shows the controls under the face picker.

**Preview.** `GET /v1/devices/{key}/lcd/preview` encodes the device's latest face frame as a PNG at
the LCD's size (404 while no LCD runs; `Cache-Control: no-store`). The device card shows it as a
small picture (at most 96 px, round for a round screen), fetched about twice a second, one request
at a time, and only while the picture is on screen and the browser tab is visible.

**Not implemented yet:**
- SignalRGB's background layer (`media` / `thumbnail`, `<meta background>` compositing);
- rotation;
- brightness;
- idle blanking and the RAF re-kick shim;
- remote AIO themes.

## Hardware status

No LCD device has been driven yet. The reference PC's NZXT Kraken X3 has ring LEDs, not a
screen. Everything above is tested with mocks:
- `crates/lightloom-lcd` unit tests;
- `crates/lightloom-plugin-host/tests/session_lcd.rs`;
- `crates/lightloom-daemon/tests/lcd.rs`;
- the fake renderer in `crates/lightloom-render/tests/cef_client.rs`.

The two starter faces were rendered by the real CEF renderer at 240x240 (`--face`, PNG
snapshots).
