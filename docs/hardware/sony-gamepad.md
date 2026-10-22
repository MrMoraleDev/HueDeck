<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Sony DualShock 4 and DualSense: light bar and player LEDs over USB (from OpenRGB)

The plugin is `plugins/sony-gamepad/`, adapted from OpenRGB's `SonyGamepadController`. It drives
the light bar of the DualShock 4 (and of its USB wireless adapter), and the light bar and the five
player LEDs of the DualSense and DualSense Edge. USB only. It never enables rumble, haptics, the
adaptive triggers, volume, audio routing or the microphone LED. Only the DualSense (`054C:0CE6`)
has been run on hardware (see below).

## Tested on hardware

2026-10-22, on the user's DualSense (`054C:0CE6`) over USB: the plugin (run from a copy without
the `ProductId()` filter that then left the DualSense ids to the single-device plugin) lit the
light bar red, green and blue in turn, with 0 errors. This plugin now owns `0CE6` and `0DF2`; the
light-bar-only single-device plugin `sony-dualsense` was retired the same day (its report field
table and bench runs stay in `docs/hardware/sony-dualsense.md`). `0CE6` is marked tested in
`manifest.json`. The DualSense Edge (`0DF2`) goes through exactly the same code path as the
DualSense (OpenRGB registers both with the same detector, and the plugin picks the DualSense
protocol for both), but has not been run on hardware. The DualShock 4 ids are untested too.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | OpenRGB `Controllers/SonyGamepadController/` at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB): `SonyGamepadControllerDetect.cpp`; `SonyDS4Controller/SonyDS4Controller.cpp`, `.h`, `RGBController_SonyDS4.cpp`, `.h`; `SonyDualSenseController/SonyDualSenseController.cpp`, `.h`, `RGBController_SonyDualSense.cpp`, `.h`. Every header: `SPDX-License-Identifier: GPL-2.0-or-later`. Authors as in the headers: Pol Rius (alpemwarrior) 24 Sep 2020, Flora Aubry 01 Jul 2022, Yoan Berthelot 06 Mar 2025 | Adapted into `plugin.js`: product ids, names, zones and LEDs, every report byte, the start-up I/O |
| S2 | OpenRGB, same commit, `GPL-2.0-or-later` (Adam Honse): `RGBController/RGBController.cpp`, `RGBController/RGBControllerInterface.h`, `DetectionManager.h`, `DetectionManager.cpp` | Read for semantics only: `RGBColor` is `0x00BBGGRR`; colours are stored zone after zone; `REGISTER_HID_DETECTOR` matches any interface, usage page and usage; `RGBController::Shutdown()` only stops OpenRGB's thread and sends nothing |
| S3 | Linux `drivers/hid/hid-playstation.c` (`GPL-2.0-or-later`), read 2026-10-18 | Names of the report fields and valid-flag bits. Facts only, no code |
| S4 | Linux v5.15 `drivers/hid/hid-sony.c` (`GPL-2.0-or-later`) | The DualShock 4 USB report there also used `buf[1] = 0x07; /* blink + LEDs + motor */`; "If both delay values are zero the DualShock 4 disables blinking."; over Bluetooth the DualShock 4 sends report 1 until "feature report 2 is requested", then report 17 |
| S5 | pydualsense (`pydualsense/pydualsense.py`, `pydualsense/enums.py`), read 2026-10-18 | DualSense bits S3 leaves unnamed: `valid_flag0` 0x04/0x08 "set the right/left trigger motor"; `valid_flag1` 0x40 "adjustment of overall motor/effect power (index 37)"; `valid_flag2` 0x01 `PlayerLedBrightness`, 0x02 `UninterrumpableLed`; byte 43 `high = 0`, `medium = 1`, `low = 2`. Facts only |
| S6 | Lightloom's device listing on the reference machine, and a recording of the (since retired) `sony-dualsense` plugin on the user's DualSense (`lightloom light red --record`, 2026-10-18) | The DualSense's only HID collection (interface 3, usage page 0x0001, usage 0x0005, 48-byte output report); the replay below |

## Products

All five are registered with `REGISTER_HID_DETECTOR` (no interface, usage page or usage).

| USB id | OpenRGB constant | Name (OpenRGB) | Report | LEDs |
|--------|------------------|----------------|--------|------|
| `054C:05C4` | `SONY_DS4_V1_PID` | Sony DualShock 4 | 0x05, 11 bytes | light bar |
| `054C:09CC` | `SONY_DS4_V2_PID` | Sony DualShock 4 | 0x05, 11 bytes | light bar |
| `054C:0BA0` | `SONY_DS4_RECEIVER_PID` (USB wireless adapter) | Sony DualShock 4 | 0x05, 11 bytes | light bar |
| `054C:0CE6` | `SONY_DUALSENSE_PID` | Sony DualSense | 0x02, 48 bytes | light bar, 5 player LEDs |
| `054C:0DF2` | `SONY_DUALSENSE_EDGE_PID` | Sony DualSense Edge | 0x02, 48 bytes | light bar, 5 player LEDs |

The DualShock 4 name comes from `RGBController_SonyDS4` (the same for all three ids); the
DualSense names are the detector names, which `RGBController_SonyDualSense` takes over.

## Transport and endpoint

OpenRGB opens every HID collection of these ids. Over USB each controller has one: the game pad
collection, usage page `0x0001`, usage `0x0005` (S6 for the DualSense: interface 3, the only HID
interface). `Validate()` accepts that collection on any USB interface (`interface >= 0`), like
OpenRGB's detector, and refuses `interface == -1`, which Lightloom reports for a HID device without
a USB interface (Bluetooth). No static `interfaces` filter in the manifest.

## Geometry

The plugin's static metadata is a placeholder (one LED at (0, 0)); `Initialize()` sets the name,
grid and LEDs from `device.productId()`. LED order is OpenRGB's colour order on the wire.

| Product | OpenRGB zones (LEDs) | Lightloom grid | LEDs (name: cell) |
|---------|----------------------|--------------|-------------------|
| DualShock 4 | "Controller Zone", single ("Controller LED") | 1 x 1 | Lightbar: (0, 0) |
| DualSense, Edge | "Lightbar", single ("LED 1"); "Player LEDs", linear ("Player 1".."Player 5") | 5 x 2 | Lightbar: (2, 0); Player 1..5: (0..4, 1) |

The light bar row has one LED, placed above the middle of the player row as on the controller.
The light bar LED is named "Lightbar" on every product, a rename of OpenRGB's LED names
("Controller LED" on the DualShock 4, "LED 1" in the DualSense "Lightbar" zone) so that the same
canvas cell has the same name everywhere; the name is not sent to the device. The "Player LED
Brightness" parameter is static plugin metadata, so it is also listed for the three DualShock 4
ids, where it does nothing (the DualShock 4 report has no such byte).

## Start-up (`Initialize()`)

- **DualShock 4** (OpenRGB: `SonyDS4Controller` constructor): GET_FEATURE report `0x02`, 64 bytes
  (on USB the calibration report, S3 `DS4_FEATURE_REPORT_CALIBRATION`; the reply is not used),
  then up to five input reads of 64 bytes. An input report `0x11` (17) means the controller talks
  Bluetooth reports; OpenRGB then switches to its Bluetooth output report, Lightloom sends nothing
  at all (logged). OpenRGB's `hid_read()` blocks; Lightloom waits at most 100 ms per read (the
  controller sends an input report every few milliseconds), so a silent device costs 0.5 s once.
  Both are reads; nothing is written at start.
- **DualSense** (`SonyDualSenseController` constructor): no I/O. Nothing is sent at start (the
  retired `sony-dualsense` plugin's "light bar setup" start report is not part of OpenRGB's USB
  path; the 2026-10-22 hardware test showed the light bar following the host without it).

## Every frame (`Render()`)

One output report per frame (about 30 per second), every frame, as OpenRGB writes one per update.

### DualShock 4: output report 0x05 (`SonyDS4Controller::sendReportUSB`)

| Byte | Field (S3) | OpenRGB | Lightloom |
|------|------------|---------|---------|
| 0 | report id | `05` | `05` |
| 1 | `valid_flag0`: 0x01 motors, 0x02 light bar, 0x04 light bar blink | `07` | **`06`** |
| 2 | `valid_flag1` | `00` | `00` |
| 3 | reserved | `00` | `00` |
| 4, 5 | `motor_right`, `motor_left` | `00 00` | `00 00` (ignored: flag clear) |
| 6, 7, 8 | light bar red, green, blue | colour | colour |
| 9, 10 | `lightbar_blink_on`, `lightbar_blink_off` | `00 00` | `00 00`: no blinking (S4) |

`hid_write(dev, buffer, 11)`: 11 bytes. The collection's output report is 32 bytes (S3
`DS4_OUTPUT_REPORT_USB_SIZE`); hidapi (OpenRGB) and Lightloom's Windows backend both pad the write
with zeros to that length.

### DualSense: output report 0x02 (`SonyDualSenseController::SetColors`, USB branch)

48 bytes (`SONY_DUALSENSE_USB_PACKET_SIZE`, the output report length Windows reports, S6).

| Byte | Field (S3, S5) | OpenRGB | Lightloom |
|------|----------------|---------|---------|
| 0 | report id | `02` | `02` |
| 1 | `valid_flag0`: 0x01 compatible vibration, 0x02 haptics select, 0x04/0x08 right/left trigger effect | `0F` | **`00`** |
| 2 | `valid_flag1`: 0x01 mic mute LED, 0x04 light bar colour, 0x10 player LEDs, 0x40 motor/effect power | `55` | **`14`** |
| 3..8 | motors, headphone/speaker/mic volume, audio control | `00` | `00` |
| 9 | `mute_button_led` | mode value: Direct `01` (LED on), "Mic Off" `00`, "Mic Pulse" `02` | **`00`** (ignored: flag clear) |
| 10..38 | power save, trigger parameters, `audio_control2` | `00` | `00` |
| 39 | `valid_flag2`: 0x01 player LED brightness, 0x02 light bar setup, 0x04 compatible vibration 2, 0x08..0x80 unnamed | `FF` | **`03`** |
| 40, 41 | reserved | `00` | `00` |
| 42 | `lightbar_setup` (0x02 = fade out) | `00` | `00` |
| 43 | `led_brightness` (player LEDs): 0 high, 1 medium, 2 low | `02` minus brightness | the same, from the parameter |
| 44 | `player_leds`: bit 0..4 = Player 1..5; OpenRGB always adds `0x20` (not named by S3 or S5, see below) | `0x20` + bits | the same |
| 45, 46, 47 | light bar red, green, blue | colour | colour |

About the `0x20` bit of `player_leds`: neither S3 nor S5 names it. Community descriptions of the
DualSense report (unverified, not checked on hardware) call it the bit that makes the player LEDs
switch instantly instead of fading in; either way it only affects how the player LEDs turn on, a
lighting detail. It is kept exactly as OpenRGB sends it.

A player LED is on when its colour is not black (OpenRGB: `colors[i] > 0`). Brightness is
OpenRGB's Direct mode brightness, 0..2, default 1; Lightloom's "Player LED Brightness" parameter
(Low / Medium / High = 0 / 1 / 2, default Medium) feeds the same byte. With Lightloom's canvas the
player LEDs are lit whenever their cells are not black, as with OpenRGB's effects.

## What Lightloom removes, and why

The controllers apply a field only when its valid flag is set (the Linux driver sets each flag
only when it updates that field, S3), so clearing a flag leaves that feature exactly as the
controller or another program set it.

| Report | Flag | Meaning | Why it is removed |
|--------|------|---------|-------------------|
| DS4 byte 1 | 0x01 | rumble motors | with the zero motor bytes it stops any rumble a game started, every frame |
| DualSense byte 1 | 0x01, 0x02 | vibration, haptics select | rumble/haptics |
| DualSense byte 1 | 0x04, 0x08 | right, left adaptive trigger effect | with zero trigger bytes it turns trigger effects off |
| DualSense byte 2 | 0x01 | microphone mute LED | OpenRGB's Direct mode lights the microphone LED (byte 9 = 1) |
| DualSense byte 2 | 0x40 | motor/effect power (byte 37) | haptics |
| DualSense byte 39 | 0x04 | compatible vibration 2 | rumble |
| DualSense byte 39 | 0x08..0x80 | not named by any source | unknown effect |

Kept as OpenRGB sends them: the light bar flags (DS4 0x02 and 0x04 with blink 0/0; DualSense
`valid_flag1` 0x04 and `valid_flag2` 0x02 with `lightbar_setup` 0), and the player LED flags
(`valid_flag1` 0x10, `valid_flag2` 0x01). Byte 9 is written as 0 instead of OpenRGB's mode
value; with its flag clear the controller ignores it either way.

## Shutdown (`Shutdown()`)

OpenRGB sends nothing when it exits (S2), and neither protocol as OpenRGB uses it has a command
that hands the lighting back to the controller. Lightloom therefore turns off what it drove, the
same on exit and on system suspend:

- DualShock 4: `05 06 00 00 00 00 00 00 00 00 00` (light bar black, steady).
- DualSense: the frame report with a black light bar and every player LED off (`player_leds`
  `0x20`), the brightness byte from the parameter.

## Left out

- **Bluetooth.** OpenRGB builds Bluetooth reports for both controllers (DualShock 4 `0x11`,
  DualSense `0x31`, 78 bytes with a CRC-32 seeded with `0xA2`). Not ported: the user's controller
  is on USB; OpenRGB's DualSense Bluetooth report starts its fields one byte earlier than the
  Linux driver's (`31 02 0F 55 ...` against `31 <seq> 10 <flags> ...`, S3), so the layout is
  uncertain; over Bluetooth the start-up feature read switches the DualShock 4 to report 17 (S4),
  which changes the input reports other programs see; and the sequence counter is never advanced.
  `Validate()` refuses Bluetooth collections and a DualShock 4 that answers with `0x11` input
  reports gets nothing.
- **"Mic Off (Direct)" and "Mic Pulse (Direct)"**: these OpenRGB modes differ from Direct only in
  the microphone LED.
- Rumble, haptics, trigger, audio and microphone LED flags (above).
- Serial number and location strings: OpenRGB only displays them.
- No flash or profile write exists in this family.

## Ownership of the DualSense ids (formerly a collision with `plugins/sony-dualsense`)

Until 2026-10-22 the single-device plugin `plugins/sony-dualsense` (light bar only, tested on
2026-10-18) also claimed `054C:0CE6` and `054C:0DF2`, and this plugin left them out of its
`ProductId()` so the two would not collide in Lightloom's VID:PID index
(`crates/lightloom-plugin-host/src/catalog.rs`, "last scanned wins"). After this plugin was tested on
the user's DualSense (see "Tested on hardware") the single-device plugin was retired and this
plugin claims both DualSense ids.

What changed on the controller compared with the retired plugin (all lighting, nothing else):

- no "light bar setup" start report (`sony-dualsense` sent one; OpenRGB's USB path does not);
- the five player LEDs are driven on every frame: all five lit whenever their canvas cells are
  not black, and `valid_flag2` `0x03` plus `led_brightness` in every report;
- at shutdown the player LEDs are turned off, including a player indicator that Steam or a game
  had set before Lightloom started.

## Validation (2026-10-18, repeated 2026-10-19 after review; no hardware)

- `node --test "plugins/sony-gamepad/**/*.test.mjs"`: every product id, byte for byte against
  `fixtures/expected.json`, which holds OpenRGB's report and Lightloom's for each case, worked out
  from the C++ (not from `plugin.js`).
- `lightloom compat dry-run --initialize plugins/sony-gamepad`: parsed and evaluated, Initialize
  and Shutdown without errors, no API gaps (it runs the first product id, a DualShock 4).
- `lightloom compat replay` of the `sony-dualsense` recording on the user's DualSense (red, 113
  frames): 114 reports against the recording's 115. The light bar bytes (45..47) match in every
  frame and at shutdown. The differences are all lighting fields OpenRGB sets: no start report
  (OpenRGB sends none on USB); in each frame `valid_flag1` `14` instead of `04` (player LEDs),
  `valid_flag2` `03` instead of `00`, `led_brightness` `01` instead of `00`, `player_leds` `3F`
  instead of `00` (all five lit: the canvas is red); at shutdown `player_leds` `20`.

## Untested

Every product id except the DualSense `0CE6` (see "Tested on hardware"): the DualShock 4 ids
(`05C4`, `09CC`, `0BA0`) and the DualSense Edge (`0DF2`). On the DualSense the plugin differs from
what the user's controller first accepted from `sony-dualsense` in three ways: no start report, the
light bar setup flag (with `lightbar_setup` 0) in every report, and the player LED fields; the
2026-10-22 test covered the light bar with these reports.
