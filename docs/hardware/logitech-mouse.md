<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Logitech G mice with fixed feature indices (G203, G Pro, G303, G403 HERO, G203 LIGHTSYNC, G600), adapted from OpenRGB

Plugin: `plugins/logitech-mouse/`. Untested on hardware (`tested` is empty).

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (https://gitlab.com/CalcProgrammer1/OpenRGB).

| File (`Controllers/LogitechController/...`) | License | Taken |
|---|---|---|
| `LogitechControllerDetect.cpp` | GPL-2.0-or-later | Ids, names, detector macros, the feature index (`0x0E`) and function bytes (`0x3A` / `0x3C`) per mouse, the two-collection opening of the Lightsync mice |
| `LogitechGLightsyncController/LogitechGLightsyncController.cpp`, `.h` (TheRogueZeta, 21 Apr 2021) | GPL-2.0-or-later | `SetDirectMode` (short report, "disable save to flash"), `UpdateMouseLED` |
| `LogitechGLightsyncController/RGBController_LogitechGLightsync.cpp`, `RGBController_LogitechGLightsync1zone.cpp` | GPL-2.0-or-later | Zones (DPI = 0, Logo = 1; one-zone: Logo = 0), Direct sent as Static |
| `LogitechG203LController/LogitechG203LController.cpp`, `.h`, `RGBController_LogitechG203L.cpp` (Eric Samuelson (edbgon), 06 Oct 2020) | GPL-2.0-or-later | Software-control report, `SetDevice` + `SendApply`, the doubled update, 10 ms reads |
| `LogitechG600Controller/LogitechG600Controller.cpp`, `.h`, `RGBController_LogitechG600.cpp` (Austin B (austinleroy), 11 Sep 2025) | GPL-2.0-or-later | Feature report `F1` |

## Products

| PID | OpenRGB name | Kind | Collections (detector) | LEDs |
|---|---|---|---|---|
| `C084` | Logitech G203 Prodigy | Lightsync 1 zone, function `3A` | interface 1, page `FF00`, usages 1 and 2 (`_IP`) | Logo |
| `C092`, `C09D` | Logitech G203 Lightsync | G203L | interface 1, page `FF00`, usage 2 (`_IPU`) | Mouse Left, Center, Right |
| `C080` | Logitech G303 Daedalus Apex | Lightsync 2 zones, `3A` | as C084 | DPI, Logo |
| `C08F` | Logitech G403 HERO | Lightsync 2 zones, `3A` | as C084 | DPI, Logo |
| `C24A` | Logitech G600 Gaming Mouse | G600 | interface 1, page `FF80`, any usage (`_IP`) | All |
| `C085` | Logitech G Pro Gaming Mouse | Lightsync 1 zone, `3C` | as C084 | Logo |
| `C08C` | Logitech G Pro HERO Gaming Mouse | Lightsync 1 zone, `3C` | as C084 | Logo |

## Protocol

**Lightsync** (OpenRGB holds two handles: the usage-1 collection for 7-byte short reports `10`,
the usage-2 collection for 20-byte long reports `11`; HueDeck's host opens one collection at a time,
so the plugin switches with `set_endpoint`):
- Start (`SetDirectMode(true)`): short report `10 FF 0E 8A 01 01 00` on usage 1 (OpenRGB: "If direct,
  disable save to flash"), then the answer is read on usage 2, which stays active.
- Every frame, per zone: `11 FF 0E <3A|3C> <zone> 01 R G B 02 00 ...` (20 bytes; mode Static `01`
  with its `02` parameter, as OpenRGB sends Direct), then a read of the answer. OpenRGB waits
  without a timeout; HueDeck 300 ms.

**G203 LIGHTSYNC** (one collection):
- Start: `11 FF 0E 50 01 03 07` (constructor, "enable software control"), then `SetDevice` with the
  still-black colours once (Direct's `DeviceUpdateMode`).
- Every frame, twice ("dirty workaround for color lag"): `11 FF 12 10 01 R G B 02 R G B 03 R G B FF`
  and the apply report `11 FF 12 70`. Each report is followed by a 10 ms read. OpenRGB reopens the
  device when a write or read fails; HueDeck's host owns the handle.

**G600**: feature report `F1 R G B 00 00 00 00` (mode 0 = Direct, speed 0), 8 bytes.

HueDeck skips a frame whose colours did not change, resending them after 1 s.

**Shutdown**: every LED black with the frame reports. OpenRGB has no exit step. The Lightsync
"leave direct" report (`SetDirectMode(false)`, `10 FF 0E 8A 00 00`) re-enables saving to flash,
so it is not sent: the mice stay in software control (not saved) until they are unplugged or
other software takes over, and come back with their stored effect.

## Safety notes

- Lightsync: colours are sent only after the "disable save to flash" report, as OpenRGB does.
- Never sent: OpenRGB's hardware modes (Cycle, Breathing, Wave, Colormixing); the G203 LIGHTSYNC's
  `SetMode` ends with byte `0x10` = `01`, the persistence byte of the RGB-effects feature.
  No DPI, report rate, button or onboard-profile command exists in these drivers.
- G600: OpenRGB marks it `@save :o:` (no saving); `F1` is the only report sent.

## Untested

Every product id.
