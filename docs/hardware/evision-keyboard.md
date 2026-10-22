<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# EVision keyboards (Sonix-based): V2 direct mode; the older V1 keyboards are left out

The plugin is `plugins/evision-keyboard/`, adapted from OpenRGB's EVision controllers:

| File | What it is |
|------|------------|
| `plugin.js` | EVision Keyboard `320F:5041` (BYGG CSB ICL01), EVision V2 layout |
| `endorfy-omnis.js` | Endorfy Omnis `3299:0012`, Endorfy layout; its own module because a plugin module has one `VendorId()` |
| `evision-v2.js` | the V2 protocol both share (helper module) |
| `layouts.js` | both key layouts as data, generated from OpenRGB's tables (helper module) |

Nothing here has been run on a keyboard: both products are untested. OpenRGB's EVision family
has two protocols. HueDeck drives the newer one ("V2"), whose firmware has a real direct mode.
The older one ("V1", 13 product ids, among them the reference machine's `320F:5064`) is **not
driven**: its only per-key path rewrites the keyboard's stored custom colours on every update
(see "Why the V1 keyboards are not driven").

## Sources

OpenRGB at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file's header was checked; all are
`SPDX-License-Identifier: GPL-2.0-or-later`. Paths are under `Controllers/EVisionKeyboardController/`
unless they start at the repository root.

| # | File | Authors (header) | Used for |
|---|------|------------------|----------|
| S1 | `EVisionV2KeyboardController/EVisionV2KeyboardControllerDetect.cpp` | (no author line) | Products, names, interface, usage page, layout per product |
| S2 | `EVisionV2KeyboardController/EVisionV2KeyboardController.h` | Le Philousophe, 25 Dec 2022 | Report id, packet size, commands, matrix size |
| S3 | `EVisionV2KeyboardController/EVisionV2KeyboardController.cpp` | Le Philousophe, 25 Dec 2022 | `Query`/`Read`/`Write`, capabilities, key-to-buffer maps, `SetLedsDirect`, `RefreshLedDirect`, `EndLedsDirect` |
| S4 | `EVisionV2KeyboardController/RGBController_EVisionV2Keyboard.cpp`, `.h` | Le Philousophe, 25 Dec 2022 | Direct mode, matrix maps, key lists, keepalive thread, logo/edge zones |
| S5 | `EVisionKeyboardController/EVisionKeyboardControllerDetect.cpp`, `EVisionKeyboardController.cpp`, `.h`; `EVisionV2KeyboardController/RGBController_EVisionKeyboard.cpp`, `.h` (the V1 RGBController sits in the V2 directory in this checkout) | Adam Honse (CalcProgrammer1), 15 and 25 Mar 2020 | The V1 protocol, read to decide it cannot be driven; no V1 code is in the plugin |
| S6 | `RGBController/RGBControllerInterface.h`, `RGBController/RGBControllerKeyNames.cpp` (root) | Adam Honse; Chris M (Dr_No) | Meaning of `MODE_FLAG_AUTOMATIC_SAVE`; which key each `KEY_EN_*` constant is |
| S7 | `DetectionManager.h`, `DetectionManager.cpp` (root) | Adam Honse | `REGISTER_HID_DETECTOR_IP` matches vendor, product, interface and usage page, any usage |
| S8 | `scripts/build-supported-devices-md.sh` (root) | | Legend of the `@save` / `@direct` doc tags |
| S9 | `docs/host-api/hid-usb.md` (HueDeck) | | Key names HueDeck plugins use; the reference machine's `320F:5064` exposes usage page `0xFF1C`, usage `0x0092` at `MI_01&Col04` |

`plugin.js`, `endorfy-omnis.js`, `evision-v2.js` and `layouts.js` carry the attribution header
for S1 to S4.

## Products

Driven (S1; both `REGISTER_HID_DETECTOR_IP(..., 1, 0xFF1C)`):

| OpenRGB name | VID:PID | Interface | Usage page | Layout | Module | Tested |
|--------------|---------|-----------|------------|--------|--------|--------|
| EVision Keyboard 320F:5041 (`BYGG_CSB_ICL01_PID`) | `320F:5041` | 1 | `0xFF1C` | EVision V2, 106 keys | `plugin.js` | no |
| Endorfy Omnis (`SPCGEAR_VID`, `ENDORFY_OMNIS_PID`) | `3299:0012` | 1 | `0xFF1C` | Endorfy, 104 keys | `endorfy-omnis.js` | no |

Not driven (S5; all `REGISTER_HID_DETECTOR_IP(..., 1, 0xFF1C)`, names "EVision Keyboard
VVVV:PPPP"): `0C45:5004`, `0C45:5104`, `0C45:5204`, `0C45:652F`, `0C45:7672`, `0C45:7698`,
`0C45:8520`, `320F:5000`, `320F:502A`, `320F:505B`, `320F:5064`, `320F:5078`, `320F:5084`.

## Why the V1 keyboards are not driven

HueDeck never writes a device's stored profiles or colours, and does not port a per-key mode
that needs such a write on every update. OpenRGB's V1 controller (S5) has exactly that, and
nothing else, for per-key colour:

1. Its doc block says `@direct :x:` (no direct mode: "not applicable for this device", S8) and
   `@save :robot:` (saving is automatic and cannot be turned off).
2. Its only per-key mode is "Custom" (`EVISION_KB_MODE_CUSTOM`, `0x14`), flagged
   `MODE_FLAG_AUTOMATIC_SAVE`, "Mode automatically saves" (S6).
3. `DeviceUpdateLEDs()` calls `SetKeyboardColors()`, which sends the whole 378-byte table
   (126 keys x R G B) as 7 requests of 54 bytes at offsets 0, 54, ..., 324 with command `0x11`,
   `EVISION_KB_COMMAND_WRITE_CUSTOM_COLOR_DATA` ("Write custom color data"); command `0x10`
   reads the same table back. Every colour update rewrites the keyboard's stored custom table.
4. The V1 command set has no other colour command. Mode changes (`SendKeyboardModeEx()`,
   command `0x06` set parameter, 8 bytes at parameter 0: mode, brightness, speed, direction,
   random flag, R, G, B) write the stored mode settings, also saved automatically. The begin
   (`0x01`) and end (`0x02`) packets are defined but OpenRGB never sends them.
5. The V2 firmware of the same family keeps the two apart: stored custom colour sets (`0x0B`)
   are written only on an explicit save (`MODE_FLAG_MANUAL_SAVE`, `DeviceSaveMode()`, between
   begin/end configure), while live colours go through `0x12`, "send dynamic colours", the
   "software controlled mode". V1 offers only the stored path.

So none of the V1 protocol is in the plugin. For whoever later learns a volatile V1 command from
a recording: V1 requests are 64-byte output reports `04 <sum lo> <sum hi> <cmd> <size>
<offset lo> <offset hi> 00 <data>`, the sum over bytes 3..63. OpenRGB keeps that buffer as
`char`, so on x86 builds it adds bytes of `0x80` and above as negative numbers: the low checksum
byte equals the plain sum's, the high byte is lower by the count of such bytes.

To light a V1 keyboard such as `320F:5064` without touching its memory, one would first have to
find out whether its firmware accepts live colours at all, for example by recording what a
program that animates it per key sends (`plugins/README.md`, "Writing a new family").

## Transport (V2)

The vendor collection: USB interface 1, usage page `0xFF1C`. OpenRGB's detector takes any usage
there (S7), so `Validate()` is `interface === 1 && usage_page === 0xFF1C`. (On the V1 keyboard
`320F:5064` of the reference machine that collection is `MI_01&Col04`, usage `0x0092` (S9); the
V2 keyboards have not been seen.) Every request is a 64-byte output report with report id 4
(the id included in the 64), and the keyboard answers each one with a 64-byte input report.

## Request and answer (`Query`, S3)

| Byte | Request | Answer (as OpenRGB checks it) |
|------|---------|-------------------------------|
| 0 | `04` report id | `04` |
| 1, 2 | checksum: sum of bytes 3..63, 16 bits, low byte first | the request's checksum bytes, unchanged |
| 3 | command | the same command |
| 4 | data size, at most 56 | size answered, at most 56 |
| 5, 6 | offset, low byte first | the same offset |
| 7 | `00` | status: `00` accepted, anything else refused |
| 8..63 | data | data (reads) |

The answer must be 64 bytes long; input reports with another report id are skipped while
waiting. Longer transfers are split into requests of 56 bytes: `Read()` advances by the size
the keyboard answered, `Write()` by the size sent and stops at the first request that is
refused or not answered.

Commands (S2):

| Command | Name | HueDeck |
|---------|------|---------|
| `01` | begin configure | not used |
| `02` | end configure | not used |
| `03` | read capabilities | `Initialize()`, read only |
| `05` | read config | not used (OpenRGB reads the stored mode only to show it) |
| `06` | write config | **never** (stored profiles and modes) |
| `0A` | read custom colours | not used |
| `0B` | write custom colours | **never** (stored colour sets) |
| `12` | send dynamic colours | every changed frame, and the keepalive |
| `13` | end dynamic colours | `Shutdown()` |

## What the plugin sends

| When | Requests (hex, zeros to 64 bytes elided) |
|------|-------------------------------------------|
| `Initialize()` | `04 0A 00 03 07 00 00 00`: read 7 capability bytes from offset 0. The answer's data: bytes 0, 1 magic `AA 55`, byte 5 the key-map size (slots in the colour buffer), byte 6 the macro memory in units of 128 bytes; the rest is unused. Like OpenRGB, the plugin gives up only when both magic bytes differ. Switching to direct mode needs no request ("No mode set packets required for Direct mode", S4). |
| a changed frame | the colour buffer, 3 x key-map size bytes, zero except each key's R G B at 3 x its slot, sent as `04 <sum> 12 <size> <offset> 00 <data>` at offsets 0, 56, 112, ...: with 126 slots, 6 requests of 56 bytes and one of 42 |
| no change for more than 200 ms | `04 25 00 12 01 12 00 00 00`: one byte at offset 18, slot 6 (OpenRGB's keepalive thread, `RefreshLedDirect()`); see "Keepalive" |
| `Shutdown()` | `04 13 00 13 00 00 00 00`: end dynamic colours; the keyboard goes back to its own stored lighting |

Every request waits for its answer. `Shutdown(true)` (system suspend) sends the same end
request: colours sent instead would not last without the refreshes, so while the computer is
suspended the keyboard shows its own stored effect (not black). The end request is sent even
when the capabilities read failed, as OpenRGB's keepalive thread does on exit; it is volatile
and stores nothing.

### Keepalive

OpenRGB refreshes the direct colours whenever 200 ms pass without an update (a thread checks
every 100 ms), which suggests the firmware leaves direct mode by itself after a short idle time;
the timeout is not in the source. HueDeck sends unchanged frames only as that refresh. The
refresh writes one byte at slot 6, the blank cell between Esc and F1 of the EVision V2 layout
(`BLANK_SPACE`). On the Endorfy layout slot 6 is F5 (its slots are row-major), so OpenRGB's zero
byte turns F5's red off until the colours change. HueDeck sends the byte the keyboard already
has at that offset instead: on the EVision V2 layout that is OpenRGB's zero, the same packet;
on the Endorfy it keeps F5 lit.

## Key layouts

`Size()` is 21 x 6, OpenRGB's matrix (`EVISION_V2_MATRIX_WIDTH` x `_HEIGHT`). The keys come in
OpenRGB's LED order (`led_evisionv2`, `led_endorfy`), each at its cell of `evisionv2_matrix` /
`endorfy_matrix`, so they sit where they are on the keyboard. Slots come from `evisionv2_map` /
`endorfy_map`: EVision V2 slots are column-major (slot = 6 x column + row), Endorfy slots
row-major with 21 per row (Right Shift and Num 0 one cell right of where OpenRGB draws them).
The EVision V2 table has cells for both ANSI and ISO keys (the backslash above Enter, `#`, the
ISO Enter and the key left of Z); the Endorfy table is ANSI with a Menu key.

Names: OpenRGB's `KEY_EN_*` constants in the key names HueDeck plugins use (S9), for example
`KEY_EN_ESCAPE` Esc, `KEY_EN_NUMPAD_LOCK` NumLock, `KEY_EN_NUMPAD_7` Num 7, `KEY_EN_POUND`
ISO_#, `KEY_EN_ISO_BACK_SLASH` ISO_<, `KEY_EN_ISO_ENTER` and `KEY_EN_ANSI_ENTER` Enter,
`KEY_EN_RIGHT_FUNCTION` Fn, `KEY_EN_LEFT_CONTROL` Left Ctrl. `layouts.js` lists every key with
its OpenRGB constant, cell and slot; it was generated from the C++ tables by a script that also
checked that each matrix places every key exactly once and that no slot is used twice.

## Where HueDeck differs from OpenRGB

- Waits at most 100 ms for an answer (OpenRGB's read blocks), skips at most 8 input reports with
  another id, and flushes stale input reports before each request (host side; nothing is sent).
- A capabilities read that fails or comes back short leaves the plugin without colours (it logs
  why). OpenRGB carries on with an uninitialised key-map size.
- Keys whose slot is not below the key-map size stay dark; OpenRGB would write past its buffer.
- Unchanged frames are not resent (see "Keepalive"); a frame that was not confirmed in full is
  resent whole on the next `Render()`. After 3 unconfirmed frames in a row the plugin tries a
  new frame only once a second (and logs it once), so a keyboard that stopped answering does not
  hold every `Render()` for the 100 ms read timeout; OpenRGB retries on every update.
- The keepalive byte on the Endorfy layout (see "Keepalive").
- At start OpenRGB also reads the stored mode, its colour and, in custom mode, the stored colour
  set (commands `05` and `0A`) to fill its mode list. HueDeck has no use for them and skips
  them; they change nothing on the keyboard.
- OpenRGB can update a single key (`SetLedDirect()`); HueDeck always sends whole frames.

## Left out (V2)

- The keyboard's hardware effects and its ten custom colour sets: OpenRGB writes them into the
  stored profile (`SetMode()`: write config between begin and end configure; `SetLedsCustom()`:
  write custom colours). Not ported.
- The logo and edge zones (`EVISION_V2_KEYBOARD_PART_LOGO`, `_EDGE`, `ENDORFY_KEYBOARD_PART_EDGE`):
  OpenRGB offers only hardware modes for them, written into the stored profile; there is no
  direct path. Not driven.
- Nothing that is not lighting (key maps, macros, polling rate) is touched.

## Tests and what is untested

`tests/packets.test.mjs` and `tests/layout.test.mjs` (node:test, with the keyboard double in
`tests/keyboard-double.mjs`) check, against `fixtures/packets.json`, the capabilities request,
frames for two colour patterns and two key-map sizes on both layouts, the keepalive, the end
request, and how answers OpenRGB would refuse are treated; plus metadata, geometry and
`Validate()`. The fixture was worked out from the C++ code by an independent re-implementation
of `Query`, `Write`, `SetLedsDirect`, `RefreshLedDirect` and `EndLedsDirect` (its `$comment`
says how). The answers in it are synthetic: the firmware's echo as `Query()` accepts it, and
made-up capability bytes. The real key-map size is unknown: both layouts use slots up to 124,
the fixtures assume 126 (21 x 6) and 128, and the plugin uses whatever the keyboard reports.

Untested on hardware: everything, including whether 7 request/answer pairs per frame keep up
with 30 frames a second, the firmware's direct-mode timeout, and the capability bytes. No
HueDeck recording of an EVision keyboard exists.
