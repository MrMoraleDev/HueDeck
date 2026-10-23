<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Logitech G813 / G815 and G915 / G915 TKL: per-key lighting, adapted from OpenRGB

Plugin: `plugins/logitech-keyboard/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (`Controllers/LogitechController/...`) | License | Taken |
|---|---|---|
| `LogitechControllerDetect.cpp` | GPL-2.0-or-later | Product ids, names, detector macros (interface, usage page, usage), the 0xC547 name probe and its routing |
| `LogitechG815Controller/LogitechG815Controller.cpp`, `.h` (Cheerpipe, 20 Mar 2021) | GPL-2.0-or-later | Report layouts: per-key frames `0x1F` / `0x6F`, commit `0x7F`, single LED, read timeouts; the left-out `InitializeDirect()` |
| `LogitechG815Controller/RGBController_LogitechG815.cpp` (Cheerpipe) | GPL-2.0-or-later | `matrix_map`, `led_names` (generated into `layouts.js`), key-code shifts, the frame-building algorithm of `DeviceUpdateLEDs()`, the Direct start |
| `LogitechG915Controller/LogitechG915Controller.cpp`, `.h` (Cheerpipe, 20 Mar 2021) | GPL-2.0-or-later | Wired / wireless device and feature indices (0x4522, 0x8040, 0x8071, 0x8081), frames, commit, the left-out `InitializeDirect()` |
| `LogitechG915Controller/RGBController_LogitechG915.cpp` (Cheerpipe) | GPL-2.0-or-later | `matrix_map`, `matrix_map_tkl`, `led_names` (generated), the G915 frame-building algorithm |
| `RGBController/RGBControllerKeyNames.cpp` (Chris M (Dr_No)) | GPL-2.0-or-later | Which key each `KEY_EN_*` constant is (mapped to HueDeck key names) |
| `DetectionManager.cpp` | GPL-2.0-or-later | How a HID detector matches one collection. Read only |

## Products

| PID | OpenRGB name | Detector | Collection | Layout |
|---|---|---|---|---|
| `C232` | Logitech G813 RGB Mechanical Gaming Keyboard | `DetectLogitechKeyboardG815` (`_IP`) | interface 1, page `FF43`, usage `0602` | G815, 27 x 7, 117 LEDs |
| `C33F` | Logitech G815 RGB Mechanical Gaming Keyboard | same | same | same |
| `C541` | Logitech G915 Wireless ... | `DetectLogitechKeyboardG915` (`_IPU`) | interface 2, page `FF00`, usage 2 | G915 full, wireless |
| `C547` | Logitech G915 Wireless ... (Receiver 2) | `DetectLogitechKeyboardG915Receiver2` | same | by name probe: G915 TKL or G915, wireless |
| `C33E` | Logitech G915 Wireless ... (Wired) | `DetectLogitechKeyboardG915Wired` | same | G915 full, wired |
| `C545` | Logitech G915TKL Wireless ... | `DetectLogitechKeyboardG915` | same | G915 TKL (20 x 7, 95 LEDs), wireless |
| `C343` | Logitech G915TKL Wireless ... (Wired) | `DetectLogitechKeyboardG915Wired` | same | G915 TKL, wired |

OpenRGB's G815 detector also needs the usage `0604` (64-byte) collection to exist and opens it, but
never sends anything there; HueDeck validates only the `0602` collection.

**Held back:** `C336` Logitech G213 (see Safety).

**Not ported:** OpenRGB's unified HID++ 2.0 controller (`LogitechHIDPP20Controller`, every
Logitech id, run-time feature discovery, receivers, G915 X, G515, ...): a separate, very large
driver registered for any product id.

## Protocol

Every report is a 20-byte HID++ long report `11 <device> <feature index> <function|swid> <16 data>`,
written as an output report and followed by a read of the device's answer (OpenRGB:
`hid_read_timeout` 300 ms, or `hid_read` without a timeout; HueDeck reads with 300 ms everywhere).

| | device | per-key feature (0x8081) index |
|---|---|---|
| G813 / G815 | `FF` | `10` |
| G915 wired | `FF` | `0A` |
| G915 through the receiver | `01` | `0B` |

- **Start** (OpenRGB's switch to Direct, minus `InitializeDirect()`, see Safety): `1F` frame with
  only Escape (`29`) black, `11 dd ff 1F 29 00 00 00 ...`, then commit `11 dd ff 7F`.
- **Every frame** (`DeviceUpdateLEDs`): only keys whose colour changed since the last frame (all
  start black, so keys that stay black are never sent). Changed keys are grouped by colour, in
  ascending `RGBColor` order (`0x00BBGGRR`).
  - Big frame `6F`: `R G B k0 .. k12`, then `FF` and zeros if fewer than 13 keys.
  - Little frame `1F`: up to four `key R G B`, then `FF` and zeros if fewer than four.
  - G815: a colour with more than four keys goes out at once in big frames of 13; the others fill
    little frames, carried over from colour to colour, sent when full; the last partial one at the
    end.
  - G915: per colour, chunks of 13; a chunk of more than four keys is a big frame, a smaller one
    goes into little frames; all big frames first, then all little frames.
  - Then commit `7F`. Nothing is sent when no key changed.
- Key codes: `led_names` index shifted by zone: keyboard `-0x03`, modifiers `-0x78`, G-keys `+0xB3`,
  logo `+0xD1`, media and the lighting/brightness key unchanged.
- **0xC547 name probe** (read-only, HID++ short reports): `10 01 03 0E 00 00 00` (read 100 ms),
  `10 01 03 1E 00 00 00`, then up to three reads (200 ms) for `11 01 03 1E <name>`. "G915 X" is
  left alone (OpenRGB hands it to the unified controller), "G915 TKL" is the TKL, other "G915" the
  full keyboard, anything else (G515, ...) is left alone.
- **Shutdown**: every key black (the same changed-keys frames) and commit. OpenRGB has no exit
  step; returning to the onboard effect would need the cluster-effect write HueDeck leaves out.

## Safety notes and deviations

- **Left out: `InitializeDirect()`** ("I used same sequence as GHUB for screen capture"). It sends
  four writes before Direct:
  1. G915 feature `0x4522` (index `0E` wired / `0F` wireless) functions 3 and 1 with no data.
     0x4522 is the "disable keys by usage" (game mode) feature: not lighting.
  2. G915 feature `0x8071` (RGB effects, index `09` / `0A`) function 1 (set cluster effect) for
     clusters 0 and 1, effect 0, with byte `0x10` = `01`. OpenRGB's own
     `LogitechProtocolCommon.cpp` labels that byte "Bit 2-3 Power Mode : Bit 1-0 Persistence", so
     this is very likely a persistent (stored) effect write.
  The G815 sends the same four writes to indices `08` and `0F`; that those are 0x4522 and 0x8071
  on the G815 is inferred from the identical pattern, OpenRGB does not name them. HueDeck sends
  none of them. **First thing to check on hardware**: whether per-key frames show without them
  (if the onboard effect keeps running over them, a volatile way to stop it is needed; do not just
  restore these writes).
- **Not ported**: every hardware effect (`SetMode`, `InitializeModeSet`): they carry the same
  persistence byte. OpenRGB marks both families `@save :robot:` (saving is automatic).
- **G815 last little frame**: OpenRGB's `memset(frame + n*4 - 1, 0, 16 - n*4)` starts one byte early
  (it clears the last pair's blue byte) and leaves byte 15 as it was. HueDeck keeps the blue byte
  and zeroes everything after the `FF` end byte, as the G915 code does.
- **G213 held back** (`C336`): its only colour command (`SetDirect`, 0x8070 set-zone-effect
  `11 FF 0C 3A <zone> 01 R G B 02`, 5 zones) is the same command OpenRGB uses for the G213's
  hardware modes, and OpenRGB marks the G213 `@save :robot:` (automatic save) without ever turning
  saving off (unlike the Lightsync mice, see `logitech-mouse.md`). Sending it 30 times a second
  could write the keyboard's flash.

## Untested

Every product id. Nobody here owns these keyboards.
