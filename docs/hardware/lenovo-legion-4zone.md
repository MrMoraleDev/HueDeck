<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Lenovo Legion 5 / IdeaPad Gaming 4-zone keyboard (`048D`, usage `FF89:CC`)

Plugin: `plugins/lenovo-legion-4zone/`. It has not been tested on hardware (`tested` is empty).
It drives the four-zone keyboard backlight of Lenovo Legion 5 laptops (2020 to 2023) and IdeaPad
Gaming laptops. That backlight sits behind the laptop's ITE USB controller.

## Sources

All files are from OpenRGB at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every one carries
`SPDX-License-Identifier: GPL-2.0-or-later` and has no author line.

| File (under `Controllers/LenovoControllers/Lenovo4ZoneUSBController/`) | What was taken |
|---|---|
| `Lenovo4ZoneUSBControllerDetect.cpp` | Product ids and names; detector `REGISTER_HID_DETECTOR_PU(..., 0xFF89, 0xCC)` |
| `LenovoDevices4Zone.h` | Product id defines, effect, speed and brightness values, `KeyboardState` (packet layout and defaults), LED names |
| `Lenovo4ZoneUSBController.cpp`, `.h` | `setMode`: the 33-byte feature report |
| `RGBController_Lenovo4ZoneUSB.cpp`, `.h` | Modes (Direct, Breathing, Rainbow Wave, Spectrum Cycle), one linear zone of 4 LEDs, `DeviceUpdateLEDs` and `DeviceUpdateMode` |

## Products

| PID | OpenRGB name |
|---|---|
| `C963` | Lenovo Ideapad 3-15ach6 (also registered as "Lenovo 5 2021 Ideapad": the same id) |
| `C985` | Lenovo 5 2023 |
| `C984` | Lenovo 5 2023 Ideapad |
| `C975` | Lenovo 5 2022 |
| `C973` | Lenovo 5 2022 Ideapad |
| `C965` | Lenovo 5 2021 (OpenRGB: "Tested on Lenovo Legion 5 2021") |
| `C955` | Lenovo 5 2020 |

That makes 7 product ids. `Validate` accepts usage page `0xFF89`, usage `0xCC`, on any interface.
The laptop's other ITE collection (usage `0x07`) belongs to `lenovo-legion-usb` on other models.

## Protocol

There is one feature report: report id `0xCC`, 33 bytes.

```
CC 16 <effect> <speed> <brightness> R0 G0 B0 R1 G1 B1 R2 G2 B2 R3 G3 B3 00 <wave ltr> <wave rtl> 00 ... 00
```

The zones are, in order: Left side, Left center, Right center, Right side. HueDeck lays them out
as a 4x1 grid.

- **Initialize.** OpenRGB's controller sends nothing when it is created, so neither does HueDeck.
- **Frame** (`DeviceUpdateLEDs`). The effect is static (`1`) and the speed is slowest (`1`, the
  `KeyboardState` default). The bytes then hold the brightness step and the four colours. HueDeck
  skips a frame whose colours did not change and resends it after 1 s.
- **Brightness.** The keyboard has two steps: `1` Low and `2` High. OpenRGB's Direct brightness
  range is 1..2. Its frame without a mode update uses `KeyboardState`'s default, Low. HueDeck
  offers the step as a parameter, "Backlight Level". It defaults to High, so the canvas colours
  show at full range. Low sends exactly OpenRGB's default frame.
- **Shutdown.** OpenRGB has no exit step. This protocol has no save command: modes are chosen by
  the same report. HueDeck hands the keyboard back to its **Spectrum Cycle** hardware effect, as
  `DeviceUpdateMode` would send it: `KeyboardState` reset, effect `6`, speed Slow (`2`), the
  chosen brightness step, and the zone colours of the last frame (this effect ignores them).
  OpenRGB leaves that mode's speed at 0, which is outside its own 1..4 range, so HueDeck picks
  Slow.

## Safety

- The plugin sends only `CC 16` reports, and a test checks that.
- There is no save command, and nothing else is written.
- Whether the laptop's embedded controller keeps the last state across a power cycle is not
  known. OpenRGB also sends this report on every update.

## Untested

Every product id. These questions need the hardware:
- whether High/Low matches the laptop's own Fn+Space steps;
- what the keyboard shows after a reboot following a HueDeck session.
