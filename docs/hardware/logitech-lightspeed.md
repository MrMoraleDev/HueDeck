<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Logitech Lightspeed mice and headsets over their cable (HID++ 2.0, feature 0x8070), adapted from OpenRGB

Plugin: `plugins/logitech-lightspeed/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (`Controllers/LogitechController/...`) | License | Taken |
|---|---|---|
| `LogitechControllerDetect.cpp` | GPL-2.0-or-later | The `DetectLogitechWired` ids, names and macros; `CreateLogitechLightspeedDevice` (ten tries, 50 ms apart) |
| `LogitechProtocolCommon.cpp`, `.h` (Chris M (Dr_No), 04 May 2021) | GPL-2.0-or-later | `logitech_device`: read-queue flush, feature-index, name and type queries, `getRGBconfig` (0x8070 zones and effects), `is_valid`, `setDirectMode`, `setMode`; the LED location names; the persistence-byte comment |
| `LogitechLightspeedController/LogitechLightspeedController.cpp`, `.h`, `RGBController_LogitechLightspeed.cpp` (TheRogueZeta, 05 Aug 2020) | GPL-2.0-or-later | Zones (one LED each, named by location), "Direct" = LED 0's first `LED_ON` effect, `DeviceUpdateLEDs` |

## Products

| PID | OpenRGB name | Collection (detector) |
|---|---|---|
| `C08D` | Logitech G502 Wireless Gaming Mouse (wired) | any interface, page `FF00`, usage 2 (`_PU`) |
| `C332` | Logitech G502 Proteus Spectrum Gaming Mouse | interface 1, `FF00`, 2 |
| `C08B` | Logitech G502 HERO Gaming Mouse | interface 1, `FF00`, 2 |
| `C083` | Logitech G403 Prodigy Gaming Mouse | interface 1, `FF00`, 2 |
| `C082` | Logitech G403 Wireless Gaming Mouse (wired) | interface 1, `FF00`, 2 |
| `C087` | Logitech G703 Wireless Gaming Mouse (wired) | interface 1, `FF00`, 2 |
| `C090` | Logitech G703 HERO Wireless Gaming Mouse (wired) | any interface, `FF00`, 2 (`_PU`) |
| `C081` | Logitech G900 Wireless Gaming Mouse (wired) | interface 1, `FF00`, 2 |
| `C086` | Logitech G903 Wireless Gaming Mouse (wired) | interface 1, `FF00`, 2 |
| `C091` | Logitech G903 HERO Wireless Gaming Mouse (wired) | interface 2, `FF00`, 2 |
| `C088` | Logitech G Pro Wireless Gaming Mouse (wired) | interface 2, `FF00`, 2 |
| `0A5C` | Logitech G633 Gaming Headset | interface 3, `FF43`, `0202` |
| `0A89` | Logitech G635 Gaming Headset | interface 3, `FF43`, `0202` |
| `0AB5`, `0AFE`, `0B1F` | Logitech G733 Gaming Headset | interface 3, `FF43`, `0202` |
| `0A87` | Logitech G935 Gaming Headset | interface 3, `FF43`, `0202` |

The device's name (HID++ feature 0x0005) becomes the Lightloom device name.

**Not claimed:**
- the LIGHTSPEED receiver `C539` and POWERPLAY `C53A`: OpenRGB finds the paired device by writing
  receiver registers (`SET_REGISTER 0x00`, enabling wireless notifications, and `0x02 = 02`, "fake a
  reconnect"), receiver settings that are not lighting, and waits for answers without a timeout;
- the "virtual" ids (`405D`, `407F`, ...): Linux kernel nodes, not USB ids;
- the G933 headset `0A5B` (own controller): its start report `11 FF 04 CA <zone>` (function 0x0C of
  feature index 4, not explained) is sent with 15 uninitialised bytes;
- OpenRGB's unified HID++ 2.0 controller (any Logitech id).

## Protocol

20-byte HID++ long reports `11 FF <index> <function> <params>` on the validated collection, each
followed by a read (64 bytes, 300 ms). Start (`Initialize`), up to ten attempts 50 ms apart, as in
OpenRGB (which gives the device up when it only becomes valid on the tenth):
1. Flush: read until nothing comes.
2. Root (index 0) function `01` for page `0005`; with that index: function `01` (name length),
   `11 <offset>` until the name is complete (16 characters per answer), `21` (device type).
3. Root function `01` for page `8070`, and if absent `8071`.
4. 0x8070: function `00` (zone count; reads until a 20-byte answer for that index or command),
   per zone `10 <zone>` (zone, location, effect count), per effect `20 <zone> <effect>` (effect id
   in data bytes 2-3).
5. Valid when the name is longer than 3 characters and starts printable, the type is at most 8,
   and there is at least one zone.
6. Direct = the first effect of zone 0 whose id is `0001` (LED_ON); software control:
   `11 FF <8070> 80 01 01` (`setDirectMode(true)`; OpenRGB pairs this function with "disable save to
   flash" in the Lightsync controller, where the same feature is at a fixed index).

Every frame, per zone: `11 FF <8070> 30 <zone> <direct effect> R G B 00 ...` with byte `0x10`
(persistence) `00`, then a read. Lightloom skips unchanged frames and resends after 1 s.

Shutdown: every zone black with the same report. Leaving software control (`80 00 00`) is not sent:
OpenRGB pairs it with saving to flash again.

## Safety notes and deviations

- **Held back at run time: 0x8071-only devices.** OpenRGB sends their colours with byte `0x10` =
  `01`, which its own comment calls the persistence bits, after "disabling all FW control"
  (`0x8071` function `50` with `01 03 05`). Lightloom logs and drives nothing once the queries show
  0x8070 is missing.
- The feature-list dump OpenRGB runs only at debug log level is not sent.
- OpenRGB's name loop asks again forever when an answer carries no characters; Lightloom stops.
- LED locations past the 12 names of `logitech_led_locations[]` are named `LED <n>` (OpenRGB
  compares against `sizeof` of the pointer array, 96, and reads past it).
- No hardware effect, DPI, report rate, onboard profile, power or lighting-timeout write
  (`set8071TimeoutControl` is not used).

## Untested

Every product id.
