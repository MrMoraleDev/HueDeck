<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Razer (`1532`): 90-byte Razer report devices, Chroma ARGB controller, Kraken V4, adapted from OpenRGB

The plugin is `plugins/razer/`. It drives, in per-LED ("Direct") mode, the Razer devices OpenRGB
handles with `DetectRazerControllers` (keyboards, laptops, mice, keypads, mousemats, headsets,
speakers, headset stands, docks, cases, GPUs and other accessories, and the devices paired to a
Razer wireless receiver), `DetectRazerARGBControllers` (Chroma Addressable RGB Controller) and
`DetectRazerKrakenV4Controllers` (Kraken V4 and Kraken Kitty V3 Pro). The other sub-protocols of
OpenRGB's `Controllers/RazerController` have nothing in common with these reports and have their
own plugins: `plugins/razer-kraken/` (Kraken 7.1 and Kraken V3 families,
`docs/hardware/razer-kraken.md`) and `plugins/razer-hanbo/` (Hanbo Chroma cooler,
`docs/hardware/razer-hanbo.md`). Razer LampArray collections (usage page `0x59`) are left to
`plugins/hid-lamparray/`.

Only the DeathAdder V2 Pro (`1532:007C` wired, `1532:007D` wireless) and the Razer Mouse Dock
Chroma (`1532:007E`) have run on hardware with this plugin (see "Tested on hardware" below); the
mouse uses Lightloom's own recorded packets, the same bytes as the retired
single-device plugin `razer-deathadder-v2-pro` (its protocol notes stay in
`docs/hardware/razer-deathadder-v2-pro.md`).

## Tested on hardware

2026-10-22, with the user's devices (the plugin run from a copy without the `ProductId()` filter
that then left `007C`/`007D` to the single-device plugin), red, green and blue in turn, 0 errors
on each:

- Razer DeathAdder V2 Pro, wired (`1532:007C`): the mouse's logo showed each colour.
- Razer DeathAdder V2 Pro, wireless through the dock (`1532:007D`): the same.
- Razer Mouse Dock Chroma (`1532:007E`): the dock's light showed each colour, and a 20 s solid
  red hold stayed red (the keepalive works). After the run the dock goes to spectrum cycle: that
  is this plugin's `Shutdown()` (`SetModeSpectrumCycle`, see "Exit"), as expected.

All of this was with Razer Synapse closed. With Synapse 4 running the light flickered, because Synapse keeps writing its own
lighting: Synapse 4's `RazerAppEngine.exe` must be closed, and Lightloom reports it as a
conflicting process (`conflictingProcesses` in `manifest.json`, with `RazerCentralService.exe`
and the Synapse 3 / Synapse executables). `007C`, `007D` and `007E` are marked tested in
`manifest.json`; every other product is not.

## Sources

| # | Source | License | Used for |
|---|--------|---------|----------|
| S1 | OpenRGB `Controllers/RazerController/RazerControllerDetect.cpp` | GPL-2.0-or-later | Detector lines (name, product id, interface, usage page, usage; `_WIN32` branch for the Nommo), detector functions (paired devices, ARGB interfaces) |
| S2 | OpenRGB `Controllers/RazerController/RazerDevices.h`, `RazerDevices.cpp` | GPL-2.0-or-later | Product ids, `razer_device_list` (name, type, matrix type, transaction id, matrix size, zones, keyboard layout overlays) |
| S3 | OpenRGB `Controllers/RazerController/RazerController/RazerController.cpp`, `RazerController.h` | GPL-2.0-or-later | Every 90-byte report, the per-product switches (report id, LED id, software mode, keepalive, brightness range, custom-matrix devices), `SetLEDs`, queries and transfers |
| S4 | OpenRGB `Controllers/RazerController/RazerController/RGBController_Razer.cpp`, `.h` | GPL-2.0-or-later | Zones and LED names (`SetupZones`), keyboard layout choice, what switching to Direct sends (`DeviceUpdateMode`) |
| S5 | OpenRGB `Controllers/RazerController/RazerController/RGBController_RazerAddressable.cpp`, `.h` | GPL-2.0-or-later | ARGB channels (6 x 80), zone-size report, `DeviceUpdateLEDs` / `DeviceUpdateMode` |
| S6 | OpenRGB `Controllers/RazerController/RazerKrakenV4Controller/RazerKrakenV4Controller.cpp`, `.h`, `RGBController_RazerKrakenV4.cpp`, `.h` | GPL-2.0-or-later | The 64-byte Kraken V4 reports (direct, brightness, wave), wireless flag |
| S7 | OpenRGB `KeyboardLayoutManager/KeyboardLayoutManager.cpp`, `.h`; `RGBController/RGBControllerKeyNames.cpp` | GPL-2.0-or-later | Key names of keyboard zones (simulated by the table generator, see below) |
| S8 | OpenRGB `Controllers/RazerController/RazerDeviceGuard.cpp`, `DetectionManager.cpp` | GPL-2.0-or-later | Read only, nothing taken: the global mutex OpenRGB shares with Synapse; how detectors run per HID collection |
| S9 | Lightloom's recordings `trace_1532_007c.json` (`#00ff00`) and `trace_multi.json` (`#ff8000`) of the user's DeathAdder V2 Pro, and the retired plugin `razer-deathadder-v2-pro` (Lightloom, GPL-3.0-or-later; `docs/hardware/razer-deathadder-v2-pro.md`, `fixtures/deathadder-v2-pro-recorded.json`) | data / Lightloom | The DeathAdder V2 Pro packets |

All OpenRGB files are at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB); every one carries
`SPDX-License-Identifier: GPL-2.0-or-later`. Author lines kept in the plugin headers:
`Adam Honse (CalcProgrammer1)  22 Jan 2021` (RazerControllerDetect.cpp, RazerController,
RGBController_Razer), `Adam Honse (CalcProgrammer1)  04 Sep 2021` (RazerDevices),
`Adam Honse (CalcProgrammer1)  11 Apr 2021` (RGBController_RazerAddressable),
`Adam Honse (CalcProgrammer1)  21 Oct 2025` (RazerKrakenV4Controller, RGBController_RazerKrakenV4),
`Chris M (Dr_No)  04 Feb 2023` (KeyboardLayoutManager), `Chris M (Dr_No)  25 Jan 2022`
(RGBControllerKeyNames). No file of `HIDLampArrayController` or any `GPL-2.0-only` file was used.

## How the tables are made

`plugins/razer/devices.js` is generated, the protocol (`plugin.js`, `razer-protocol.js`) is
hand-written. The generator (two Python scripts kept outside the repository, run with
`python -I`) tokenises the C++ sources and evaluates their brace initialisers:

1. `RazerDevices.h`: every `#define RAZER_*_PID`; `RazerDevices.cpp`: every `razer_zone`,
   `razer_device` and `keyboard_keymap_overlay_values`, and the order of `razer_device_list`
   (OpenRGB takes the **last** entry with a product id; all 199 ids are unique).
2. `RazerControllerDetect.cpp` with `_WIN32` defined and comments removed (three commented-out
   Bluetooth lines are not detectors): 201 `REGISTER_HID_DETECTOR*` lines.
3. The `switch(dev_pid)` groups of `RazerController.cpp` (report id, software mode, LED id 0,
   keepalive, brightness 100) and `RazerKrakenV4Controller.cpp` (wireless flag), found by the
   comment above each switch and the statement in the case group.
4. Keyboard zones: a Python model of `KeyboardLayoutManager` (same insert / swap / remove / row
   algorithms, including its quirks) builds each layout for the four layouts
   `RGBController_Razer::SetupZones()` can pick (ANSI, ISO, ISO QWERTZ, ISO AZERTY) and names
   every cell of the zone grid with `GetKeyNameAt(row, col)`.

`docs/hardware/razer.md`'s product table and the test fixtures come from the same data. The
fixtures (`plugins/razer/fixtures/openrgb-packets.json`) are computed by a separate Python
transcription of the report functions, not by running `plugin.js`.

Both were checked against OpenRGB's own code compiled outside the repository (g++ with stub
headers for hidapi, the device guard and the logger): the key names of all 53 keyboard tables in
all four layouts (24 352 cells) equal the Python model's, and the fixture packets of the 13 test
products (137 reports, start / frame / exit, built by calling `RazerController` and
`RazerKrakenV4Controller` methods) equal the compiled code's byte for byte, except three bytes of
the Orbweaver Chroma's fourth row that OpenRGB leaves uninitialised (Lightloom sends zeros).

## Transport and Validate

90-byte reports are HID **feature** reports of 91 bytes (`sizeof(razer_report)`, report id
first): `hid_send_feature_report` = `device.send_report(bytes, 91)`, answers with
`hid_get_feature_report` = `device.get_report([report id], 91)`. Report id `0x00`, except
`0x07` for the Leviathan V2 and V2 X.

`Validate()` accepts the collections of the product's detector lines: `IPU` = interface, usage
page and usage; `I` (Tiamat 7.1 V2, ARGB controller) = any collection of that interface; `PU`
(Naga Pro V3, Seiren Emote, Thunderbolt 4/5 Dock) = any interface with that usage page and usage.
Two products have two lines: the Blackwidow V4 and V4 Pro (interface 3, `0001/0000` before
firmware 1.5, `000C/0001` from 1.5) and `0F21` (Kraken Kitty Black Edition on interface 1
`0001/0003`, Thunderbolt 4 Dock Chroma on `000C/0001`; both use the Thunderbolt 4 Dock's table
entry, the only one with that id). Usage page `0x59` (LampArray) is never accepted.

OpenRGB runs a detector on every collection that matches, so an interface-only line drives every
collection of the interface and the one with the Razer report works. Lightloom opens one
collection; for the Tiamat 7.1 V2 and for products with two detector lines (Blackwidow V4 and V4
Pro) the plugin therefore switches (`set_endpoint`) to the collection, among those its detector
lines match, whose report descriptor declares a feature report of at least 91 bytes
(`getHidReportFields()`), else to the first one. Where OpenRGB would create two controllers (a
Blackwidow V4 exposing both collections), Lightloom drives one. The ARGB controller is detected
on interface 0 and OpenRGB opens interface 1 beside it; `Validate()` accepts both interfaces and
the plugin picks interface 0's 91-byte collection for the Razer reports and interface 1's
321-byte collection for the ARGB frames the same way.

## The 90-byte report (S3)

| Byte | Field | Value |
|------|-------|-------|
| 0 | report id | `0x00` (`0x07` on the Leviathan V2 / V2 X) |
| 1 | status | `0x00` in requests; answers: 1 busy, 2 success, 3 failure, 4 timeout, 5 not supported |
| 2 | transaction id | from the table (`0x1F`, `0x3F`, `0x9F`, `0xFF`, ...) |
| 3-4 | remaining packets | 0 |
| 5 | protocol type | 0 |
| 6 | data size | per command |
| 7 | command class | |
| 8 | command id | |
| 9-88 | arguments (80) | |
| 89 | crc | XOR of bytes 3..88 |
| 90 | reserved | 0 |

Commands the plugin sends (storage byte always `0x00`, "no save"; `led` = the product's LED id,
`0x00` for the products of the `RAZER_LED_ID_ZERO` case list, else `0x05`):

| Command | Class, id, size | Arguments | When |
|---------|-----------------|-----------|------|
| custom frame, standard matrix | `03 0B`, 3n+4 | `FF row start stop rgb...` | each frame, one per matrix row |
| custom frame, extended matrix | `0F 03`, 3n+5 | `00 00 row start stop rgb...` | each frame, one per matrix row |
| custom frame, linear | `03 0C`, `32` | `start stop rgb...` | each frame |
| custom effect, standard / linear | `03 0A`, `02` | `05 00` | after each frame |
| custom effect, extended | `0F 02`, `0C` | `00 00 08` | after each frame |
| brightness, standard / linear | `03 03`, `03` | `00 led b` | on start (Direct) and exit |
| brightness, extended | `0F 04`, `03` | `00 led b` | on start (Direct) and exit; ARGB: led `1A`..`1F` |
| spectrum cycle, standard / linear | `03 0A`, `01` | `04` | exit |
| spectrum cycle, extended / ARGB | `0F 02`, `06` | `00 led 03` | exit |
| device mode | `00 04`, `02` | `03 00` software / `00 00` hardware | start / exit of the Charging Pad Chroma, Chroma Mug Holder, Firefly HyperFlux |
| per-LED colour | `03 01`, `05` | `00 led r g b` | custom-matrix devices, each frame |
| per-LED effect | `03 02`, `03` | `00 led effect` (0 static/custom, 4 spectrum) | custom-matrix devices |
| addressable zone sizes | `0F 08`, `0D` | `06`, then per channel `19 n` (or `ch 00` when n = 0) | ARGB controller, only after a channel count changes during the session (never at start, never all zero) |
| get device mode | `00 84`, `02` | | Blade 14 (2021/2022) keepalive, every second |
| get keyboard info | `00 86`, `00` | answer: layout in argument 0 | start, devices with a keyboard layout |
| get wireless connections | `00 BF`, `50` | answer: slot count, then per slot status + big-endian pid | start (paired devices) |

Requests that need an answer follow `razer_usb_query()`: up to 3 sends, each followed by up to
25 reads 2 ms apart, accepting only an answer with the same transaction id, class and id whose
status is neither new nor busy.

## Start (Initialize)

What OpenRGB does when it creates the controller and the user picks Direct:

1. Charging Pad Chroma (`0F26`), Chroma Mug Holder (`0F07`), Firefly HyperFlux (`0068`): device
   mode software (`00 04 03 00`) (`RazerController` constructor).
2. Devices with a keyboard layout: `00 86`, then the key names of the layout
   (`RGBController_Razer::SetupZones()`; ANSI for US, Russian, Chinese Traditional, Turkish, Thai,
   Arabic, Japanese and unknown answers; ISO for Greek, UK, Nordic, Korean, Brazilian, Latin
   American, Swiss, Spanish, Italian, Portuguese, Hebrew; QWERTZ for German; AZERTY for French).
3. Paired devices (`DetectRazerControllers()`): `00 BF` query. A receiver (Mouse Dock Pro,
   HyperSpeed dongles) answers with its slots; every listed product id other than its own that has
   a table entry becomes a Lightloom **subdevice** named "`<device> (<receiver>)`", with its own
   transaction id, report id and LED id, sharing the receiver's handle, and gets steps 1 and 2
   itself. As in OpenRGB, every later command to a paired device is followed by polling the
   answer (up to 50 reads, 2 ms apart) until the receiver has relayed it. Devices that are not
   receivers refuse the query (status 5) and get no subdevice.
4. Switching to Direct: no effect report, then `SetBrightness(max)` (255, or 100 on the
   DeathAdder Essential V2). Lightloom's colours already carry the user's brightness.

Left out (read-only, they only fill OpenRGB's info page or mode list): firmware version
(`00 81`), serial (`00 82`), the keyboard info query on non-keyboard devices, the variant name,
and the supported-effects queries (`0F 80`, `0F 81`) of extended-matrix devices.

## Frames (Render)

`RazerController::SetLEDs()`: for each matrix row, 1 ms pause, the custom-frame report of the
matrix type (columns 0..cols-1); 1 ms pause; the custom-effect report. Colours are OpenRGB's
LED list in order: zone by zone, row by row; colour index `row * cols + col` of the table's matrix
size. Where the zones hold fewer LEDs than the matrix (Blackwidow V4: 156 of 8x23; Blackwidow V4
Pro: 181 of 184; Basilisk V3 Pro 35K Phantom Green: 12 of 13) OpenRGB reads past its colour
array; Lightloom sends black there.

Custom-matrix devices (OpenRGB's nested `switch(dev_pid)`):

| Product | Frame | Custom effect |
|---------|-------|---------------|
| Tartarus Chroma `0208` | `03 01` backlight `05` = LED 0 | `03 02` backlight 0 |
| DeathAdder Chroma `0043` | `03 01` scroll wheel `01` = LED 0, logo `04` = LED 1 | `03 02` scroll wheel 0, logo 0 |
| Naga Epic Chroma `003E` | `03 01` scroll wheel = LED 0, 1 ms, backlight = LED 1 | `03 02` scroll wheel 0, 1 ms, backlight 0 |
| Mamba 2012 `0024`, `0025` | `03 01` scroll wheel = LED 0 (matrix 1x15, one LED) | `03 02` scroll wheel 0 |
| Orbweaver Chroma `0207` | standard frame rows 1..4, columns +1; row 4 (index 3) is spread over 6 columns with a gap after the first key (uninitialised in OpenRGB, zeros here) | `03 0A 05 00` |

On the DeathAdder Chroma OpenRGB's LED 0 is the zone named "Logo" but goes to the scroll-wheel
LED id (and LED 1, "Scroll Wheel", to the logo); the port keeps that.

Unchanged frames are sent again only after 1 s (OpenRGB sends every update). The Blade 14 (2021)
and (2022) also get OpenRGB's keepalive: a get-device-mode request (2 ms, request, 5 ms, read)
once a second (OpenRGB's thread checks a `last_update_time` that is never set, so it always sends).

## Exit (Shutdown)

OpenRGB has no exit step. Lightloom hands each device (and each paired device) a hardware effect
back with OpenRGB's own reports: spectrum cycle (`SetModeSpectrumCycle()`, custom-matrix devices
their per-LED effect 4), then brightness at the maximum (`DeviceUpdateMode()` applies the mode's
brightness). The three products switched to software mode at start go back to hardware mode
(`00 04 00 00`). Nothing is saved: the device's own stored effect returns at its next power-up.

## Chroma Addressable RGB Controller (`0F1F`, S5)

Six channels, `Channel 1` .. `Channel 6`, up to 80 LEDs each (Lightloom channels). The colour
frames are 321-byte feature reports on interface 1, unnumbered (`hid id 0x00`): byte 1 `0x04`
(`0x84` from row 5 on, i.e. channel 6), channel index twice, a pad byte, the last LED index (79),
then 80 RGB triplets; one report per channel, 1 ms apart. Unused LEDs of a channel: OpenRGB's
buffer is uninitialised there; Lightloom sends black. No custom-effect report follows (OpenRGB sends
none for this matrix type). OpenRGB sends `SetAddressableZoneSizes()` only from
`DeviceConfigureZone()`, when the user resizes a zone, never at start; whether the controller
stores the sizes persistently is unknown, and direct frames do not need them (80 LEDs per channel
are sent whatever the size). So the plugin sends nothing at start and takes the first frame's
channel counts as a baseline; it sends the report (on interface 0) only after a channel count
changes during the session, and never when every count is 0. The controller otherwise keeps the
sizes it already has (for example set in Synapse). If interface 0 or 1 is missing, nothing is
sent. Start: brightness 255 on LED ids `1A`..`1F`; exit: spectrum cycle (LED id `05`)
and brightness 255.

## Kraken V4 family (`056B`, `056C`, `0587`, `0588`, S6)

64-byte **output** reports (`hid_write`), report id `0x02`: status, transaction id `0x60`,
remaining packets, protocol type, data size, class, id, a wireless flag (`0x80` on `056C` and
`0588`, else `0x00`), 52 argument bytes, crc, reserved. OpenRGB's send function does not compute
the crc (only its receive function does), so the crc byte goes out as `0x00`; the port keeps that.

- Direct sends nothing on start (OpenRGB's `DeviceUpdateMode()` only handles Wave).
- Frame: `0F 03`, size `5 + 3 * cols`, arguments `00 00 00 00 cols-1 rgb...` (9 LEDs; 10 on the
  Kitty V3 Pro: headset 9 + ears).
- Exit: wave (`00 00`, size 5, `C0 00 01 04`) and brightness 255 (`C1 00 01 FF`).

## DeathAdder V2 Pro (`007C` wired, `007D` receiver)

OpenRGB drives it as an extended-matrix mouse with transaction id `0x3F`, LED id `0x05`, the
brightness report on start and the custom effect after every frame. Lightloom's own recordings
(S9) show the mouse answering transaction ids `0x1F` to `0x9F` and taking frames without the
effect report. Because this family plugin replaced the single-device plugin
`razer-deathadder-v2-pro` (retired 2026-10-22; report layout and commands in
`docs/hardware/razer-deathadder-v2-pro.md`), it keeps that plugin's packets for these two ids instead of OpenRGB's: serial-number query `00 82` with
transaction ids `1F`, `3F`, `9F`, `FF` until one is confirmed, frame mode `0F 02` with
`00 00 08 00 01 00`, each frame `0F 03` with `00 00 00 00 00 r g b` (every frame, no skipping),
and on exit a black frame and `00 04 00 00`. No paired-device query.

Replay (`lightloom compat replay`, no hardware): with `trace_multi.json` (`#ff8000`) the plugin
sends the mouse 162 reports (serial query, frame mode, 158 frames `0F 03 ... FF 80 00`, black
frame, `00 04`), and every send and read of the replay is byte for byte what
the single-device plugin did in the same replay (before it was retired). The recorded colour frames are the
same bytes (`00 00 1F 00 00 00 08 0F 03 00 00 00 00 00 FF 80 00 ...`, crc `7B`). Both plugins
diverge from the recording at packet 1: it was made with a SignalRGB plugin, which probes
transaction ids `0x2F`..`0x9F` and sends other queries first, sets brightness with the save flag
and writes the DPI stages (`04 06`) on exit; Lightloom sends none of that. Same with
`trace_1532_007c.json` (5 reports, identical to the single-device plugin's).

Ownership: this plugin claims `007C` and `007D` (Validate: interface 0, usage page 1, usage 2).
The single-device plugin `plugins/razer-deathadder-v2-pro/` that claimed them too was retired
on 2026-10-22, after this plugin was tested on the mouse (see "Tested on hardware"), so there is
no double claim.

## Geometry

Zones are stacked top to bottom in table order; LED (row, col) of a zone is the cell
`[col, zone offset + row]`, the device is as wide as its widest zone. LED names follow
`RGBController_Razer::SetupZones()`: the zone name for a one-LED zone, `<zone> LED <col + 1>`
for one-row zones; multi-row zones without a keyboard layout get `<zone> LED <row * cols + col +
1>` (OpenRGB repeats `<col + 1>` on every row; Lightloom needs unique names). Keyboard zones name
their keys; cells without a key are sampled (their colour is on the wire, as in OpenRGB) but not
listed as LEDs. Paired devices are subdevices with their own grid.

## Untested

Everything except the DeathAdder V2 Pro (`007C`, `007D`) and the Mouse Dock Chroma (`007E`; see
"Tested on hardware"): no other product of this plugin has run on hardware (`tested` in
`manifest.json` lists those three only). In particular the paired-device path,
the ARGB controller's interface selection, the Kraken V4 reports and the custom-matrix devices
are unverified.

## Left out, and why

- Hardware effects other than the exit's spectrum cycle / wave (static, breathing, wave,
  reactive, starlight, ripple, fire): Lightloom drives LEDs per frame.
- Anything with the save storage flag: OpenRGB sends none; neither does Lightloom.
- Firmware, serial, variant and supported-effect queries (see "Start").
- The Bluetooth detector lines OpenRGB comments out (Blackwidow V3 Pro, Basilisk V3 Pro and
  Basilisk V3 Pro 35K Phantom Green over Bluetooth, vendor `068E`): not detectors at this commit.
- OpenRGB's global mutex `Global\RazerLinkReadWriteGuardMutex` (`RazerDeviceGuard`), which it
  shares with Razer Synapse: Lightloom has no such host call; Synapse is reported as a conflicting
  process instead.

## Products

187 product ids from 190 detector lines (OpenRGB has 201; the Kraken 7.1, Kraken V3 and Hanbo
lines belong to the other two plugins; `0287`, `028D` and `0F21` have two lines), vendor `0x1532`. Detector: `IPU` interface / usage page / usage, `I` interface
only, `PU` usage page / usage only. Matrix, tid: OpenRGB matrix type and transaction id.

| PID | OpenRGB name (detector line) | Detector | Protocol | Matrix, tid | Grid | Zones | Notes |
|-----|------------------------------|----------|----------|-------------|------|-------|-------|
| `0241` | Razer Blackwidow 2019 | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_2019_layout` |
| `0203` | Razer Blackwidow Chroma | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_chroma_layout` |
| `0209` | Razer Blackwidow Chroma Tournament Edition | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_chroma_te_layout` |
| `0221` | Razer Blackwidow Chroma V2 | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_chroma_v2_layout` |
| `0228` | Razer Blackwidow Elite | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_elite_layout` |
| `0211` | Razer Blackwidow Overwatch | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05 |
| `024E` | Razer Blackwidow V3 | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_v3_layout` |
| `025A` | Razer Blackwidow V3 Pro (Wired) | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_v3_pro_layout` |
| `025C` | Razer Blackwidow V3 Pro (Wireless) | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_v3_pro_layout` |
| `0A24` | Razer Blackwidow V3 TKL | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v3_tkl_layout` |
| `0258` | Razer Blackwidow V3 Mini (Wired) | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 5x16 | Keyboard 5x16 | LED id 0x05, layout `razer_blackwidow_v3_mini_layout` |
| `0271` | Razer Blackwidow V3 Mini (Wireless) | IPU I 3, P `000C`, U `0001` | razer | extended, `9F` | 5x16 | Keyboard 5x16 | LED id 0x05, layout `razer_blackwidow_v3_mini_layout` |
| `0287` | Razer Blackwidow V4 | IPU I 3, P `0001`, U `0000` | razer | extended, `1F` | 8x23 | Keyboard 6x23; Underglow Left 1x9; Underglow Right 1x9 | LED id 0x05, layout `razer_blackwidow_v4_layout`, zones hold 156 of 184 LEDs |
| `0287` | Razer Blackwidow V4 | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 8x23 | Keyboard 6x23; Underglow Left 1x9; Underglow Right 1x9 | LED id 0x05, layout `razer_blackwidow_v4_layout`, zones hold 156 of 184 LEDs |
| `028D` | Razer Blackwidow V4 Pro | IPU I 3, P `0001`, U `0000` | razer | extended, `1F` | 8x23 | Keyboard 6x23; Underglow Left 1x9; Underglow Right 1x9; Void 1x5; Underglow Wrist Rest 1x20 | LED id 0x05, layout `razer_blackwidow_v4_pro_layout`, zones hold 181 of 184 LEDs |
| `028D` | Razer Blackwidow V4 Pro | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 8x23 | Keyboard 6x23; Underglow Left 1x9; Underglow Right 1x9; Void 1x5; Underglow Wrist Rest 1x20 | LED id 0x05, layout `razer_blackwidow_v4_pro_layout`, zones hold 181 of 184 LEDs |
| `02B3` | Razer Blackwidow V4 Pro 75% (Wired) | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v4_pro_75_layout` |
| `02B4` | Razer Blackwidow V4 Pro 75% (Wireless) | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v4_pro_75_layout` |
| `02A5` | Razer Blackwidow V4 75% (Wired) | IPU I 3, P `0001`, U `0000` | razer | extended, `1F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v4_pro_75_layout` |
| `0293` | Razer Blackwidow V4 X | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_v4_x_layout` |
| `02D7` | Razer Blackwidow V4 TKL (Wired) | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v4_tkl_layout` |
| `02D5` | Razer Blackwidow V4 TKL (Wireless) | IPU I 2, P `0001`, U `0002` | razer | extended, `9F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v4_tkl_layout` |
| `02D4` | Razer Blackwidow V4 Low Profile TKL (Wired) | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v4_tkl_layout` |
| `02D2` | Razer Blackwidow V4 Low Profile TKL (Wireless) | IPU I 2, P `0001`, U `0002` | razer | extended, `9F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_blackwidow_v4_tkl_layout` |
| `0216` | Razer Blackwidow X Chroma | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_x_chroma_layout` |
| `021A` | Razer Blackwidow X Chroma Tournament Edition | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_blackwidow_x_chroma_te_layout` |
| `022A` | Razer Cynosa Chroma | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_cynosa_chroma_layout` |
| `025E` | Razer Cynosa Chroma V2 | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_cynosa_chroma_v2_layout` |
| `023F` | Razer Cynosa Lite | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Keyboard | LED id 0x05 |
| `0204` | Razer Deathstalker Chroma | IPU I 2, P `0001`, U `0002` | razer | linear, `3F` | 1x12 | Keyboard 1x12 | LED id 0x05 |
| `0295` | Razer Deathstalker V2 | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_deathstalker_v2_layout` |
| `0298` | Razer Deathstalker V2 Pro TKL (Wired) | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 6x17 | Keyboard 6x17 | LED id 0x00, layout `razer_deathstalker_v2_pro_tkl_layout` |
| `0296` | Razer Deathstalker V2 Pro TKL (Wireless) | IPU I 2, P `0001`, U `0002` | razer | extended, `9F` | 6x17 | Keyboard 6x17 | LED id 0x00, layout `razer_deathstalker_v2_pro_tkl_layout` |
| `0292` | Razer Deathstalker V2 Pro (Wired) | IPU I 3, P `000C`, U `0001` | razer | extended, `9F` | 6x22 | Keyboard 6x22 | LED id 0x00, layout `razer_deathstalker_v2_pro_layout` |
| `0290` | Razer Deathstalker V2 Pro (Wireless) | IPU I 2, P `0001`, U `0002` | razer | extended, `9F` | 6x22 | Keyboard 6x22 | LED id 0x00, layout `razer_deathstalker_v2_pro_layout` |
| `0227` | Razer Huntsman | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_full_size_shifted_layout` |
| `0226` | Razer Huntsman Elite | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 9x22 | Keyboard 6x22; Underglow 3x22 | LED id 0x05, layout `razer_huntsman_common_layout` |
| `0257` | Razer Huntsman Mini | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 5x15 | Keyboard 5x15 | LED id 0x05, layout `razer_huntsman_mini_layout` |
| `0282` | Razer Huntsman Mini Analog | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 5x15 | Keyboard 5x15 | LED id 0x05, layout `razer_huntsman_mini_layout` |
| `0243` | Razer Huntsman Tournament Edition | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 6x18 | Keyboard 6x18 | LED id 0x05, layout `razer_huntsman_te_layout` |
| `0266` | Razer Huntsman V2 Analog | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 9x22 | Keyboard 6x22; Underglow 3x22 | LED id 0x05, layout `razer_huntsman_common_layout` |
| `026B` | Razer Huntsman V2 TKL | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 6x17 | Keyboard 6x17 | LED id 0x05, layout `razer_huntsman_v2_tkl_layout` |
| `026C` | Razer Huntsman V2 | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_huntsman_v2_layout` |
| `02A6` | Razer Huntsman V3 Pro | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_huntsman_v3_pro_layout` |
| `02A7` | Razer Huntsman V3 Pro TKL White | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 6x19 | Keyboard 6x19 | LED id 0x05, layout `razer_huntsman_v3_pro_tkl_layout` |
| `02E4` | Razer Huntsman V3 HE Magnetic Mini | IPU I 3, P `000C`, U `0001` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05 |
| `021E` | Razer Ornata Chroma | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_full_size_shifted_layout` |
| `025D` | Razer Ornata Chroma V2 | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 6x22 | Keyboard 6x22 | LED id 0x05, layout `razer_ornata_chroma_v2_layout` |
| `028F` | Razer Ornata V3 | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 1x10 | Keyboard 1x10 | LED id 0x05 |
| `02A1` | Razer Ornata V3 Rev2 | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 1x10 | Keyboard 1x10 | LED id 0x05 |
| `02A3` | Razer Ornata V3 TKL | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 1x8 | Keyboard 1x8 | LED id 0x05 |
| `0294` | Razer Ornata V3 X | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 1x1 | Keyboard | LED id 0x05 |
| `02A2` | Razer Ornata V3 X Rev2 | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 1x1 | Keyboard | LED id 0x05 |
| `020F` | Razer Blade (2016) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0224` | Razer Blade (Late 2016) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0270` | Razer Blade 14 (2021) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05, keepalive, layout `razer_laptop_common_layout` |
| `028C` | Razer Blade 14 (2022) | IPU I 2, P `0001`, U `0002` | razer | standard, `1F` | 6x16 | Keyboard 6x16 | LED id 0x00, keepalive, layout `razer_laptop_with_spacebar_layout` |
| `029D` | Razer Blade 14 (2023) | IPU I 2, P `0001`, U `0002` | razer | standard, `1F` | 6x16 | Keyboard 6x16 | LED id 0x00, layout `razer_laptop_with_spacebar_layout` |
| `028A` | Razer Blade 15 (2022) | IPU I 2, P `0001`, U `0002` | razer | standard, `1F` | 6x16 | Keyboard 6x16 | LED id 0x00, layout `razer_blade_15_2022_layout` |
| `0233` | Razer Blade 15 (2018 Advanced) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `023B` | Razer Blade 15 (2018 Base) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0240` | Razer Blade 15 (2018 Mercury) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `023A` | Razer Blade 15 (2019 Advanced) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0246` | Razer Blade 15 (2019 Base) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 1x1 | Keyboard | LED id 0x05 |
| `0245` | Razer Blade 15 (2019 Mercury) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `024D` | Razer Blade 15 (2019 Studio) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0253` | Razer Blade 15 (2020 Advanced) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0255` | Razer Blade 15 (2020 Base) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 1x16 | Keyboard 1x16 | LED id 0x05 |
| `0268` | Razer Blade 15 (Late 2020) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `026D` | Razer Blade 15 (2021 Advanced) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0276` | Razer Blade 15 (Late 2021 Advanced) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05, layout `razer_blade_15_2021_advanced_layout` |
| `026F` | Razer Blade 15 (2021 Base) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 1x16 | Keyboard 1x16 | LED id 0x05 |
| `027A` | Razer Blade 15 (2021 Base) | IPU I 2, P `0001`, U `0002` | razer | standard, `1F` | 1x16 | Keyboard 1x16 | LED id 0x05 |
| `0210` | Razer Blade Pro (2016) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x25 | Keyboard 6x25 | LED id 0x05 |
| `0225` | Razer Blade Pro (2017) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x25 | Keyboard 6x25 | LED id 0x05, layout `razer_blade_pro_2017_layout` |
| `022F` | Razer Blade Pro (2017 FullHD) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x25 | Keyboard 6x25 | LED id 0x05 |
| `0234` | Razer Blade Pro (2019) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `024C` | Razer Blade Pro (Late 2019) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0256` | Razer Blade Pro 17 (2020) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0279` | Razer Blade Pro 17 (2021) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05, layout `razer_blade_17_pro_2021_layout` |
| `0205` | Razer Blade Stealth (2016) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05, layout `razer_blade_stealth_2016_layout` |
| `0220` | Razer Blade Stealth (Late 2016) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `022D` | Razer Blade Stealth (2017) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0232` | Razer Blade Stealth (Late 2017) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `0239` | Razer Blade Stealth (2019) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05 |
| `024A` | Razer Blade Stealth (Late 2019) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 1x1 | Keyboard | LED id 0x05 |
| `0252` | Razer Blade Stealth (2020) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 1x1 | Keyboard | LED id 0x05 |
| `0259` | Razer Blade Stealth (Late 2020) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 1x16 | Keyboard 1x16 | LED id 0x05 |
| `026A` | Razer Book 13 (2020) | IPU I 2, P `0001`, U `0002` | razer | standard, `3F` | 6x16 | Keyboard 6x16 | LED id 0x05, layout `razer_laptop_common_layout` |
| `006A` | Razer Abyssus Elite D.Va Edition | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Logo | LED id 0x05 |
| `006B` | Razer Abyssus Essential | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Logo | LED id 0x05 |
| `0064` | Razer Basilisk | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Logo; Scroll Wheel | LED id 0x05 |
| `0065` | Razer Basilisk Essential | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Logo | LED id 0x05 |
| `0086` | Razer Basilisk Ultimate (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x14 | Scroll Wheel; Logo; Left LED Strip 1x8; Right LED Strip 1x4 | LED id 0x00 |
| `0088` | Razer Basilisk Ultimate (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x14 | Scroll Wheel; Logo; Left LED Strip 1x8; Right LED Strip 1x4 | LED id 0x00 |
| `0085` | Razer Basilisk V2 | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Scroll Wheel; Logo | LED id 0x05 |
| `0099` | Razer Basilisk V3 | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 1x11 | Logo; Scroll Wheel; LED Strip 1x9 | LED id 0x00 |
| `00CB` | Razer Basilisk V3 35K | IPU I 3, P `000C`, U `0001` | razer | extended, `1F` | 1x11 | Logo; Scroll Wheel; LED Strip 1x9 | LED id 0x00 |
| `00AA` | Razer Basilisk V3 Pro (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x13 | Scroll Wheel; Logo; LED Strip 1x11 | LED id 0x00 |
| `00AB` | Razer Basilisk V3 Pro (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x13 | Scroll Wheel; Logo; LED Strip 1x11 | LED id 0x00 |
| `00CC` | Razer Basilisk V3 Pro 35K (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x13 | Logo; Scroll Wheel; LED Strip 1x11 | LED id 0x00 |
| `00CD` | Razer Basilisk V3 Pro 35K (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x13 | Logo; Scroll Wheel; LED Strip 1x11 | LED id 0x00 |
| `00D6` | Razer Basilisk V3 Pro 35K Phantom Green (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x13 | Scroll Wheel; LED Strip 1x11 | LED id 0x00, zones hold 12 of 13 LEDs |
| `00D7` | Razer Basilisk V3 Pro 35K Phantom Green (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x13 | Scroll Wheel; LED Strip 1x11 | LED id 0x00, zones hold 12 of 13 LEDs |
| `00B9` | Razer Basilisk V3 X HyperSpeed | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x1 | Scroll Wheel | LED id 0x00 |
| `00A3` | Razer Cobra | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x1 | Logo | LED id 0x05 |
| `00AF` | Razer Cobra Pro (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x11 | Logo; Scroll Wheel; Underglow 1x9 | LED id 0x00 |
| `00B0` | Razer Cobra Pro (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x11 | Logo; Scroll Wheel; Underglow 1x9 | LED id 0x00 |
| `0043` | Razer Deathadder Chroma | IPU I 0, P `0001`, U `0002` | razer | custom, `1F` | 1x2 | Logo; Scroll Wheel | LED id 0x05 |
| `005C` | Razer Deathadder Elite | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Logo; Scroll Wheel | LED id 0x00 |
| `006E` | Razer Deathadder Essential | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Logo; Scroll Wheel | LED id 0x05 |
| `0098` | Razer Deathadder Essential V2 | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x2 | Logo; Scroll Wheel | LED id 0x00, brightness 0..100 |
| `0071` | Razer Deathadder Essential White Edition | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Logo; Scroll Wheel | LED id 0x05 |
| `0084` | Razer Deathadder V2 | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x2 | Scroll Wheel; Logo | LED id 0x00 |
| `008C` | Razer Deathadder V2 Mini | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x1 | Logo | LED id 0x00 |
| `007C` | Razer Deathadder V2 Pro (Wired) | IPU I 0, P `0001`, U `0002` | recorded | extended, probed `1F`/`3F`/`9F`/`FF` | 1x1 | Logo | Lightloom's recorded profile (see "DeathAdder V2 Pro"), not OpenRGB's tid `3F` / LED id 0x05 |
| `007D` | Razer Deathadder V2 Pro (Wireless) | IPU I 0, P `0001`, U `0002` | recorded | extended, probed `1F`/`3F`/`9F`/`FF` | 1x1 | Logo | Lightloom's recorded profile (see "DeathAdder V2 Pro"), not OpenRGB's tid `3F` / LED id 0x05 |
| `004C` | Razer Diamondback | IPU I 0, P `0001`, U `0002` | razer | linear, `3F` | 1x21 | LED Strip 1x19; Logo; Scroll Wheel | LED id 0x05 |
| `0059` | Razer Lancehead 2017 (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x16 | Right LED Strip 1x7; Left LED Strip 1x7; Logo; Scroll Wheel | LED id 0x05 |
| `005A` | Razer Lancehead 2017 (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x16 | Right LED Strip 1x7; Left LED Strip 1x7; Logo; Scroll Wheel | LED id 0x05 |
| `0070` | Razer Lancehead 2019 (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x16 | Right LED Strip 1x7; Left LED Strip 1x7; Logo; Scroll Wheel | LED id 0x05 |
| `006F` | Razer Lancehead 2019 (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x16 | Right LED Strip 1x7; Left LED Strip 1x7; Logo; Scroll Wheel | LED id 0x05 |
| `0060` | Razer Lancehead Tournament Edition | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x16 | Scroll Wheel; Logo; Right LED Strip 1x7; Left LED Strip 1x7 | LED id 0x05 |
| `0024` | Razer Mamba 2012 (Wired) | IPU I 0, P `0001`, U `0002` | razer | custom, `3F` | 1x15 | Scroll Wheel | LED id 0x05, zones hold 1 of 15 LEDs |
| `0025` | Razer Mamba 2012 (Wireless) | IPU I 0, P `0001`, U `0002` | razer | custom, `3F` | 1x15 | Scroll Wheel | LED id 0x05, zones hold 1 of 15 LEDs |
| `0044` | Razer Mamba 2015 (Wired) | IPU I 0, P `0001`, U `0002` | razer | linear, `3F` | 1x15 | Chroma Zone 1x15 | LED id 0x05 |
| `0045` | Razer Mamba 2015 (Wireless) | IPU I 0, P `0001`, U `0002` | razer | linear, `3F` | 1x15 | Chroma Zone 1x15 | LED id 0x05 |
| `0073` | Razer Mamba 2018 (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Scroll Wheel Zone; Logo Zone | LED id 0x05 |
| `0072` | Razer Mamba 2018 (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Scroll Wheel Zone; Logo Zone | LED id 0x05 |
| `006C` | Razer Mamba Elite | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x20 | Scroll Wheel; Logo; Left LED Strip 1x9; Right LED Strip 1x9 | LED id 0x00 |
| `0069` | Razer Mamba Hyperflux (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x2 | Scroll Wheel; Logo | LED id 0x00 |
| `0046` | Razer Mamba Tournament Edition | IPU I 0, P `0001`, U `0002` | razer | linear, `3F` | 1x16 | Left LED Strip 1x7; Right LED Strip 1x7; Logo; Scroll Wheel | LED id 0x05 |
| `0053` | Razer Naga Chroma | IPU I 0, P `0001`, U `0002` | razer | linear, `3F` | 1x3 | Scroll Wheel; Logo; Numpad | LED id 0x05 |
| `0093` | Razer Naga Classic | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x3 | Logo; Scroll Wheel; Numpad | LED id 0x00 |
| `003E` | Razer Naga Epic Chroma | IPU I 0, P `0001`, U `0002` | razer | custom, `1F` | 1x2 | Scroll Wheel; Numpad | LED id 0x05 |
| `008D` | Razer Naga Left Handed | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x3 | Logo; Scroll Wheel; Numpad | LED id 0x00 |
| `0050` | Razer Naga Hex V2 | IPU I 0, P `0001`, U `0002` | razer | standard, `3F` | 1x3 | Logo; Scroll Wheel; Numpad | LED id 0x05 |
| `0067` | Razer Naga Trinity | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x3 | Scroll Wheel; Logo; Numpad | LED id 0x05 |
| `008F` | Razer Naga Pro (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x3 | Scroll Wheel; Logo; Numpad | LED id 0x05 |
| `0090` | Razer Naga Pro (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x3 | Scroll Wheel; Logo; Numpad | LED id 0x05 |
| `00A7` | Razer Naga Pro V2 (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x2 | Logo; Numpad | LED id 0x00 |
| `00A8` | Razer Naga Pro V2 (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x2 | Logo; Numpad | LED id 0x00 |
| `00E7` | Razer Naga Pro V3 (Wired) | PU P `0001`, U `0003` | razer | extended, `1F` | 1x3 | Logo; Scroll Wheel; Numpad | LED id 0x00 |
| `00E8` | Razer Naga Pro V3 (Wireless) | PU P `0001`, U `0003` | razer | extended, `1F` | 1x3 | Logo; Scroll Wheel; Numpad | LED id 0x00 |
| `0078` | Razer Viper | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Logo | LED id 0x05 |
| `0091` | Razer Viper 8kHz | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x1 | Logo | LED id 0x05 |
| `008A` | Razer Viper Mini | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Logo | LED id 0x05 |
| `007A` | Razer Viper Ultimate (Wired) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Logo | LED id 0x05 |
| `007B` | Razer Viper Ultimate (Wireless) | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Logo | LED id 0x05 |
| `0207` | Razer Orbweaver Chroma | IPU I 2, P `0001`, U `0002` | razer | custom, `3F` | 4x5 | Keypad 4x5 | LED id 0x05 |
| `0208` | Razer Tartarus Chroma | IPU I 2, P `0001`, U `0002` | razer | custom, `1F` | 1x1 | Keypad | LED id 0x05 |
| `0244` | Razer Tartarus Pro | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 1x21 | Keypad 4x5; Keypad LED 20 | LED id 0x00 |
| `022B` | Razer Tartarus V2 | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 4x6 | Keypad 4x6 | LED id 0x00, layout `razer_tartarus_v2_layout` |
| `0F19` | Razer Kraken Kitty Edition | IPU I 1, P `0001`, U `0003` | razer | extended, `1F` | 1x4 | Headset 1x4 | LED id 0x05 |
| `0F21` | Razer Kraken Kitty Black Edition | IPU I 1, P `0001`, U `0003` | razer | extended, `3F` | 1x12 | LED Strip 1x12 | LED id 0x05 |
| `056B` | Razer Kraken V4 (Wired) | IPU I 5, P `FF14`, U `0001` | kraken-v4 | extended, `60` | 1x9 | Headset 1x9 | LED id 0x05 |
| `056C` | Razer Kraken V4 (Wireless) | IPU I 5, P `FF14`, U `0001` | kraken-v4 | extended, `60` | 1x9 | Headset 1x9 | LED id 0x05, wireless flag 0x80 |
| `0587` | Razer Kraken Kitty V3 Pro (Wired) | IPU I 5, P `FF14`, U `0001` | kraken-v4 | extended, `60` | 1x10 | Headset 1x9; Ears | LED id 0x05 |
| `0588` | Razer Kraken Kitty V3 Pro (Wireless) | IPU I 5, P `FF14`, U `0001` | kraken-v4 | extended, `60` | 1x10 | Headset 1x9; Ears | LED id 0x05, wireless flag 0x80 |
| `0F03` | Razer Tiamat 7.1 V2 | I I 0 | razer | extended, `3F` | 1x17 | Controller 1x15; Headset Left; Headset Right | LED id 0x05 |
| `0C00` | Razer Firefly | IPU I 0, P `0001`, U `0002` | razer | linear, `3F` | 1x15 | LED Strip 1x15 | LED id 0x05 |
| `0C04` | Razer Firefly V2 | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x19 | LED Strip 1x19 | LED id 0x00 |
| `0C08` | Razer Firefly V2 Pro | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x17 | LED Strip 1x17 | LED id 0x00 |
| `0068` | Razer Firefly Hyperflux | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x14 | Scroll Wheel; Logo; Mousemat 1x12 | LED id 0x00, software mode |
| `0C01` | Razer Goliathus | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | LED Strip | LED id 0x00 |
| `0C06` | Razer Goliathus Chroma 3XL | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x1 | LED Strip | LED id 0x00 |
| `0C02` | Razer Goliathus Extended | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | LED Strip | LED id 0x00 |
| `0C05` | Razer Strider Chroma | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x19 | LED Strip 1x19 | LED id 0x00 |
| `0F08` | Razer Base Station Chroma | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x15 | LED Strip 1x15 | LED id 0x00 |
| `0F20` | Razer Base Station V2 Chroma | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x8 | LED Strip 1x8 | LED id 0x00 |
| `0F26` | Razer Charging Pad Chroma | IPU I 0, P `000C`, U `0001` | razer | extended, `1F` | 1x10 | LED Strip 1x10 | LED id 0x00, software mode |
| `0F1F` | Razer Chroma Addressable RGB Controller | I I 0 | razer-argb | extended ARGB, `3F` | 6x80 | Channel 1 1x80; Channel 2 1x80; Channel 3 1x80; Channel 4 1x80; Channel 5 1x80; Channel 6 1x80 | LED id 0x05 |
| `0F09` | Razer Chroma HDK | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 4x16 | Channel 1 1x16; Channel 2 1x16; Channel 3 1x16; Channel 4 1x16 | LED id 0x00 |
| `0F07` | Razer Chroma Mug Holder | IPU I 0, P `0001`, U `0002` | razer | linear, `3F` | 1x15 | LED Strip 1x15 | LED id 0x05, software mode |
| `0F0E` | Razer Chroma PC Case Lighting Kit | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 2x16 | Channel 1 1x16; Channel 2 1x16 | LED id 0x05 |
| `0215` | Razer Core | IPU I 0, P `FF00`, U `0001` | razer | standard, `3F` | 1x9 | Side Window Lights; LED Strip 1x8 | LED id 0x05 |
| `0F1A` | Razer Core X | IPU I 2, P `0001`, U `0002` | razer | extended, `3F` | 1x16 | Side Window Lights; LED Strip 1x15 | LED id 0x00 |
| `0F0D` | Razer Laptop Stand Chroma | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x15 | LED Strip 1x15 | LED id 0x00 |
| `0F2B` | Razer Laptop Stand Chroma V2 | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x15 | LED Strip 1x15 | LED id 0x00 |
| `0532` | Razer Leviathan V2 | IPU I 2, P `000C`, U `0001` | razer | extended, `1F` | 2x9 | Speaker Underglow 2x9 | report id 0x07, LED id 0x00 |
| `054A` | Razer Leviathan V2 X | IPU I 0, P `000C`, U `0001` | razer | extended, `1F` | 1x14 | Speaker Underglow 1x14 | report id 0x07, LED id 0x00 |
| `0F1D` | Razer Mouse Bungee V3 Chroma | IPU I 0, P `0001`, U `0002` | razer | extended, `1F` | 1x8 | LED Strip 1x8 | LED id 0x00 |
| `007E` | Razer Mouse Dock Chroma | IPU I 0, P `0001`, U `0002` | razer | extended, `3F` | 1x1 | Base | LED id 0x05 |
| `00A4` | Razer Mouse Dock Pro | IPU I 0, P `0001`, U `0002` | razer | extended, `FF` | 1x9 | Base 1x9 | LED id 0x00 |
| `0F13` | Lian Li O11 Dynamic - Razer Edition | IPU I 2, P `0001`, U `0002` | razer | extended, `1F` | 4x16 | Case LEDs 4x16 | LED id 0x00 |
| `0F1B` | Razer Seiren Emote | PU P `000C`, U `0001` | razer | extended, `3F` | 4x16 | 8-Bit LED Matrix 8x8 | LED id 0x05 |
| `0F21` | Razer Thunderbolt 4 Dock Chroma | PU P `000C`, U `0001` | razer | extended, `3F` | 1x12 | LED Strip 1x12 | LED id 0x05 |
| `0F52` | Razer Thunderbolt 5 Dock Chroma | PU P `000C`, U `0001` | razer | extended, `3F` | 1x12 | LED Strip 1x12 | LED id 0x05 |
| `0517` | Razer Nommo Chroma | IPU I 1, P `0001`, U `0003` | razer | extended, `3F` | 2x24 | Right Speaker 1x24; Left Speaker 1x24 | LED id 0x05 |
| `0518` | Razer Nommo Pro | IPU I 1, P `0001`, U `0003` | razer | extended, `3F` | 2x8 | Left Speaker 1x8; Right Speaker 1x8 | LED id 0x05 |

Table entries without a detector line (reached only as a receiver's paired device):

| PID | OpenRGB table name | Matrix, tid | Grid | Zones |
|-----|------|------|------|------|
| `025B` | Razer BlackWidow V3 Pro (Bluetooth) | extended, `3F` | 6x22 | Keyboard 6x22 |
| `00D8` | Razer Basilisk V3 Pro 35K Phantom Green Edition (Bluetooth) | extended, `1F` | 1x13 | Scroll Wheel; LED Strip 1x11 |
| `00AC` | Razer Basilisk V3 Pro (Bluetooth) | extended, `1F` | 1x13 | Scroll Wheel; Logo; LED Strip 1x11 |
