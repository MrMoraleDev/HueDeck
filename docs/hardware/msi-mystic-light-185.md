<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# MSI Mystic Light "185-byte" motherboard controller (USB HID 1462:7C91)

Hardware notes for the first-party plugin `plugins/msi-mystic-light-7c91/plugin.js` (milestone M1).
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

## What the Lightloom M1 plugin does, exactly

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

Golden packet: `plugins/msi-mystic-light-7c91/fixtures/expected-packet.json`
(test: `node --test "plugins/**/*.test.mjs"` from the repo root; Node >= 21 does not expand a bare
directory argument).

## Safety rules (carry into the host and the CLI)

- Never send a 0x52 packet that was not derived from a successful GET of the same report.
- Byte `0xB8` is always `0` — Lightloom never persists to the controller's flash.
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
