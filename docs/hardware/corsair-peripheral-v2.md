<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Corsair Peripheral V2 (`1B1C`): keyboards, mice, MM700 and Slipstream receivers, adapted from OpenRGB

Plugin: `plugins/corsair-peripheral-v2/`. Untested on hardware (`tested` is empty). It drives the
LEDs of the Corsair peripherals OpenRGB's "Corsair V2" driver knows, per LED, in the device's
software render mode, and hands them back to the device's own lighting at shutdown.

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file below carries
`SPDX-License-Identifier: GPL-2.0-or-later` (checked file by file).

| # | File | License | What was taken |
|---|------|---------|----------------|
| S1 | `Controllers/CorsairPeripheralV2Controller/CorsairPeripheralV2ControllerDetect.cpp` | GPL-2.0-or-later | Product ids and names, which controller class each id gets (HW / SW), the detector macro (`REGISTER_HID_DETECTOR_IP`, interface 1, usage page `0xFF42`) |
| S2 | `.../CorsairPeripheralV2Controller.cpp`, `.h` (Chris M (Dr_No), 07 Aug 2022) | GPL-2.0-or-later | Every report: product id / vendor id queries and retries, receiver detection, lighting endpoint probe, render mode, `LightingControl`, transactions, block write, read-buffer drain, the destructor's switch back to hardware mode |
| S3 | `.../CorsairPeripheralV2HardwareController.cpp`, `.h` (Chris M (Dr_No), 07 Dec 2022) | GPL-2.0-or-later | Start sequence of the HW controller, colour data layouts (colour blocks for endpoint 1, `12 00` + RGB triplets for endpoint 2) |
| S4 | `.../CorsairPeripheralV2SoftwareController.cpp`, `.h` (Chris M (Dr_No), 11 Aug 2022) | GPL-2.0-or-later | Start sequence of the SW controller, colour blocks |
| S5 | `.../RGBController_CorsairV2Hardware.cpp`, `.h` (Chris M (Dr_No), 10 Dec 2022); `.../RGBController_CorsairV2Software.cpp`, `.h` (Chris M (Dr_No), 11 Aug 2022) | GPL-2.0-or-later | Zones and LEDs (`SetupZones`), keyboard layout selection from address `0x41`, the colour buffer map (`buffer_map`), Direct mode, the keepalive |
| S6 | `.../CorsairPeripheralV2Devices.cpp`, `.h` | GPL-2.0-or-later | Device list (zones, keymaps, key values), product id defines, render mode values |
| S7 | `KeyboardLayoutManager/KeyboardLayoutManager.cpp`, `.h` (Chris M (Dr_No), 04 Feb 2023); `RGBController/RGBControllerKeyNames.cpp` (Chris M (Dr_No), 25 Jan 2022) | GPL-2.0-or-later | Keyboard key names, rows and columns (run, not transcribed: see "How the tables and fixtures were made") |

`plugin.js` and `devices.js` keep these credits in their headers.

## How the tables and fixtures were made

OpenRGB's own code (S2-S7) was compiled with g++ outside the repository, with stub headers for
hidapi, the logger, threads and a minimal `RGBController` base class, and run against a scripted
device. That run produced:

- `devices.js`: for every entry of the device list and each keyboard layout code (1 ANSI, 2 ISO,
  4 JIS), the LEDs `SetupZones()` creates (names, `led.value`), the colour buffer size, and the
  row / column of every key in `KeyboardLayoutManager`; a generator turns that into HueDeck tables
  (identical tables shared). Detector rows are parsed from S1.
- `fixtures/expected.json`: the I/O (writes, reads with their length and timeout, sleeps) of the
  controller and RGBController constructors, two `DeviceUpdateLEDs()` calls and the destructor,
  for 16 scripted cases (below). `plugin.js` was never run to make them.

## Products

All `REGISTER_HID_DETECTOR_IP(..., 1, 0xFF42)`: interface 1, usage page `0xFF42`, any usage.
38 ids, all claimed; none is claimed by another OpenRGB Corsair detector or another HueDeck plugin.

| Product id | OpenRGB name | Controller | LEDs (ANSI / ISO) | Detect.cpp line |
|---|---|---|---|---|
| `1B1C:1BA4` | Corsair K55 RGB PRO | SW | keyboard, 6 (one linear zone) | 92 |
| `1B1C:1B6E` | Corsair K57 RGB (Wired) | SW | keyboard, 112 / 113 | 93 |
| `1B1C:1BA0` | Corsair K60 RGB PRO | SW | keyboard, 104 / 105 | 94 |
| `1B1C:1B8D` | Corsair K60 RGB PRO SE | SW | keyboard, 104 / 105 | 95 |
| `1B1C:1BAD` | Corsair K60 RGB PRO Low Profile | SW | keyboard, 104 / 105 | 96 |
| `1B1C:1BC7` | Corsair K60 RGB PRO TKL Black | HW | keyboard, 87 / 88 | 97 |
| `1B1C:1BED` | Corsair K60 RGB PRO TKL White | HW | keyboard, 87 / 88 | 98 |
| `1B1C:1BFD` | Corsair K70 Core RGB | HW | keyboard, 104 / 105 | 99 |
| `1B1C:2B01` | Corsair K70 Core RGB TKL | HW | keyboard, 86 / 87 | 100 |
| `1B1C:1BC4` | Corsair K70 RGB PRO | HW | keyboard, 115 / 116 | 102 |
| `1B1C:1BB3` | Corsair K70 RGB PRO V2 | HW | keyboard, 115 / 116 | 103 |
| `1B1C:1B73` | Corsair K70 RGB TKL | HW | keyboard, 97 / 98 | 104 |
| `1B1C:1BB9` | Corsair K70 RGB TKL Champion Series | HW | keyboard, 97 / 98 | 105 |
| `1B1C:1B89` | Corsair K95 RGB PLATINUM XT | HW | keyboard, 137 / 138 | 106 |
| `1B1C:1B7C` | Corsair K100 RGB Optical | HW | keyboard, 173 / 174 | 107 |
| `1B1C:1BC5` | Corsair K100 RGB Optical | HW | keyboard, 173 / 174 | 108 |
| `1B1C:1B7D` | Corsair K100 MX Red | HW | keyboard, 173 / 174 | 109 |
| `1B1C:1B4B` | Corsair Dark Core RGB SE (Wired) | HW | mouse, 12 | 114 |
| `1B1C:1B7E` | Corsair Dark Core RGB Pro SE (Wired) | HW | mouse, 12 | 115 |
| `1B1C:1B5E` | Corsair Harpoon Wireless (Wired) | SW | mouse, 2 | 116 |
| `1B1C:1B4C` | Corsair Ironclaw Wireless (Wired) | SW | mouse, 6 | 117 |
| `1B1C:2B32` | Corsair Ironclaw Wireless SE (Wired) | SW | mouse, 6 | 118 |
| `1B1C:2B00` | Corsair Ironclaw Wireless SE (receiver) | SW | the device it reports | 119 |
| `1B1C:1B93` | Corsair Katar Pro | HW | mouse, 2 | 120 |
| `1B1C:1BBA` | Corsair Katar Pro V2 | HW | mouse, 2 | 121 |
| `1B1C:1BAC` | Corsair Katar Pro XT | HW | mouse, 2 | 122 |
| `1B1C:1B70` | Corsair M55 RGB PRO | SW | mouse, 2 | 123 |
| `1B1C:1B9E` | Corsair M65 RGB Ultra Wired | SW | mouse, 3 | 124 |
| `1B1C:1BB5` | Corsair M65 RGB Ultra Wireless (Wired) | HW | mouse, 2 | 125 |
| `1B1C:1BF0` | Corsair M75 Gaming Mouse | HW | mouse, 2 | 126 |
| `1B1C:1BDC` | Corsair M75 Wireless (receiver) | HW | the device it reports | 127 |
| `1B1C:1BF1` | Corsair M75 Wireless (Wired) | HW | mouse, 2 | 128 |
| `1B1C:1BE3` | Corsair Scimitar Elite RGB | HW | mouse, 5 | 129 |
| `1B1C:1BA6` | Corsair Slipstream Wireless Receiver HW | HW | the device it reports | 130 |
| `1B1C:1B65` | Corsair Slipstream Wireless Receiver SW | SW | the device it reports | 131 |
| `1B1C:1B66` | Corsair Slipstream Wireless Receiver HW | SW | the device it reports | 132 |
| `1B1C:1B9B` | Corsair MM700 | SW | mousepad, 3 | 137 |
| `1B1C:1BC9` | Corsair MM700 3XL | SW | mousepad, 3 | 138 |

JIS tables (layout code 4) differ again (for example 117 LEDs on the K70 RGB PRO). The device list has 33
entries, keyed by the product id the **device** reports, not by the USB id: a receiver is driven
with the LED table of the device behind it. The K57 RGB Wireless (`1B62`) is not in OpenRGB's
list, so behind a receiver it is not driven (OpenRGB drops it the same way).

Grid: keyboard keys at their `KeyboardLayoutManager` row and column; other zones one row each
(a linear zone has its LEDs side by side), consecutive single-LED zones share a row. The K70 RGB
PRO / PRO V2's numeric-pad `.` lies in column 21, outside OpenRGB's 21-column zone map (so OpenRGB's
matrix view does not show it), but it is a LED with a colour slot; HueDeck places it at its
`KeyboardLayoutManager` cell, making those grids 22 columns wide.

## Protocol

Every request is a 65-byte **output report**: report id `0x00`, `write_cmd`, command, arguments,
zeros. Answers are 64-byte input reports; with hidapi's numbering (no report id), byte 2 is a status
(0 = success; 1 invalid value, 3 failed, 5 unsupported) and bytes 3-6 a little-endian value. HueDeck's
`device.read` puts the report id `0x00` first, so the plugin reads them one byte later.

`write_cmd` is `0x08` (wired), `0x09` (through a wireless receiver) or `0x80` (K57 RGB wired, which
also never gets its answers read except for GETs).

Commands: `0x01` SET, `0x02` GET, `0x05` stop transaction, `0x06` block write (first packet),
`0x07` block write (more), `0x0D` start transaction.

**Start** (`CorsairPeripheralV2Controller` constructor, then the HW / SW constructor, then
`SetupZones`):

1. GET `0x12` (product id), up to 8 tries 120 ms apart while the answer is an error or 0. A
   Slipstream receiver id (`1BA6`, `1B66`, `1B65`, `1BDC`, `2B00`) switches `write_cmd` to `0x09`
   and asks again (the device behind it); `1B6E` (K57) switches to `0x80`, lighting endpoint 1, no
   reads.
2. Unknown product id: OpenRGB drops the device; HueDeck stops here (see Deviations).
3. GET `0x11` (vendor id; OpenRGB uses it to settle the packet length), read with length 1024.
4. Lighting endpoint: endpoint 2 (`0x22`) unless the device refuses it: start transaction
   `0D 00 22`; a non-zero status (or no answer) selects endpoint 1 and starts again `0D 00 01`;
   then stop `05 01 00`. The K100 ids (`1B7C`, `1BC5`, `1B7D`) stay on endpoint 2 without the probe.
5. SET `0x03` (render mode) = `0x02` (software): `01 03 00 02`.
6. GET `0x5F` (`LightingControl`).
7. GET `0x41`: keyboard layout (1 ANSI, 2 ISO, 4 JIS; anything else ANSI), which picks the table.

**Every frame** (`DeviceUpdateLEDs` → `SetLedsDirect` → `SetLEDs`):

- Colour buffer: `bufferSize` slots, each LED's colour at its `led.value`, black elsewhere.
- Data: HW controller on endpoint 2: `12 00` (Direct mode id `0x0012`) then R G B per slot; HW on
  endpoint 1 and every SW controller: all R, then all G, then all B ("colour blocks").
- Drain pending input reports (3 ms reads until none; HueDeck stops after 64), start transaction
  `0D 00 <endpoint>` + read, first packet `06 00 <len lo> <len hi> 00 00` + the first 57 data bytes,
  read its answer for 3 ms (no answer: sleep 2 ms), then `07 00` + 61 data bytes per packet with a
  2 ms sleep after each (the last one zero-padded), then stop `05 01 00` + read. With the K57 there
  are no reads, only the sleeps.

HueDeck skips a frame whose colours did not change and resends it after 1 s (OpenRGB's keepalive
resends after 30 s idle, because the K55 RGB PRO falls back to its rainbow after a minute).

**Shutdown.** OpenRGB's controller destructor sets render mode `0x01` (hardware): `01 03 00 01`
(+ read). That hands the LEDs back to the device's own lighting (its onboard / hardware effect),
and per OpenRGB's comment also restores onboard input events such as the K70 Core's volume knob.
HueDeck sends exactly that, also when the system suspends.

## Deviations from OpenRGB (deliberate)

- The vendor-id query buffer is never cleared in OpenRGB, so byte 0 (the report id!) and bytes
  4-64 are stack contents; HueDeck sends `00 <write_cmd> 02 11` and zeros.
- For a product id OpenRGB's list does not have, OpenRGB still sends the vendor id query, switches
  to software mode and, when the detector deletes the controller, back to hardware mode. HueDeck
  sends nothing after the product id query.
- The read-buffer drain is bounded (64 reads); OpenRGB loops while reads return data (and forever
  on a read error, since it stores the result in a `uint8_t`).
- Frames are skipped while unchanged (1 s resend) instead of OpenRGB's 30 s keepalive thread.

## Safety

- Only lighting I/O is written: SET of property `0x03` (render mode, 1 or 2), GETs (`0x11`,
  `0x12`, `0x41`, `0x5F`), transactions on the lighting endpoint (`0x01` / `0x22`) and block writes
  of the colour buffer. The tests check that no other command or SET property is ever written.
- OpenRGB's hardware-effect block write in `UpdateHWMode` (Static and other effects, written to the
  device's hardware mode) is commented out in OpenRGB itself; HueDeck does not have it either. The
  HW controller's Static mode does nothing in OpenRGB (`DeviceUpdateMode` is empty) and is not
  ported. No profile, flash or onboard-memory write exists in this protocol path.
- No DPI, polling rate, key remap, macro, battery or power setting is ever sent.

## Untested

Every product id. Things to look at on hardware first: the read offsets of the answers (HueDeck
assumes unnumbered input reports, so the answer starts after a `0x00` report id, as hidapi strips
it for OpenRGB), the lighting endpoint probe, and the receivers (the device behind them must be
awake during `Initialize()`; otherwise nothing is driven, as in OpenRGB).
