<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Corsair peripherals (CUE protocol), K55 RGB PRO XT, K65 Mini: per-LED lighting, adapted from OpenRGB

Plugin: `plugins/corsair-peripheral/`. Untested on hardware (`tested` is empty for every id).

It drives the lights of the older Corsair peripherals that OpenRGB handles with its "Corsair
Peripheral" controllers: the K55 / K65 / K68 / K70 / K95 / Strafe RGB keyboards, the Glaive,
Harpoon, Ironclaw, M65, Nightsword, Scimitar and Sabre mice, the MM800 RGB Polaris mousemat and
the ST100 RGB headset stand, which speak the "CUE" protocol (commands 0x07 / 0x0E / 0x7F), plus
the K55 RGB PRO XT and the K65 Mini, which OpenRGB drives with two small separate controllers in
the same directory (commands 0x08, the protocol of the newer Corsair keyboards). The newer
wireless / iCUE peripherals (OpenRGB's `CorsairPeripheralV2Controller`) are not part of this
plugin.

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file's header says
`SPDX-License-Identifier: GPL-2.0-or-later`.

| File | License | What was taken |
|---|---|---|
| `Controllers/CorsairPeripheralController/CorsairPeripheralControllerDetect.cpp` | GPL-2.0-or-later | Product ids, names, detector macros (interface / usage page), which controller each id gets |
| `Controllers/CorsairPeripheralController/CorsairPeripheralController.cpp`, `.h` (Adam Honse (CalcProgrammer1), 09 Jan 2020) | GPL-2.0-or-later | The CUE protocol: firmware-info request and reply fields, lighting control, special function, the 0x40 key packets and their skip lists, the 24-bit keyboard colour stream and submit, the K55 zone report, the mouse and mousepad reports, the ST100 slot order, the key slot tables (`keys`, `keys_k70_mk2`, `keys_k95_plat`, `keys_k95`); the hardware-mode path was read only to leave it out |
| `Controllers/CorsairPeripheralController/RGBController_CorsairPeripheral.cpp`, `.h` (Adam Honse (CalcProgrammer1), 09 Jan 2020) | GPL-2.0-or-later | Zones, LED counts and names, keyboard matrix maps, mouse LED names, what Direct mode sends (`DeviceUpdateLEDs`, `DeviceUpdateMode`) |
| `Controllers/CorsairPeripheralController/CorsairK55RGBPROXTController.cpp`, `.h` | GPL-2.0-or-later | K55 RGB PRO XT start reports, colour banks and their slot table, the seven colour reports; the hardware-mode path was read only to leave it out |
| `Controllers/CorsairPeripheralController/RGBController_CorsairK55RGBPROXT.cpp`, `.h` | GPL-2.0-or-later | K55 RGB PRO XT key names and matrix map, the 50 s keepalive |
| `Controllers/CorsairPeripheralController/CorsairK65MiniController.cpp`, `.h` | GPL-2.0-or-later | K65 Mini start reports and colour report |
| `Controllers/CorsairPeripheralController/RGBController_CorsairK65Mini.cpp`, `.h` | GPL-2.0-or-later | K65 Mini key names, packet positions and matrix map, the 50 s keepalive |
| `RGBController/RGBControllerKeyNames.cpp`, `.h` (Chris M (Dr_No), 25 Jan 2022) | GPL-2.0-or-later | The key name strings (`KEY_EN_*`), used as LED names |
| `DetectionManager.h` | GPL-2.0-or-later | What `REGISTER_HID_DETECTOR_I` / `_IP` / `_P` match. Read only |

`plugin.js` carries the credit header; `devices.js` (product table, layouts) is generated from
the C++ tables by a script in the porting session's scratch space.

## Products

39 ids, vendor `0x1B1C`. "LEDs" is OpenRGB's LED count (the number of colours the plugin
samples and sends). CUE devices are told apart by the firmware-info reply (keyboard `0xC0`,
mouse `0xC1`, mousepad / headset stand `0xC2`); the "Protocol, layout" column is what the
product is expected to answer.

| Id | OpenRGB name | Protocol, layout | LEDs | Detector (`Validate`) |
|---|---|---|---|---|
| `1B1C:1B3D` | Corsair K55 RGB | CUE keyboard, K55 | 3 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B17` | Corsair K65 RGB | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B37` | Corsair K65 LUX RGB | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B39` | Corsair K65 RGB RAPIDFIRE | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B4F` | Corsair K68 RGB | CUE keyboard, K68 | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B3F` | Corsair K68 RED | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1BA5` | Corsair K68 RED SHADOW | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B13` | Corsair K70 RGB | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B36` | Corsair K70 LUX | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B33` | Corsair K70 LUX RGB | CUE keyboard, K70_LUX_RGB | 116 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B38` | Corsair K70 RGB RAPIDFIRE | CUE keyboard, K70_MK2 | 116 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B49` | Corsair K70 RGB MK.2 | CUE keyboard, K70_MK2 | 116 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B6B` | Corsair K70 RGB MK.2 SE | CUE keyboard, K70_MK2 | 116 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B55` | Corsair K70 RGB MK.2 Low Profile | CUE keyboard, K70_MK2 | 116 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1BC6` | Corsair K70 RGB PRO Optomechanical | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B11` | Corsair K95 RGB | CUE keyboard, K95 | 135 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B2D` | Corsair K95 RGB PLATINUM | CUE keyboard, K95_PLAT | 139 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B82` | Corsair K95 RGB PLATINUM SE | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B20` | Corsair Strafe | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B44` | Corsair Strafe Red | CUE keyboard, NORMAL | 113 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B48` | Corsair Strafe MK.2 | CUE keyboard, K70_MK2 | 116 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B34` | Corsair Glaive RGB | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B74` | Corsair Glaive RGB PRO | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B3C` | Corsair Harpoon RGB | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B75` | Corsair Harpoon RGB PRO | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B5D` | Corsair Ironclaw RGB | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B12` | Corsair M65 | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B2E` | Corsair M65 PRO | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B5A` | Corsair M65 RGB Elite | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B5C` | Corsair Nightsword | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B1E` | Corsair Scimitar RGB | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B3E` | Corsair Scimitar PRO RGB | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B8B` | Corsair Scimitar Elite RGB | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B2F` | Corsair Sabre RGB | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B32` | Corsair Sabre RGB Optical | CUE mouse | 15 | `_IP`: interface 1, usage page `0xFFC2` |
| `1B1C:1B3B` | Corsair MM800 RGB Polaris | CUE mousemat | 15 | `_P`: usage page `0xFFC2` (any interface) |
| `1B1C:0A34` | Corsair ST100 RGB | CUE stand | 9 | `_P`: usage page `0xFFC2` (any interface) |
| `1B1C:1BAF` | Corsair K65 Mini | K65 Mini (0x08 protocol) | 64 | `_I`: interface 1 |
| `1B1C:1BA1` | Corsair K55 RGB PRO XT | K55 RGB PRO XT (0x08 protocol) | 111 | `_IP`: interface 1, usage page `0xFF42` |

"Layout" is OpenRGB's logical keyboard layout (`CORSAIR_TYPE_*`), picked by product id:
K95 PLATINUM `1B2D` = K95_PLAT, K95 RGB `1B11` = K95, K55 RGB `1B3D` = K55, K70 LUX RGB `1B33` =
K70_LUX_RGB, `1B38` / `1B49` / `1B6B` / `1B55` / `1B48` = K70_MK2, K68 RGB `1B4F` = K68, every
other keyboard NORMAL (the K95 RGB PLATINUM SE `1B82` too: OpenRGB has no case for it). The K68
uses the NORMAL tables and differs only in its start. The non-RGB K68 RED, K68 RED SHADOW and
K70 LUX are registered by OpenRGB and driven like the RGB boards (their LEDs show only red).

## LED grid

LED order is OpenRGB's, which is also the order colours go into the packets. Keyboards use
OpenRGB's matrix maps as the grid. HueDeck additions so that every LED has a cell: on NORMAL
(23 x 7) the 6-row map moves down one row and Brightness, Windows Lock and Media Mute (not in
OpenRGB's map) go on the new top row where OpenRGB's K70 LUX RGB map has them (columns 16, 17,
20); on K70_LUX_RGB, Profile, Logo Left and Logo Right (not in its map) go where the K70 MK.2 map
has them (row 0, columns 3, 11, 12); on K95_PLAT (24 x 8) the 19-LED "Light Bar" zone is a row of
its own above the keys (columns 2 to 20). K55 RGB: Left, Middle, Right in a row. Mice: the 15
LEDs OpenRGB sends, in a row; their names are what OpenRGB's `SetupZones` ends with (its if /
else-if chain leaves only the Ironclaw RGB and the Sabre RGB with their own lists), with OpenRGB's
empty names given the generic "Mouse LED n". MM800: 15 LEDs in a row. ST100: the 8-LED base strip
on the bottom row, the logo (OpenRGB's 9th LED) above its middle. K55 RGB PRO XT (24 x 6) and K65
Mini (15 x 5): OpenRGB's matrix maps; the K65 Mini's two unnamed space-bar LEDs are called
"Key: Space (left LED)" and "Key: Space (right LED)".

## Protocol: CUE devices

Every report is a 65-byte output report (`hid_write`): report id `0x00`, then 64 bytes. Offsets
below are in the 65-byte buffer.

**Start** (OpenRGB's controller constructor; it runs when OpenRGB detects the device, and Direct
is the default mode):

1. Firmware info, read-only, up to 10 tries: write `00 0E 01`, read an input report (1 s
   timeout). When nothing comes, send the same request as a feature report and read the feature
   report (report id `0x00`) instead; the reply fields are then one byte further. The type byte
   (`[0x14]`, or `[0x15]` from the feature report) ends the tries when it is `C0` keyboard, `C1`
   mouse or `C2` mousepad / headset stand. On a keyboard `[0x17]` (`[0x18]`) is the physical
   layout (0 ANSI, 1 ISO, 2 ABNT, 3 JIS, 4 Dubeolsik; anything else ANSI). With no valid type
   after 10 tries OpenRGB drops the device; so does HueDeck (nothing else is sent). HueDeck
   returns the report id first from `device.read`; the plugin strips it to get hidapi's buffer.
   On a device that never answers, this takes about 10 s.
2. Mouse, mousepad, stand: special function `00 07 04 02` (inside OpenRGB's firmware-info step).
3. K55, K95_PLAT, K70_MK2 and K68 keyboards: special function `00 07 04 02`.
4. Lighting control, software: `00 07 05 02 00 xx`, `xx` = 03 keyboard and headset stand, 01
   mouse, 04 mousepad (OpenRGB: "On K95 Platinum, this controls keyboard brightness").
5. K55, K95_PLAT, K70_MK2 and K68 keyboards: `00 07 05 08 00 01`, then four `00 07 40 1E 00`
   reports of 30 (identifier, `C0`) pairs: identifiers 0, 1, 2, ... skipping those of
   `key_mapping_k95_plat_ansi` (ANSI and others), `key_mapping_k95_plat_iso` (ISO) or
   `key_mapping_k70_mk2_plat_iso` (ISO, K70_MK2 / K70_LUX_RGB layouts), with OpenRGB's one-pass
   skip.

**Every frame** (`SetLEDs`):

- Keyboards (not K55): three 168-byte planes (red, green, blue), LED i at slot `keys[i]` of the
  layout's table (`keys`; `keys_k70_mk2` for K70_MK2 and K70_LUX_RGB; `keys_k95_plat`; `keys_k95`).
  Per plane: `00 7F 01 3C 00` + plane[0..59], `00 7F 02 3C 00` + plane[60..119],
  `00 7F 03 nn 00` + plane[120..] with nn = 0x18 (0x30 on K95 / K95_PLAT), then the submit
  `00 07 28 cc 03 ff` (cc = 1, 2, 3; ff = 1, 1, 2). Twelve reports.
- K55 RGB: `00 07 25 00 00 R G B R G B R G B` (left, middle, right).
- Mice: `00 07 22 0F 00`, then per LED `<n> R G B` (15 LEDs, n = 0..14).
- MM800: `00 07 22 0F 00`, then `R G B` per LED.
- ST100: the mousepad report with 9 LEDs, OpenRGB LED i in slot `st100[i]` = 0, 1, 2, 3, 5, 6, 7,
  8, 4 (the logo is slot 4, in the middle of the strip).

## Protocol: K55 RGB PRO XT and K65 Mini

**K55 RGB PRO XT** (65-byte output reports, report id `0x00`). Start: `00 08 01 03 00 02`,
`00 08 02 5F`, `00 08 0D 00 01`. Every frame: three 137-byte colour banks (red, green, blue; LED
i in slot `keys[i]`; the banks keep their bytes between frames as OpenRGB's static banks do),
sent as one report `00 08 06 00 9B 01 00 .. 00` with bank bytes 0..52 at offset 12, then six
`00 08 07 00` reports with 61 bytes each. 53 + 6 x 61 = 419 bytes, but the banks are 411 bytes:
OpenRGB's last report reads 8 bytes past its array (undefined in C++); HueDeck sends zeros there.

**K65 Mini** (1025-byte output reports, report id `0x00`). Start: `00 08 01 03 00 02`,
`00 08 02 6E`, `00 08 0D 01 22`. Every frame: one `00 08 06 01 73 01 00 00 12 00` report with
each LED's R G B at `0x0A + 3 x position` (positions from OpenRGB's key table, up to 122).

Both keyboards go back to their own (rainbow) effect when no colour report comes for about a
minute; OpenRGB resends after 50 s. HueDeck resends unchanged colours every second.

## Shutdown

OpenRGB has no exit step. Its only way to hand a CUE keyboard back to a hardware effect is its
hardware-mode path (below), which stores the effect in the keyboard; nothing else in OpenRGB
selects the device's own lighting. So `Shutdown()` switches every LED off (black, through the
frame path above) on all products. The K55 RGB PRO XT and the K65 Mini then return to their own
effect by themselves about a minute later. A power cycle (unplugging, or a reboot that cuts USB
power) brings every device back to its stored lighting.

## Left out (safety)

- **Hardware modes** (`SetHardwareMode`, OpenRGB lists them only for the K70 RGB MK.2 and MK.2
  Low Profile, with `MODE_FLAG_AUTOMATIC_SAVE`): `07 05 02 00 <brightness>`, a "lght_00.d" file
  name (`07 17 05 ...`), the effect streamed with `7F 01 0D`, then `07 17 09` and `07 17 08`
  ("Stop stream and commit") and `SwitchMode(false)` = `07 04 01`. That writes an effect file
  into the keyboard; never sent.
- **K55 RGB PRO XT hardware modes**: `08 0D 01 61 6D`, `08 09 01`, `08 06 01 78 ...` with the
  effect and the `filler` table, two `08 07 01` reports, `08 05 01 01`, then `00 00 01 03 00 01`
  (`SwitchMode(false)`, whose command byte is 0x00, not explained): a write of an effect into the
  keyboard's memory; never sent.
- `SetLEDsKeyboardLimited` (9-bit colours, `07 27`): unused by OpenRGB itself; not ported.
- Properties OpenRGB defines but does not send (`02` reset, `13` hardware profile): never sent.
- No DPI, polling rate, macro, key remap or profile command exists in the plugin.

## Deviation from OpenRGB

- **Keyboard layout from the USB product id.** OpenRGB reads the product id from bytes 0x0E-0x0F
  of the firmware-info reply, big-endian for keyboards (`[0x0E] << 8 | [0x0F]`) but little-endian
  for the mousepad / stand check (`[0x0F] << 8 | [0x0E]`), and without the feature-report offset.
  One of the two byte orders must be wrong for some reply. HueDeck uses the same pid-to-layout
  table on the product id the device enumerated with (and `0A34` for the ST100), which is what the
  table means. Everything else of the reply is read exactly as OpenRGB does.
- LED names: OpenRGB's empty names (mice, K65 Mini space bar) get names (above).
- Unchanged frames are not resent within a second (OpenRGB sends on every change).
- Each report is handed to the host as a copy, so buffers OpenRGB reuses between writes
  (K55 RGB PRO XT colour reports) go out exactly as the C++ sends them.

## Open questions

- **The 0x40 key packets and 07 04 02.** On the K55, K95_PLAT, K70_MK2 and K68 keyboards OpenRGB
  sends special function `07 04 02` and four `07 40 1E` reports of (key identifier, `C0`) pairs
  ("Probably a key mapping packet?" in its source). In other Corsair tools this is the per-key
  input mode (`C0` = report both the standard HID key and Corsair's own input), which goes with
  software control of the special keys (brightness, Windows lock, media, G / M keys). It is not a
  lighting setting, but it is part of OpenRGB's known-working switch to software lighting on these
  keyboards and is not stored (no commit), so HueDeck sends it unchanged. Shutdown does not undo it
  (OpenRGB neither); a power cycle does. Check on hardware that typing and the special keys keep
  working while and after HueDeck runs.
- Whether `07 04 01` alone (without the effect file write) hands a CUE device back to its stored
  lighting: if so, it would be a better shutdown than black. Not done without a hardware test.

## Untested

Every product id (39). Nothing here was run on hardware; the packets are checked against the
C++ only (`fixtures/expected.json`).
