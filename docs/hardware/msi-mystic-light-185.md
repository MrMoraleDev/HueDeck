<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# MSI Mystic Light "185-byte" motherboard controller (USB HID 1462:7C91)

Hardware notes for the first-party plugin `plugins/msi-mystic-light-7c91/plugin.js` (zone mode in
M1, v0.1; per-LED mode since v0.2, see [Per-LED mode](#per-led-mode-what-the-plugin-does-since-v02-2026-10-18)).
This page records **facts and their provenance**; everything in the "Unverified" column must be
confirmed on the bench before the plugin relies on it. No code was copied from any source listed
below; the tables are our own re-expression of publicly documented behaviour.

## Sources

| # | Source | Used for | Licence note |
|---|--------|----------|--------------|
| S1 | OpenRGB community wiki, page "MSI Motherboards Mystic Light Knowledge Collection" (public wiki of the OpenRGB fork by Thomas Boos, `gitlab.com/thombo/OpenRGB/-/wikis/`; the same knowledge collection is referenced by upstream OpenRGB merge requests) | 185-byte layout, zone record bit fields, effect numbers, direct-mode procedure, board list | Documentation only. OpenRGB **code** is GPL-2 and was not read. |
| S2 | The user's own SignalRGB 2.5.80 plugin cache, `Plugins\Msi\185 Byte Motherboard Controller\MSI_185Byte_Controller.js` | API-usage shape (which `device.*` calls, lengths), zone offsets, per-board configuration table for 7C91, EZ-LED BIOS probe, per-LED mode switching values | Read to learn facts only; nothing re-used verbatim. |
| S3 | SignalRGB log files on this machine (`SignalRGB_2026101x_*.log`) | Behaviour observed on the actual board: USB path, product string, "PerLED" detection result, which configuration table applied | Observation. |
| S4 | Read-only HID probe on this machine (`CM_Get_Device_Interface_List` + `HidP_GetCaps`, device opened with zero access, no I/O) | Interface/usage page/report lengths | Observation. |
| S5 | USB-IF HID 1.11 specification | Feature report semantics (GET/SET_REPORT, report id as first byte) | Public standard. |
| S6 | Lightloom's own recordings of this board's traffic in per-LED mode, decoded with the coordinate probe effect (2026-10-18) | Start sequence, per-LED configuration packet, colour stream layout, EZ LED reply, read-back of the configuration | Observation. |

## Device identity (observed, S3/S4)

| Item | Value | Source |
|------|-------|--------|
| USB VID:PID | `0x1462:0x7C91` | S3, S4 |
| USB product string | `MYSTIC LIGHT ` (trailing space) | S3 |
| USB parent instance | `USB\VID_1462&PID_7C91\<serial>` (12-hex-digit serial) | S3 |
| HID child instance | `HID\VID_1462&PID_7C91\7&...&0&0000` — **no `&MI_xx`**, **no `&ColNN`**: one interface, one top-level collection | S4 |
| HID usage page / usage | `0x0001` (Generic Desktop) / `0x0000` (undefined usage). **Not** a vendor-defined page. | S4 |
| Report lengths (incl. report id) | input 64, output 64, **feature max 725** | S4 |
| Link collections inside the TLC | 2 | S4 |
| Board | MSI MAG B550 TOMAHAWK (MS-7C91); same PID is used by MPG B550 GAMING EDGE WIFI | S2, S3 |
| S1 board-list row | `7C91 — JRGB1, JRGB2, JRAINBOW1, JRAINBOW2, 6 onboard LEDs — per-LED — verified` | S1 |
| SignalRGB `Validate` | accepts `endpoint.interface === 0 \|\| === 2`; usage page ignored | S2 |
| SignalRGB runtime result on this board | "Motherboard is PerLED", "Using Configuration Override" (table entry `MAG B550 TOMAHAWK (MS-7C91)`: 6 onboard LEDs, 2 RGB headers, 2 ARGB headers, 0 JPIPE, 0 Corsair) | S3 |

Lightloom consequence: `Validate` selects on the interface number only. This non-composite USB device
reports interface `0` (hidapi, SignalRGB and Lightloom agree; see `docs/host-api/hid-usb.md`,
"Endpoint tuple semantics"); `-1` is accepted too, for transports without a USB interface.
Selecting on a vendor usage page would reject this device.

## Report inventory

| Report id | Direction / type | Length (incl. id) | Purpose | Unverified |
|-----------|------------------|-------------------|---------|------------|
| `0x52` | Feature GET/SET | **185** | Per-zone built-in effect configuration + "save" byte. GET returns the controller's current table. | GET length on *this* board (SignalRGB probes it with a 200-byte buffer and expects 185; OpenRGB does the same per S1-linked discussion). Our `Initialize()` aborts all writes unless 185 bytes come back. |
| `0x53` | Feature SET | 725 | Per-LED colour stream (ARGB headers and, when switched, onboard LEDs). Not used in M1. | — |
| `0x01` | Output report (write) + Input read | 64 | Command channel: `0xBA` = query "EZ LED" BIOS switch (reply byte 6 == 1 means enabled); `0x80/0x82/0x84` = ARGB Gen2 query/detect/enable per header | Reply byte positions; not exercised by Lightloom yet. |
| `0x80`..`0x82` | Feature GET | 242 | ARGB Gen2 strip info per header (LED count at `15 + 16*n`) | Everything; M2+. |

## Feature report 0x52 — 185-byte layout (S1 and S2 agree on every offset)

Offsets are into the full buffer, index 0 = report id. Record size is 10 bytes unless noted.

| Offset | Size | Zone (S1 name) | 7C91 wiring (S1/S2) | Lightloom M1 | Unverified |
|--------|------|----------------|---------------------|------------|------------|
| `0x00` | 1 | Report id `0x52` | — | written as read | — |
| `0x01` | 10 | J_RGB_1 (12 V header) | present | preserved | — |
| `0x0B` | 10 | J_PIPE_1 | absent on 7C91 | preserved | — |
| `0x15` | 10 | J_PIPE_2 | absent | preserved | — |
| `0x1F` | **11** | J_RAINBOW_1 (5 V ARGB) — byte +10 = LED count | present | preserved (channel in M2) | — |
| `0x2A` | **11** | J_RAINBOW_2 — byte +10 = LED count | present | preserved (channel in M2) | — |
| `0x35` | **11** | J_CORSAIR, or J_RAINBOW_3 on 3-header boards (then byte +9 must be `0x04`) | absent | preserved | — |
| `0x40` | 10 | J_CORSAIR outer ring (LL120) | absent | preserved | — |
| `0x4A` | 10 | J_ONBOARD_LED_0 — also the "master" zone for multi-zone effects | onboard LED 1 | **static + colour** | Whether the 6 physical LEDs are slots 0..5 (`0x4A..0x7C`, SignalRGB table) or 1..6 (`0x54..0x86`, as S1's direct-mode text "LED_1 to LED_9" could suggest). Resolve with a per-LED test pattern on the bench. |
| `0x54` | 10 | J_ONBOARD_LED_1 | onboard LED 2 | static + colour | see above |
| `0x5E` | 10 | J_ONBOARD_LED_2 | onboard LED 3 | static + colour | see above |
| `0x68` | 10 | J_ONBOARD_LED_3 | onboard LED 4 | static + colour | see above |
| `0x72` | 10 | J_ONBOARD_LED_4 | onboard LED 5 | static + colour | see above |
| `0x7C` | 10 | J_ONBOARD_LED_5 | onboard LED 6 | static + colour | see above |
| `0x86` | 10 | J_ONBOARD_LED_6 | unused | preserved | — |
| `0x90` | 10 | J_ONBOARD_LED_7 | unused | preserved | — |
| `0x9A` | 10 | J_ONBOARD_LED_8 | unused | preserved | — |
| `0xA4` | 10 | J_ONBOARD_LED_9 | unused | preserved | — |
| `0xAE` | 10 | J_RGB_2 (12 V header) | present | preserved | — |
| `0xB8` | 1 | Save flag: `0` = apply only, non-zero = **persist to flash** | — | **always written as 0** | Exact non-zero value MSI Center uses; irrelevant because we never set it. |

Sum: 1 + 10×3 + 11×3 + 10 + 10×10 + 10 + 1 = 185.

### Zone record (10 bytes; S1 bit fields, S2 constant values)

| Rel. offset | Field | Meaning | Value Lightloom writes for a driven onboard zone | Unverified |
|-------------|-------|---------|-----------------------------------------------|------------|
| +0 | effect | built-in effect number (table below) | `0x01` static | — |
| +1..+3 | colour 1 | R, G, B | sampled colour | — |
| +4 | options | bit 0-1 speed, bit 2-6 brightness, bit 7 "sync with JRGB" | `0x28` = speed 0, brightness 10, no sync (the value S2 uses for every onboard record, in both zone and per-LED configuration packets) | Brightness scale (is 10 the maximum MSI Center uses? S2 uses 0x28 everywhere, S1 does not state a range). Keeping the board's own value was considered but a restore image left in per-LED mode may carry 0x00 here. |
| +5..+7 | colour 2 | R, G, B; second LED on boards wiring two LEDs to one zone; second colour for two-colour effects | same as colour 1 | — |
| +8 | sync / colour flag | bit 0 onboard, 1 rainbow1, 2 rainbow2, 3 corsair, 4 pipe1, 5 pipe2 sync flags; bit 7 "colour flag" (zone shows its own colour bytes; cleared = auto colour cycling) | `0x80` (S1: single-zone effects need sync clear + colour flag set; S2 uses 0x80 on onboard records 2..10) | — |
| +9 | padding | — | preserved from restore image | — |

JRAINBOW records add byte +10 = LED count of the strip. The Corsair record has a different
layout (+4 fan flags, +5 quantity, +10 "individual" flag) — preserved untouched.

### Effect numbers (S1 "tested effects"; S2's 43-entry list agrees on the ones shown)

| Value | Effect | | Value | Effect |
|-------|--------|-|-------|--------|
| `0x00` | Off/disable (S2 only) | | `0x14` | Clock |
| `0x01` | **Static** | | `0x15` | Color Pulse |
| `0x02` | Breathing | | `0x16` | Color Shift |
| `0x03` | Flashing | | `0x17` | Color Wave |
| `0x04` | Double Flashing | | `0x18` | Marquee |
| `0x05` | Lightning | | `0x1A` | Rainbow Wave |
| `0x07` | Meteor | | `0x1B` | Visor |
| `0x0F` | Color Ring | | `0x1D` | Rainbow Flashing |
| `0x10` | Planetary | | `0x23` | Color Ring Double Flashing |
| `0x11` | Double Meteor | | `0x24` | Stack |
| `0x12` | Energy | | `0x25` | **per-LED / direct stream mode** (S1: switching value; S2: same) |
| `0x13` | Blink | | `0x28` | Fire (newer boards) |

Static, Breathing, Flashing, Double Flashing and Lightning are *single-zone* effects (each zone
independent, sync flags clear, colour flag set). All other effects are *multi-zone*: the onboard
LEDs, JRGBs and pipes follow J_ONBOARD_LED_0 when the sync flags are set.

## Modes of operation

1. **Built-in effects** — one 0x52 packet programs every zone.
2. **Zone-based direct mode (used by Lightloom M1)** — set the onboard zones to static (`0x01`) and
   write the colour bytes; one 185-byte SET per frame. Headers (JRGB/JRAINBOW) can only be set as
   a whole zone in this mode.
3. **Per-LED mode (M2)** — a 185-byte "switching" packet puts effect `0x25` on J_ONBOARD_LED_0
   (synchronised variant: all sync flags set, JRAINBOW LED count 40, Corsair 120) or on each of
   J_ONBOARD_LED_0 / J_RAINBOW_1 / J_RAINBOW_2 / J_CORSAIR (unsynchronised variant, LED counts up to
   200/240). Colours then stream via feature report `0x53`: `[0x53, 0x25, area, index, 0x00, R,G,B ...]`
   with area `0x04` = JRAINBOW n, `0x05` = Corsair, `0x06` = onboard; up to 240 LEDs per message.
   The next 0x52 packet leaves per-LED mode. SignalRGB runs this board in per-LED mode (S3).

Consequence for the restore image: if Lightloom starts while the board was last configured by a
per-LED-mode application, the captured image carries effect `0x25` on the onboard master zone.
`Shutdown(false)` writes that image back verbatim (save flag forced to 0), which re-enters
per-LED mode with no colour stream — the LEDs may stay dark until the next reboot or until the
owning application re-streams. This is faithful "restore what we read" behaviour, but it should be
noted in the user documentation. **Unverified** until tried on the bench.

## What the plugin did in M1 (v0.1, zone mode; replaced by per-LED mode in v0.2)

1. `Initialize()`: `device.get_report([0x52], 185)`; require `getLastReadSize() === 185` (or array
   length 185 when the host lacks the call), an array of at least 185 entries, and byte 0 == `0x52`;
   keep a copy as the restore image. Anything else is logged to file and **disables every write**
   for the session. The host re-runs `Initialize()` after a system resume: when the fresh read is
   byte-identical to the last packet Lightloom itself wrote (the suspend colour), the restore image
   captured at first start is kept, so `Shutdown(false)` still restores the pre-run state; any other
   image (cold boot, another application) replaces it.
2. `Render()`: copy the restore image; for the six onboard slots `0x4A, 0x54, 0x5E, 0x68, 0x72, 0x7C`
   write `[0x01, R,G,B, 0x28, R,G,B, 0x80]` (9 bytes, padding byte preserved); force byte `0xB8` to
   `0`; `device.send_report(packet, 185)`. Colour comes from `device.color(x, 0)` for LED x, or from
   `forcedColor` when `LightingMode == "Forced"`.
3. `Shutdown(true)` (suspend): same packet with `shutdownColor` (default black).
   `Shutdown(false)` (exit): the restore image with byte `0xB8` forced to `0`.
4. Never touched: JRGB1/JRGB2, JRAINBOW1/JRAINBOW2 (including their LED-count byte), Corsair slots,
   onboard slots 7..10, any `0x53` / `0x01` / Gen2 traffic.

Fixtures of the current plugin: `plugins/msi-mystic-light-7c91/fixtures/per-led.json`
(test: `node --test "plugins/**/*.test.mjs"` from the repo root; Node >= 21 does not expand a bare
directory argument).

## Per-LED mode: what the plugin does since v0.2 (2026-10-18)

Source S6 (own recordings): Lightloom recorded the board's traffic while another plugin
drove it in per-LED mode, with the coordinate probe effect (`tools/device-probe/coords.html`,
every canvas pixel encodes its position) and two layouts: everything at its default place,
then the two 12 V header subdevices and the two JRAINBOW strips at distinct places
(`lightloom effect ... --signalrgb --layout ... --record`). Every slot of the colour stream
was matched to the canvas position it carried. The same recording of Lightloom's own plugin
afterwards decodes identically.

### Start sequence (as recorded; Lightloom reads 0x52 first)

| # | Report | Bytes | Meaning |
|---|--------|-------|---------|
| 0 | GET feature `0x52` | 185 | the restore image (Lightloom only) |
| 1 | SET feature `0x53` | `53` + 724 zero bytes | sent before every configuration; meaning unknown ("reset") |
| 2 | SET feature `0x52` | the configuration below | synchronised per-LED mode |
| 3 | output report `0x01` | `01 BA` + 62 × `CC` | EZ LED query |
| 4 | input report `0x01` | `01 5A CC CC CC CC <state> CC ...` | `<state>` 1 = EZ LED Control on in the BIOS, 0 = off (onboard LEDs dark) |

### The configuration packet (0x52)

Every record static black (`01 00 00 00 <options> 00 00 00 80 00`) except the onboard
master zone, which carries effect `0x25` with all sync flags set, so every other zone
follows its stream:

| Offset | Zone | Bytes |
|--------|------|-------|
| `0x01` | JRGB1 | `01 00 00 00 08 00 00 00 80 00` |
| `0x0B`, `0x15` | JPIPE1, JPIPE2 | `01 00 00 00 2A 00 00 00 80 00` |
| `0x1F`, `0x2A` | JRAINBOW1, JRAINBOW2 | `01 00 00 00 29 00 00 00 80 00 <LED count>` |
| `0x35` | JCORSAIR | `01 00 00 00 29 00 00 00 82 00 01` |
| `0x40` | JCORSAIR outer | `01 00 00 00 28 00 00 00 80 00` |
| `0x4A` | onboard master | `25 00 00 00 A9 00 00 00 BF 00` |
| `0x54` ... `0xA4` | onboard 1-9 | `01 00 00 00 28 00 00 00 80 00` |
| `0xAE` | JRGB2 | `01 00 00 00 2A 00 00 00 80 00` |
| `0xB8` | save flag | `00` |

The LED counts are the strips' lengths: 60 each on the wire (the "Default Strip - 60"
setup), 1 each before any strip length was known. Lightloom writes the channels' LED counts
(at least 1, at most 120 per header, 232 for both) and sends the reset and the
configuration again whenever a count changes.

Read-back of this packet (GET right after it, S6): identical except `0x3E` (+4, as in zone
mode) and the master zone's sync flags at `0x52`: written `0xBF`, read `0x87` (the flags
of headers this board does not have are dropped). The plugin ignores `0x52` when it checks
whether the board still holds its own configuration after a resume.

### The colour stream (0x53, one per frame)

`53 25 06 00 00`, then 240 LEDs of `R G B` (725 bytes). Effect `0x25`, area `0x06`
(onboard), start index 0. In synchronised mode the one stream carries every zone, in this
order:

| Slots | LEDs |
|-------|------|
| 0-5 | onboard LEDs 1-6 (left to right as the board's LED names go) |
| 6 | JRGB1 (12 V RGB header) |
| 7 | JRGB2 |
| 8 ... 8 + n1 - 1 | JRAINBOW1 strip, LED 0 first (n1 = its LED count in the configuration) |
| then n2 slots | JRAINBOW2 strip |
| the rest | black |

The other plugin sent a stream every frame and, on shutdown, one black stream; Lightloom
skips an unchanged stream for up to 1 s and, on exit, sends a black stream and then the
restore image, which leaves per-LED mode.

### Plugin layout

- Device: the six onboard LEDs, `Size` 6 × 1 (names unchanged since v0.1).
- Subdevices `12v RGB Header 1` and `12v RGB Header 2` (shown as "JRGB1/JRGB2 (12 V RGB
  header)"), one LED each.
- Channels `JRainbow 1` and `JRainbow 2`, 120 LEDs each, 232 together
  (`SetLedLimit(232)`).
- The ids and channel names are those of SignalRGB setups, so an imported layout and
  channel setup apply to them unchanged.

### Bench (2026-10-18, SignalRGB closed)

| Check | Result |
|-------|--------|
| Start sequence and configuration vs the recording | byte-identical (reset, configuration with 60 + 60, EZ LED query) |
| Probe decode of Lightloom's own stream | same slot order as above (JRGB1 at slot 6, JRGB2 at 7, strips at 8-67 and 68-127) |
| `--restore-check` (0x52 and 0x53 before `Initialize` and after `Shutdown(false)`) | identical |
| `lightloom effect effects/rainbow-wave.html`, 20 s | 582 frames, 586 SET_FEATUREs, 0 errors, canvas to wire p50 / p99 13 / 13 ms |

A 725-byte SET_FEATURE takes about 12-13 ms on this board, against 3.6 ms for the
185-byte report of zone mode; at 30 fps that is well inside the frame time.

## Save to devices and exit modes (Lightloom extensions, 2026-10-23, untested on the board)

"Save to devices" ([docs/host-api/save-shutdown.md](../host-api/save-shutdown.md)) stores a static
(or breathing) configuration in the controller's flash with the save byte `0xB8 = 1`. The packet
is built on the restore image: the records of this board's zones (JRGB1 `0x01`, JRGB2 `0xAE`,
JRAINBOW1 `0x1F`, JRAINBOW2 `0x2A`, the onboard master `0x4A` and the nine onboard slots
`0x54..0xA4`) become `[effect, R, G, B, options, R, G, B, flags, 0]` with effect `0x01` (static) or
`0x02` (breathing), options `0x28` (brightness 10, speed 0) or `0x29` (speed medium), flags `0x80`
(custom colour; the master `0x81`, onboard sync), one colour per zone (the average of its LEDs on
the frame shown), and the strips' LED counts at `+10` of the JRAINBOW records; every other byte as
read. The values are those of OpenRGB's `MSIMotherboard185Controller::SetMode()` /
`SetZoneColor()` for `MSI_MODE_STATIC` / `MSI_MODE_BREATHING` (MSIMotherboardCommon.h flag
layout); the composition is Lightloom's.

The send sequence is OpenRGB's `MSIMotherboard185Controller::Update(true)` for non-direct modes:
GET 0x52, SET what was read with the save byte set, then SET the new configuration with the save
byte set ("Windows Mystic Light sends two consecutive reports", OpenRGB's comment). The first
report therefore also writes the per-LED configuration Lightloom had set to flash, immediately
replaced by the second; if the second failed the board would boot in per-LED mode with no stream
(dark) until saved again. After the save the plugin reads 0x52 again (the new restore image) and
re-enters per-LED mode (reset report, configuration with save byte 0).

Exit modes: `Shutdown(false, "keep")` sends a black stream and the same static configuration with
the last colours, `"off"` with black, both with the save byte 0 (volatile).

## Safety rules (carry into the host and the CLI)

- Nothing is sent before a successful GET of 0x52 (the restore image). The restore packet is
  that image; the per-LED configuration is the packet recorded on the wire (S6).
- Byte `0xB8` is `0` in every packet except the two of "Save to devices" (above), which the user
  asks for and confirms; nothing else persists to the controller's flash.
- Byte `0x3E` is written as the read value - 4 (see "Bench results": the controller reads it back
  as the written value + 4).
- Nothing is written on `Shutdown(false)` when the session never wrote.
- Other lighting software must not be running. The host reports SignalRGB and OpenRGB itself;
  the plugin's `ConflictingProcesses()` adds MSI's lighting services `LEDKeeper.exe`,
  `LEDKeeper2.exe`, `LightKeeperService.exe` and `LightKeeperService2.exe` (names from public
  reports, not verified on this machine, where MSI Center is not installed). Conflicts are
  reported, never killed.
- `--dry-run` in the CLI prints the packet diff against the restore image instead of sending.
- A `send_report` return of `-1` is logged; the plugin keeps running (next frame retries).

## Open questions for the M1 bench session

| Question | Status after the 2026-10-18 bench |
|----------|-----------------------------------|
| Which 6 of the 10 onboard slots are wired (0..5 vs 1..6)? | **Open.** The rainbow was visible on the board's LEDs (user report); a per-slot colour test is still to do. |
| Does the board accept static colour on a slot while the restore image's master slot (0x4A) still says `0x25`? | **Yes.** After SignalRGB the image held `0x25` (per-LED mode) on `0x4A`; our packet (static on the six onboard records) was applied and read back. |
| Brightness field: is `0x28` full brightness? What does MSI Center write at 100 %? | Open (MSI Center not installed). `0x28` = brightness 10 of a 0-10 scale per S1; the board lit visibly. |
| Does GET 0x52 return exactly 185 bytes on Windows? | **Yes**, 185 bytes. Windows refuses GET_FEATURE buffers shorter than the collection's longest feature report (725), so the host retries with 725 and passes the real size to `getLastReadSize()`. |
| Does `Shutdown(false)` really restore the pre-run appearance? | **Yes** at protocol level: GET of 0x52 and 0x53 before `Initialize` and after `Shutdown` are byte-identical (once the 0x3E read-back bias is handled). |

## Bench results (2026-10-18)

Setup: MSI MAG B550 TOMAHAWK (1462:7C91), Windows 11, SignalRGB closed for the runs and relaunched
afterwards. Commands: `lightloom light|rainbow --restore-check`, with and without `--dry-run`.

**State left by SignalRGB.** The 0x52 image read after closing SignalRGB has effect `0x25`
(per-LED stream mode) on `J_ONBOARD_LED_0` (`0x4A`), options `0xA9`, sync flags `0x87`; every other
record is static (`0x01`). JRAINBOW1/2 report 60 LEDs (`0x3C` at `0x29` and `0x34`).

**Packets are applied.** A GET right after a live run that left the board red (`--keep`) returns the
six onboard records exactly as sent (`01 FF 00 00 28 FF 00 00 80 00`). Two exceptions:

| Offset | Written | Read back | Explanation |
|--------|---------|-----------|-------------|
| `0x3E` (J_CORSAIR record +9) | `v` | `v + 4` | Firmware behaviour, seen on every write (0x04 → 0x08 → 0x0C … 0x24 over successive sessions that wrote the read value back). SignalRGB writes `0x00` there and the controller reports `0x04`. The plugin now writes `read - 4`, which keeps the value stable across sessions. |
| `0x81` (slot 6 colour 2, red) | `0xFF` | `0x00` | Slot 6 does not retain colour 2 (probably a single-LED slot). Ignored when the plugin recognises its own packet on re-`Initialize`. |

GETs alone change nothing (dry-run sessions read identical images before and after).

**Restore.** With both quirks handled, `--restore-check` (every feature report the descriptor
declares: 0x52 and 0x53) is byte-identical before `Initialize` and after `Shutdown(false)`, for
normal ends, `--duration` and Ctrl-Break.

**Performance** (release build; 30 fps rainbow; one 185-byte SET_FEATURE per frame because every
frame changes):

| Run | Frames | HID errors | Render p50 / p99 / max | SET_FEATURE p50 / p99 | Peak RSS |
|-----|--------|------------|------------------------|-----------------------|----------|
| 10 min (hidapi backend) | 17 989 | 0 | 3.75 / 4.28 / 7.86 ms | 3.62 / 4.11 ms | 10.6 MB |
| 60 s (native backend) | 1 798 | 0 | 3.74 / 4.25 / 4.44 ms | 3.60 / 4.10 ms | 10.5 MB |

`Render()` time is dominated by the USB control transfer (Windows pads the 185-byte report to the
collection's 725-byte feature length). Command start to the first write (`lightloom light`): 431 ms
with the hidapi backend (its context enumerates and reads strings of every HID device first),
**56-58 ms** with the native backend.
