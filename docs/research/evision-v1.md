<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# EVision V1 keyboards (Sonix `320F:5064` and 12 others): what is volatile, what is stored, and how to find out

Status: research only. Nothing here has been sent to a keyboard. The plugin
`plugins/evision-keyboard/` drives only the V2 protocol; the V1 decision and the V2 protocol are
in `docs/hardware/evision-keyboard.md`. This note covers the reference machine's keyboard,
`320F:5064`, and what it would take to light it safely.

## Sources and licences

OpenRGB at commit `d4798c12` (clone in the scratchpad, read only). All files read carry
`SPDX-License-Identifier: GPL-2.0-or-later`, so they may be adapted with a credit header; none is
GPL-2.0-only.

| File (under `Controllers/EVisionKeyboardController/`) | Author line | Licence |
|---|---|---|
| `EVisionKeyboardController/EVisionKeyboardController.h`, `.cpp` | Adam Honse, 15 Mar 2020 | GPL-2.0-or-later |
| `EVisionKeyboardController/EVisionKeyboardControllerDetect.cpp` | (none) | GPL-2.0-or-later |
| `EVisionV2KeyboardController/RGBController_EVisionKeyboard.cpp`, `.h` (the V1 RGBController; it sits in the V2 directory) | Adam Honse | GPL-2.0-or-later |
| `EVisionV2KeyboardController/EVisionV2KeyboardController.h`, `.cpp`, `RGBController_EVisionV2Keyboard.*`, `…Detect.cpp` | Le Philousophe, 25 Dec 2022 | GPL-2.0-or-later |

No SignalRGB plugin exists for `320F:5064` (`docs/hardware/imported-plugins-m2.md`), so there is
nothing to compare against there, and SignalRGB files are off limits anyway.

## The V1 command set (OpenRGB, `EVisionKeyboardController.h/.cpp`)

Transport: interface 1, usage page `0xFF1C` (on this machine `MI_01&Col04`, usage `0x0092`).
Every request is a 64-byte output report with report id `04`, written with `hid_write`, and each
is followed by a blocking `hid_read` of 64 bytes (OpenRGB ignores the answer's contents).

```
byte 0     04            report id
byte 1,2   checksum      16-bit sum of bytes 3..63, low byte first (begin/end: fixed 01 00 / 02 00)
byte 3     command
byte 4     size          data bytes, at most 0x36 (54) for colour data
byte 5,6   offset        low byte first
byte 7     00
byte 8..   data
```

OpenRGB sums `char` values, so on x86 it adds bytes >= 0x80 as negatives (see the V2 doc); a
re-implementation must decide which sum the firmware really checks, which a capture settles.

| Cmd | OpenRGB name | Used by OpenRGB | Payload |
|---|---|---|---|
| `01` | `COMMAND_BEGIN` | defined, never sent | none |
| `02` | `COMMAND_END` | defined, never sent | none |
| `06` | `SET_PARAMETER` | every mode change (`SendKeyboardModeEx`) | size 8 at "parameter" 0: mode, brightness 0..4, speed 0 (fast)..5 (slow), direction, random flag, R, G, B |
| `10` | `READ_CUSTOM_COLOR_DATA` | defined, never sent | (reads the custom table) |
| `11` | `WRITE_CUSTOM_COLOR_DATA` | every per-key update (`SetKeyboardColors`) | 7 x 54 bytes at offsets 0, 54, ..., 324: 126 keys x R G B |

Parameter indices (`EVISION_KB_PARAMETER_*`): 0 mode, 1 brightness, 2 speed, 3 direction, 4 random
colour flag, 5 mode colour (3 bytes), `0x0F` polling rate, `0x11` "surmount" mode colour. The
indices match byte positions in the 8-byte block `SendKeyboardModeEx` writes at index 0, so the
"parameter" field is in practice a **byte offset into one stored configuration block** with a
size: `06 / size 8 / offset 0` rewrites bytes 0..7. A write that reached offset `0x0F` would change
the USB polling rate, which Lightloom must never touch. The V2 protocol has the same shape
(`06` = write config at an offset, `05` = read config, `01`/`02` = begin/end configure), which
supports this reading; V1 `05` is not in OpenRGB and is unconfirmed.

Modes (`EVISION_KB_MODE_*`): `01..12` built-in effects (wave, wheel, spectrum, breathing, `06`
static, reactive variants, starlight, blooming, hurricane, accumulate, visor, surmount, rainbow
circle) and `14` custom (per-key table). No direct/software mode exists in V1; V2 adds `FF`
direct plus command `12` "send dynamic colours" / `13` "end dynamic colours".

## Volatile vs stored

What OpenRGB tells us:

- Every V1 mode in `RGBController_EVisionKeyboard.cpp` carries `MODE_FLAG_AUTOMATIC_SAVE`
  ("mode automatically saves"), and the supported-devices doc tag is `@save :robot:` /
  `@direct :x:`. So both `06` (mode/config block) and `11` (custom colour table) are, as far as
  OpenRGB's authors observed, **persistent across power cycles**: they land in the MCU's flash.
- OpenRGB never brackets them with `01`/`02`. Whether the firmware commits each `11` request
  straight to flash, buffers the table in RAM and commits on a timer or a page boundary, or
  commits on `02` (as V2's begin/end-configure pair suggests) is **unknown**.
- OpenRGB's per-key "Custom" mode therefore writes the stored custom table on every LED update
  (7 requests, 378 bytes). Driven at 30 frames/s that is 30 table rewrites per second. Sonix
  SN32F2xx flash is rated in the 10^4 to 10^5 erase-cycle range (check the exact part's
  datasheet once the MCU is identified); if each frame costs one erase, a page wears out in
  minutes to an hour. This is why batch 1 left V1 per-key out, and it stays out.

Nothing known in V1 is volatile. The only volatile candidates are (a) the same `11` table if the
firmware commits only on `02`, or (b) an undocumented live command used by a vendor "music",
"screen sync" or "real-time" mode, perhaps `12` as in V2. Both are capture questions (below).

## Is there a safe mode? Yes, for whole-keyboard lighting, with writes only on user change

Without a volatile command the keyboard cannot follow the Lightloom canvas. It can still be
managed safely as a **stored-state device** through the bake path (`feature/bake`, in progress):

1. **Built-in effects and static colour**: one `06` write of the 8-byte block at offset 0
   (mode, brightness, speed, direction, random, R, G, B), **sent only when the user changes a
   setting** in the device panel or presses "save to device", debounced (for example 1 s after
   the last slider move), never from `Render()`, never on start-up, never on exit. Static red is
   `mode 06, R FF G 00 B 00`. Never write past offset 7 (polling rate lives at `0x0F`).
2. **Per-key custom layout** (mode `14` + one `11` table, 7 requests): only as an explicit
   "bake this frame to the keyboard" action, with a confirmation that it writes keyboard memory,
   rate-limited (for example at most once per 10 s), and skipped when the table equals the last
   one written (cache it in Lightloom's database, because V1 has no confirmed read-back; `10` read
   is defined but untested).
3. **No per-frame path at all**: the device is shown as "hardware lighting only" in the
   dashboard and excluded from canvas sampling. `Render()` sends nothing.
4. **Shutdown/on-exit**: nothing. The keyboard keeps its own stored state, which is what the
   user last baked.

This needs a plugin capability "stored settings only" (no live frames) and a host-side write
budget enforced in the bake path, not in plugin code alone. Until a capture confirms the packet
format (checksum, answers, whether `01`/`02` are required), even this mode should stay out of
`tested` and be offered behind an explicit opt-in.

## What a capture of the vendor software must establish

1. **The live path.** Does the vendor app have a mode that animates keys from the PC (music
   rhythm, screen colour, game, "real-time")? Which command does it stream, at what rate? If it
   is not `11` (for example `12`), that is the volatile direct path and V1 can be driven like V2.
2. **Begin/end.** Does the app wrap `06` and `11` writes in `01` ... `02`? If it does, test
   (later, by the user) whether `11` without `02` survives an unplug: if not, the table is RAM
   until `02` commits it, and `11` alone is a volatile per-key path.
3. **Read-back.** Which requests the app sends at start-up to read the current mode and colours
   (`05`? `10`? others), so Lightloom can show the stored state without writing.
4. **Checksum and answers.** The exact checksum over bytes >= 0x80, and the answer format
   (does byte 7 carry a status, as in V2?).
5. **Key map.** Which byte offsets in the `11` table belong to which key on this exact layout
   (OpenRGB's 126-entry, 6 x 23 matrix is generic).
6. **Anything else written** on profile switch, "reset", or app exit, so Lightloom never repeats
   it by accident.

## Capture procedure for the user (USBPcap + Wireshark, Windows)

Preparation:

1. Install Wireshark and tick **USBPcap** in its installer (USBPcap is a filter driver; Windows
   asks for a reboot). This is the user's own install; Lightloom agents never install drivers.
2. Quit everything that talks to the keyboard: SignalRGB (tray icon > Exit), OpenRGB, and stop
   Lightloom's daemon if it is running (Lightloom does not drive `320F:5064`, but quitting it keeps
   the capture clean).
3. Install the keyboard's vendor software (the OEM app for this model; EVision keyboards are sold
   by Redragon, Ajazz, Tecware and others) but do not start it yet.
4. Privacy: the capture records every key typed on this keyboard. Do not type passwords while
   capturing; use the mouse for the vendor app. The filter in step 12 removes key reports before
   sharing.

Find the right USBPcap interface:

5. Start Wireshark as administrator. The capture list shows `USBPcap1`, `USBPcap2`, ... (one per
   root hub). Click the gear next to each to see its device tree; pick the one listing
   `320F:5064` (or run `"C:\Program Files\USBPcap\USBPcapCMD.exe"` and choose the hub listing it).
   In the interface options, tick only that device if the tree allows it.

Record, with markers:

6. Start the capture, then start the vendor app and leave it idle for 10 s (start-up reads).
7. Between steps, mark the capture by pressing **Scroll Lock three times** on the keyboard (key
   reports on interface 0 show up as clear markers) and write down the step and the time.
8. Steps, one at a time, each followed by "Apply"/"Save" if the app has one:
   - a. Static mode, colour red; then static green.
   - b. Brightness: move the slider one step down, then one step up.
   - c. A built-in effect (wave); change its speed once and its direction once.
   - d. Custom/per-key mode: set Esc to blue and apply; then set F1 to blue and apply.
   - e. Any PC-driven mode (music/audio rhythm, screen sync, "real-time", game): run it 15 s.
   - f. If the app has profiles 1..3: switch profile once and back.
   - g. Close the app normally (File/Exit or tray > Exit), wait 5 s.
9. Stop the capture and save it as `evision-5064-vendor.pcapng`.
10. Without capturing: after step 8e (with the app closed) unplug and replug the keyboard, and
    note what it shows: the step-8d custom layout (the streamed colours were volatile) or the
    last streamed frame (they were stored). Note the same after step 8d.

Filter and inspect:

11. Find the keyboard's address: filter `usb.idVendor == 0x320f` and read `usb.device_address`
    from the GET DESCRIPTOR response (call it N).
12. Vendor traffic only (interface 1 OUT and its answers, no keystrokes):
    `usb.device_address == N && usb.data_len == 64` for interrupt transfers, plus
    `usb.device_address == N && usbhid.setup.bRequest == 0x09` in case the app uses SET_REPORT
    on the control pipe instead of the interrupt endpoint. Requests start with `04`
    (`usb.capdata[0] == 0x04`); the command is `usb.capdata[3]` (for control transfers, the same
    offsets in the data stage).
13. Per command: `usb.capdata[3] == 0x11`, `== 0x06`, `== 0x01`, `== 0x02`, `== 0x12`, and
    `!(usb.capdata[3] in {0x01 0x02 0x06 0x10 0x11})` for anything OpenRGB does not know.
14. Export for Lightloom:
    `tshark -r evision-5064-vendor.pcapng -Y "usb.device_address == N && usb.data_len == 64" -T fields -e frame.time_relative -e usb.endpoint_address -e usb.capdata > evision-5064.tsv`
    and share the `.tsv` plus the step/time notes (not the raw pcapng, which contains the
    keystrokes).

What to look for, per step: the command byte; size and offset; whether `01`/`02` frame the
writes; the request rate in step 8e (dozens per second means a live path); which command and
offsets change between 8d's two applies (key-to-offset map); what is read at start-up; the
answer bytes on the IN endpoint (echo, status at byte 7?).

## Outcome matrix

| Capture shows | Lightloom can do |
|---|---|
| a distinct live command (e.g. `12`) in the PC-driven mode | a V1 direct plugin like V2, per-key at canvas rate, with the vendor's end/exit command on shutdown |
| only `11`, bracketed by `01`/`02`, and the replug test shows `11` without `02` is lost | per-key via `11` without `02` (volatile), `02` only through the bake path |
| only `11`, unbracketed or always committed | no live path: the safe stored-only mode above, writes only on user change |
| no PC-driven mode in the app | the same: stored-only mode, unless a later firmware or tool shows otherwise |
