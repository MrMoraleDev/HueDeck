<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Razer Hanbo Chroma (`1532:0F35`), adapted from OpenRGB

The plugin is `plugins/razer-hanbo/`. It drives the lights of the Razer Hanbo Chroma liquid
cooler (pump cap: 16 LEDs; three fans: 18 LEDs each), the product OpenRGB handles with
`DetectRazerHanboControllers`. Its reports have nothing in common with the 90-byte Razer report
of `plugins/razer/`, hence a plugin of its own. Pump and fan speeds are never touched. Untested on
hardware (`tested` is empty in `manifest.json`).

## Sources

| # | Source | License | Used for |
|---|--------|---------|----------|
| S1 | OpenRGB `Controllers/RazerController/RazerControllerDetect.cpp` | GPL-2.0-or-later | Detector line (`IPU` 0, `FF00`, `0001`) |
| S2 | OpenRGB `Controllers/RazerController/RazerDevices.h`, `RazerDevices.cpp` | GPL-2.0-or-later | Product id, zones (Pump 1x16, Fan 1..3 1x18) |
| S3 | OpenRGB `Controllers/RazerController/RazerHanboController/RazerHanboController.cpp`, `.h`, `RGBController_RazerHanbo.cpp`, `.h` | GPL-2.0-or-later | Every report, the direct-mode sequence, zone order, G/R/B colour order |

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB); every file carries
`SPDX-License-Identifier: GPL-2.0-or-later`. Author lines kept in `plugin.js`:
`Joseph East (dripsnek)  12 Apr 2025` (RazerHanboController, RGBController_RazerHanbo),
`Adam Honse (CalcProgrammer1)  22 Jan 2021` (RazerControllerDetect.cpp),
`Adam Honse (CalcProgrammer1)  04 Sep 2021` (RazerDevices). `devices.js` is generated (see
`docs/hardware/razer.md`, "How the tables are made"); the protocol is hand-written.

## Protocol (S3)

Interface 0, collection usage page `0xFF00`, usage `0x0001`. The cooler has no report ids:
65-byte output reports `razer_hanbo_report`, byte 0 the dummy report id `0x00` (not on the
wire), byte 1 the command, 63 argument bytes. Answers are read with a 2 ms timeout and not
checked.

- Start (`RazerHanboController` constructor, `SetDirectMode()`), each followed by a read:
  `82 01`, `80 01 00 01`, `80 01 01 01`, `82 01`.
- Frame (`DeviceUpdateLEDs()`: pump, fan 1, fan 2, fan 3): command `0x32` for the pump, `0x40`
  for a fan; arguments `01 07 00 00 00 00 <zone 0..3>` (OpenRGB builds them from the hex string
  `"0107000000000" + zone`), then per LED **G, R, B**; no answer, 2 ms pause after each.
  Unchanged frames are sent again after 1 s.
- Exit: OpenRGB has none; Lightloom sets the cooler's spectrum cycle with OpenRGB's
  `SetModeSpectrumCycle()`: `82 01`, `80 01`, `80 01 01`, each followed by a read.

## Check against OpenRGB

The fixture packets (`plugins/razer-hanbo/fixtures/openrgb-packets.json`, a Python transcription of
the C++ functions) were compared with OpenRGB's own controller code compiled outside the
repository (g++, stub headers for hidapi and the logger): the Hanbo's 11 reports (the constructor's firmware request excluded) are equal byte for byte.

## Left out, and why

- `GetFirmware()` (`01 01`, two reads): it only fills OpenRGB's version and serial fields.
- `SetBrightness()` (`70 01 00/01 b`): OpenRGB never calls it (Direct has no brightness flag);
  Lightloom's colours carry the brightness.

## Untested / open

- The meaning of commands `0x80` / `0x82` is not documented in OpenRGB beyond "direct mode" and
  "spectrum cycle"; they are sent exactly as OpenRGB sends them.

## Products

| PID | OpenRGB name (detector line) | Detector | Protocol | Matrix, tid | Grid | Zones | Notes |
|-----|------------------------------|----------|----------|-------------|------|-------|-------|
| `0F35` | Razer Hanbo Chroma | IPU I 0, P `FF00`, U `0001` | hanbo | extended, `3F` | 1x70 | Pump 1x16; Fan 1 1x18; Fan 2 1x18; Fan 3 1x18 |  |
