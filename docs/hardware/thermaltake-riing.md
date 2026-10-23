<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Thermaltake Riing, Riing Quad and Riing Trio controllers (`264A`)

Plugin: `plugins/thermaltake-riing/`. Untested on hardware (`tested` is empty). It drives the
LEDs on the five fan headers of Thermaltake's Riing USB controllers per LED. It never sets fan
speeds and never saves anything to the controller.

The three controller generations use different packets, but they share the device class (five
ARGB headers with a user-set LED count), the init report and the report header. So they are one
plugin with three variants, picked by product id, and not three plugins.

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file listed carries
`SPDX-License-Identifier: GPL-2.0-or-later`.

| File (under `Controllers/ThermaltakeRiingController/`) | Authors (as in the header) | What was taken |
|---|---|---|
| `ThermaltakeRiingControllerDetect.cpp` | Adam Honse (CalcProgrammer1) 07 Feb 2020; Chris M (Dr_No) 15 Feb 2021; Sam B (4rcheria) 24 Nov 2024 | Product ids, which controller each id gets, the detector macro (`REGISTER_HID_DETECTOR`: no interface or usage filter) |
| `ThermaltakeRiingController/ThermaltakeRiingController.cpp`, `.h` | Adam Honse (CalcProgrammer1) 07 Feb 2020 | Riing: init report, RGB report, mode and speed constants, the read after every write |
| `ThermaltakeRiingController/RGBController_ThermaltakeRiing.cpp`, `.h` | Adam Honse (CalcProgrammer1) 09 Feb 2020 | Riing: five zones of 0..20 LEDs, Direct mode value 0x18, the Spectrum mode and its default speed, the update order |
| `ThermaltakeRiingQuadController/ThermaltakeRiingQuadController.cpp`, `.h` | Chris M (Dr_No) 15 Feb 2021 | Quad: 193-byte reports, persistent channel buffers, mode 0x24, the keepalive requirement |
| `ThermaltakeRiingQuadController/RGBController_ThermaltakeRiingQuad.cpp`, `.h` | Chris M (Dr_No) 15 Feb 2021 | Quad: five zones of 0..60 LEDs, Direct only |
| `ThermaltakeRiingTrioController/ThermaltakeRiingTrioController.cpp`, `.h` | Sam B (4rcheria) 24 Nov 2024 | Trio: 65-byte chunked reports (19 LEDs per chunk, at least two chunks) |
| `ThermaltakeRiingTrioController/RGBController_ThermaltakeRiingTrio.cpp`, `.h` | Sam B (4rcheria) 24 Nov 2024 | Trio: five zones of 0..54 LEDs, Direct only |

## Products

All `REGISTER_HID_DETECTOR` (any collection), vendor `0x264A`. OpenRGB names each one
"Thermaltake Riing (PID 0x....)", "Thermaltake Riing Quad (PID 0x....)" or
"Thermaltake Riing Trio (PID 0x....)".

| Product ids | Variant | LEDs per header |
|---|---|---|
| `1FA5`..`1FB5` (17 ids) | Riing | 0..20 |
| `2260`..`2270`, `232B` (18 ids) | Riing Quad | 0..60 |
| `2135`..`213F`, `2141`..`2145` (16 ids; `2140` is not registered) | Riing Trio | 0..54 |

51 product ids in total.

## Layout

Each controller has five channels, `Fan Header 1` .. `Fan Header 5` (OpenRGB: "Thermaltake Fan
Header n"). Their LED limit is the variant's maximum. The LED count comes from the user's
components or channel setting, like OpenRGB's resizable zones. The device itself has no fixed
LEDs.

## Protocol

All reports are output reports (`hid_write`) with report id `0x00`. Colour data is G, R, B per
LED.

**Init** (each controller's constructor, `SendInit`): `00 FE 33`, zero-padded to the report
length (65 for Riing and Trio, 193 for Quad), then one input report read (`hid_read_timeout`:
100 ms for Riing, 250 ms for Quad and Trio). `GetFirmwareVersion` (Riing: `00 33 50` + read) only
fills OpenRGB's device info, so it is not sent.

**Riing frame** (`SendRGB`, for each header 1..5 in order, also for headers with no LEDs): a
fresh 65-byte buffer `00 32 52 <port 1..5> <mode + (speed & 3)> G R B ...`. Direct is mode 0x18,
speed 0. The data holds at most 20 LEDs (60 bytes). One input report read (100 ms) follows each
write.

**Riing Quad frame** (`SetChannelLEDs`, each header in order, also with no LEDs): the header's
**persistent** 193-byte buffer `00 32 52 <port> 24 G R B ...` is written. OpenRGB never clears it,
so after a header shrinks, the bytes of earlier, longer frames stay behind the new data. Lightloom
keeps the same buffers. OpenRGB's comment: the Quad "only seems to run in direct mode and requires
a packet within seconds to remain in the set mode". OpenRGB resends the buffers when 3 s pass
without a commit. Lightloom resends an unchanged frame after 1 s.

**Riing Trio frame** (`SetChannelLEDs`): nothing for a header with no LEDs. Otherwise the header's
persistent 65-byte buffer gets `[3] = port`, `[4] = 0x24`, `[5] = 0x03`, `[7] = 0x00`. Then, for
chunk i = 0, 1, ... while `LEDs > 19 * i` or `i < 2`: bytes 6..64 are cleared, `[6] = i + 1`, and
up to 19 LEDs of data start at byte 8. Each chunk is one write. So 1..38 LEDs give 2 chunks and
39..54 give 3. The meaning of byte 5 (`0x03`) is not explained in OpenRGB.

Lightloom skips a frame whose colours did not change, and resends it after 1 s.

**Shutdown.** OpenRGB has no exit step.
- Riing: OpenRGB selects its hardware effects with the same RGB report and no save. The
  controller is handed back to **Spectrum** (mode `0x04`, speed Normal `0x02`, so mode byte
  `0x06`), with black colour data for each header's LED count.
- Quad and Trio: OpenRGB has only Direct. Every LED is set black in Direct. What the Quad does
  once the packets stop is unknown.

## Safety

- Fan speed: OpenRGB declares `SendFan()` and `SendSave()` in all three headers, but never
  implements or calls them. Lightloom sends only `FE 33` and `32 52` reports, and a test checks
  that.
- No other OpenRGB mode is exposed. The Riing's Flow, Ripple, Blink, Pulse and Wave are not used.
- Header data is cut to the variant's maximum, so a report never overflows.

## Endpoint

`REGISTER_HID_DETECTOR` takes every HID collection of the product, so `Validate` accepts any
endpoint. The descriptors of these controllers are not known here. If one turns out to expose
several collections, `Validate` should be narrowed to the one OpenRGB writes to.

## Untested

Every product id. Open questions for someone with the hardware:
- whether the Riing replies to every RGB report (otherwise each read waits 100 ms);
- what the Quad shows once Lightloom stops sending;
- the Trio's byte 5.
