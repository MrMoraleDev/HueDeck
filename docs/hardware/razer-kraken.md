<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Razer Kraken 7.1 and Kraken V3 headsets (`1532`), adapted from OpenRGB

The plugin is `plugins/razer-kraken/`. It drives the headset lights of the products OpenRGB
handles with `DetectRazerKrakenControllers` (Kraken 7.1 family) and
`DetectRazerKrakenV3Controllers` (Kraken V3 family). Their reports have nothing in common with
the 90-byte Razer report of `plugins/razer/` (see `docs/hardware/razer.md`), hence a plugin of
their own. Untested on hardware (`tested` is empty in `manifest.json`).

## Sources

| # | Source | License | Used for |
|---|--------|---------|----------|
| S1 | OpenRGB `Controllers/RazerController/RazerControllerDetect.cpp` | GPL-2.0-or-later | Detector lines (all `IPU` 3, `000C`, `0001`) |
| S2 | OpenRGB `Controllers/RazerController/RazerDevices.h`, `RazerDevices.cpp` | GPL-2.0-or-later | Product ids, table entries (zones; `cols` = LED count of the V3 frame) |
| S3 | OpenRGB `Controllers/RazerController/RazerKrakenController/RazerKrakenController.cpp`, `.h`, `RGBController_RazerKraken.cpp`, `.h` | GPL-2.0-or-later | Kraken 7.1 reports, memory addresses per product, effect byte, Direct = `SetModeCustom(colors[0])` |
| S4 | OpenRGB `Controllers/RazerController/RazerKrakenV3Controller/RazerKrakenV3Controller.cpp`, `.h`, `RGBController_RazerKrakenV3.cpp`, `.h` | GPL-2.0-or-later | Kraken V3 reports (mode, brightness, colours), Direct and Wave modes |

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB); every file carries
`SPDX-License-Identifier: GPL-2.0-or-later`. Author lines kept in `plugin.js`:
`Adam Honse (CalcProgrammer1)  28 Feb 2021` (RazerKrakenController, RGBController_RazerKraken),
`Greg Sandstrom (superstrom)  1 Nov 2025` (RazerKrakenV3Controller, RGBController_RazerKrakenV3),
`Adam Honse (CalcProgrammer1)  22 Jan 2021` (RazerControllerDetect.cpp),
`Adam Honse (CalcProgrammer1)  04 Sep 2021` (RazerDevices). `devices.js` is generated from S1, S2
and the address switch of S3 by the same scripts as `plugins/razer/devices.js` (see
`docs/hardware/razer.md`, "How the tables are made"); the protocol is hand-written.

## Transport

Interface 3, collection usage page `0x000C`, usage `0x0001`; HID **output** reports
(`hid_write` = `device.write`).

## Kraken 7.1 family (S3)

37-byte report `razer_kraken_request_report`: report id `0x04`, destination, length, address
(big endian), 32 argument bytes. OpenRGB writes with destination `0x40`.

| Product | LED mode address | Colour address |
|---------|------------------|----------------|
| Kraken 7.1 `0501`, `0506`, Kraken 7.1 Chroma `0504` | `0x1008` | `0x1189` |
| Kraken 7.1 V2 `0510`, Kitty Black Edition V2 `0560`, Ultimate `0527` | `0x172D` | `0x1189` |

- Start: nothing (switching to Direct sends nothing on this controller).
- Frame (`SetModeCustom(colors[0])`): on `0504`, `0510`, `0560`, `0527` the colour,
  `04 40 03 11 89 r g b`; then on every product the effect byte "on" (`0x01`),
  `04 40 01 <mode address> 01`. Only the first LED's colour reaches the wire, so HueDeck offers
  one LED, "Headset" (OpenRGB lists the table's zones; `0501` and `0506` have no table entry and
  OpenRGB falls back to the first table entry, a keyboard). Unchanged frames are sent again after
  1 s.
- Exit: OpenRGB has none; HueDeck sets the spectrum cycle, effect byte `0x05` (on + spectrum
  cycling), with OpenRGB's `razer_set_mode_spectrum_cycle()`.

Effect byte bits (`razer_kraken_effect_byte`, first field = bit 0): on/static `0x01`, single
colour breathing `0x02`, spectrum cycling `0x04`, sync `0x08`, two colour breathing `0x10`,
three colour breathing `0x20`.

## Kraken V3 family (S4)

15-byte report `razer_kraken_v3_request_report`: report id `0x40`, command id, 13 argument bytes.

- Start: the headset starts in its wave effect; switching to Direct sends mode `40 01 00 0F 08`
  and brightness `40 02 00 00 FF` (the maximum; HueDeck's colours carry the brightness).
- Frame: `40 03 00 r g b ...` with the `cols` LEDs of the table entry (1; 4 on the Kitty V2 Pro:
  left and right cat ear, left and right logo).
- Exit: HueDeck gives the headset its wave back, `40 01 00 0F 03` and `40 02 00 00 FF`.

## Check against OpenRGB

The fixture packets (`plugins/razer-kraken/fixtures/openrgb-packets.json`, a Python transcription of
the C++ functions) were compared with OpenRGB's own controller code compiled outside the
repository (g++, stub headers for hidapi and the logger): the five test products (Kraken 7.1, 7.1 Chroma, 7.1 V2, V3, Kitty V2 Pro; 18 reports) are equal byte for byte.

## Left out, and why

- Firmware and serial queries (`04 20 ...` with a read of report `0x05`): they only fill
  OpenRGB's info page.
- Hardware effects other than the exit's (static, breathing): HueDeck drives the colour per frame.

## Untested / open

- Whether destination `0x40` writes RAM only. OpenRGB writes the same addresses on every colour
  change, but nothing in OpenRGB documents the destination byte. If it were EEPROM, a running
  effect would wear it; to limit that, the plugin writes changed Kraken 7.1 colours at most every
  250 ms (unchanged colours are resent once a second), instead of up to 30 times a second.
- The Kraken 7.1 (`0501`, `0506`) gets no colour report in OpenRGB; its frames only switch the
  light on, and the exit's spectrum bit may do nothing on a single-colour headset.
- The V3 report is 15 bytes (`sizeof` of OpenRGB's struct) although OpenRGB's file header speaks
  of 13-byte reports; the port sends 15 like OpenRGB.

## Products

| PID | OpenRGB name (detector line) | Detector | Protocol | Matrix, tid | Grid | Zones | Notes |
|-----|------------------------------|----------|----------|-------------|------|-------|-------|
| `0501` | Razer Kraken 7.1 | IPU I 3, P `000C`, U `0001` | kraken | (no table entry) | - | Headset | OpenRGB falls back to device_list[0] |
| `0506` | Razer Kraken 7.1 | IPU I 3, P `000C`, U `0001` | kraken | (no table entry) | - | Headset | OpenRGB falls back to device_list[0] |
| `0504` | Razer Kraken 7.1 Chroma | IPU I 3, P `000C`, U `0001` | kraken | none, `00` | 1x1 | Headset |  |
| `0510` | Razer Kraken 7.1 V2 | IPU I 3, P `000C`, U `0001` | kraken | none, `00` | 1x1 | Headset |  |
| `0560` | Razer Kraken Kitty Black Edition V2 | IPU I 3, P `000C`, U `0001` | kraken | extended, `3F` | 1x4 | Headset Left; Headset Right; Cat ears 1x2 |  |
| `0527` | Razer Kraken Ultimate | IPU I 3, P `000C`, U `0001` | kraken | none, `00` | 1x1 | Headset |  |
| `0533` | Razer Kraken V3 HyperSense | IPU I 3, P `000C`, U `0001` | kraken-v3 | none, `00` | 1x1 | Headset |  |
| `0537` | Razer Kraken V3 X | IPU I 3, P `000C`, U `0001` | kraken-v3 | none, `00` | 1x1 | Headset |  |
| `0549` | Razer Kraken V3 | IPU I 3, P `000C`, U `0001` | kraken-v3 | none, `00` | 1x1 | Headset |  |
| `0554` | Razer Kraken Kitty V2 Pro | IPU I 3, P `000C`, U `0001` | kraken-v3 | none, `00` | 1x4 | Left Cat Ear; Right Cat Ear; Left Logo; Right Logo |  |
