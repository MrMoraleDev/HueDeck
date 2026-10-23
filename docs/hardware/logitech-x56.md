<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Logitech (Saitek) X56 Rhino HOTAS, adapted from OpenRGB

Plugin: `plugins/logitech-x56/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (`Controllers/LogitechController/...`) | License | Taken |
|---|---|---|
| `LogitechControllerDetect.cpp` | GPL-2.0-or-later | Vendor `0738`, ids, names, detector macros |
| `LogitechX56Controller/LogitechX56Controller.cpp`, `.h` (Edbgon, 11 Jun 2021) | GPL-2.0-or-later | `SetColor` (two 64-byte feature reports); the left-out `Save` |
| `LogitechX56Controller/RGBController_LogitechX56.cpp` (Edbgon) | GPL-2.0-or-later | One LED, Direct brightness default `0x64`, `MODE_FLAG_MANUAL_SAVE` |

## Products

| ID | OpenRGB name | Collections |
|---|---|---|
| `0738:2221` | Logitech X56 Rhino Hotas Joystick | interface 2, page `FF00`, usage 1 or 3 |
| `0738:A221` | Logitech X56 Rhino Hotas Throttle | same |

OpenRGB registers both usages, so it creates a controller for each collection; HueDeck drives the
first one its host validates.

## Protocol

Per colour, feature reports of 64 bytes: `09 00 02 64 ...` (brightness, kept at Direct's default
maximum because HueDeck applies its own brightness to the colour) then `09 00 03 R G B ...`.
Initialize sends nothing (OpenRGB's Direct only updates the colour); unchanged colours are resent
after 1 s. Shutdown: black (OpenRGB has no exit step and no hardware effect for the X56).

## Safety notes

`Save()` (`01 01 ...`, OpenRGB's manual save) is never sent.

## Untested

Both ids.
