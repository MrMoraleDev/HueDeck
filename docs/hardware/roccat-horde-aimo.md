<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Roccat Horde Aimo keyboard (`1E7D:303E`)

Plugin: `plugins/roccat-horde-aimo/`. Six lighting zones. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB):

| Source | License | Taken |
|---|---|---|
| `Controllers/RoccatController/RoccatControllerDetect.cpp` | GPL-2.0-or-later | Product id, name, `REGISTER_HID_DETECTOR_IPU(..., 1, 0x0B, 0)` |
| `.../RoccatHordeAimoController/RoccatHordeAimoController.cpp`, `.h` (Morgan Guimard (morg), 24 Feb 2022) | GPL-2.0-or-later | Init report, colour report and checksum |
| `.../RoccatHordeAimoController/RGBController_RoccatHordeAimo.cpp` (Morgan Guimard (morg), 24 Feb 2022) | GPL-2.0-or-later | One linear zone of 6 LEDs, names |

## Protocol

Collection: interface 1, usage page `0x000B`, usage `0x0000`.

- Start: SET_FEATURE `13 08 01 00 00 00 00 00` (8 bytes).
- Frame: SET_FEATURE of 24 bytes: `18 18 FF FF`, `R G B` for LED 1..6, then the 16-bit sum of bytes
  0..21, low byte first. Unchanged frames are resent after 1 s.
- Shutdown: black (OpenRGB has no exit step and no hardware mode for this keyboard, `@effects :x:`).

## Deviation

After each frame OpenRGB calls `hid_get_feature_report` on an uninitialised 3-byte buffer, so the
report id it asks for is undefined; Lightloom does not send that read.

## Safety

OpenRGB marks the device as not saving (`@save :x:`). The plugin sends only the two reports above.

## Untested

`1E7D:303E`.
