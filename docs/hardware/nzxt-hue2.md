<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# NZXT Hue 2 family (`1E71`): per-LED lighting, adapted from OpenRGB

The plugin is `plugins/nzxt-hue2/`. It drives the LEDs of NZXT's "Hue 2" generation of
USB lighting controllers in per-LED ("Direct") mode: Hue 2 and Hue 2 Ambient, the RGB & Fan
Controllers, the RGB Controllers, the Smart Device V2, the NZXT USB lighting controllers of
Hue 2 / B650E motherboards, and the Kraken X3 and Kraken 2024 Elite RGB coolers. It never
changes fan or pump speeds and sends no hardware effect.

## Tested on hardware

2026-10-22, on the user's NZXT Kraken X3 (`1E71:2007`): the plugin (run from a copy without the
`ProductId()` filter that then left `2007` to the single-device plugin) lit the pump ring and the
logo red, green and blue in turn, with 0 errors. That confirms the OpenRGB logo path (zone 2,
`22 10 04` + apply `22 A0 04`), which had never been seen on this cooler before (see Untested).
This plugin now owns `2007`; the single-device plugin `nzxt-kraken-x3` was retired the same day
(its recording-based protocol notes stay in `docs/hardware/nzxt-kraken-x3.md`, its recording in
`fixtures/recorded-kraken-x3.json`). `2007` is marked tested in `manifest.json`; every other
product id is not. NZXT CAM (`NZXT CAM.exe`) is reported as a conflicting process, as it was for
the single-device plugin.

## Sources

| # | Source | License | Used for |
|---|--------|---------|----------|
| S1 | OpenRGB `Controllers/NZXTHue2Controller/NZXTHue2ControllerDetect.cpp` | GPL-2.0-or-later | Product ids, product names, RGB channel count per product, detector macro (no interface / usage page / usage) |
| S2 | OpenRGB `Controllers/NZXTHue2Controller/NZXTHue2Controller.cpp` and `NZXTHue2Controller.h` | GPL-2.0-or-later | Every report: firmware request, device-list request and reply layout, accessory LED counts, direct and apply reports, reply-wait bounds |
| S3 | OpenRGB `Controllers/NZXTHue2Controller/RGBController_NZXTHue2.cpp` and `RGBController_NZXTHue2.h` | GPL-2.0-or-later | Zones (one linear zone per RGB channel, 0..40 LEDs), zone and LED names, accessory names, the order reports go out in (`DeviceUpdateLEDs`) |
| S4 | OpenRGB `DetectionManager.cpp`, `RGBController/RGBController.cpp`, `RGBController/RGBControllerInterface.h` | GPL-2.0-or-later | What `REGISTER_HID_DETECTOR` opens; how zones and colours are laid out; `RGBColor` byte order. Read only, no code taken |
| S5 | HueDeck's recordings of the user's Kraken X3 `1E71:2007` (`huedeck light #00ff00 --record` and `huedeck light #ff8000 ... --record`, 2026-10-18, another plugin driving the device) | data | The collection (interface 0, usage page `0xFF00`, usage `0x01`, collection 0, 64-byte input/output/feature reports) and the reports the device accepted |

All OpenRGB files are at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every one of S1-S4 carries
`SPDX-License-Identifier: GPL-2.0-or-later`; the NZXT Hue 2 files name
`Adam Honse (calcprogrammer1@gmail.com)      29 Dec 2019`. `plugin.js` keeps that credit.

## Products

All 21 ids OpenRGB's detector registers (S1), vendor `0x1E71`. "Channels" is the RGB channel
count the detector hands to the controller; OpenRGB makes one zone per channel. The fan
channel counts only feed OpenRGB's fan code, which is not ported.

| PID | OpenRGB name | Detector | Channels | HueDeck layout |
|-----|--------------|----------|----------|----------------|
| `2009`, `2010`, `200E` | NZXT RGB & Fan Controller | `DetectNZXTFanController` | 2 | `Channel 1`, `Channel 2` |
| `2011`, `2019`, `2020`, `201F` | NZXT RGB & Fan Controller | `DetectNZXTFanController6Channel` | 6 | `Channel 1` .. `Channel 6` |
| `2022` | NZXT RGB & Fan Controller 2024 | `DetectNZXTFanController6Channel` | 6 | `Channel 1` .. `Channel 6` |
| `201B` | NZXT B650E Motherboard | `DetectNZXTFanController6Channel` | 6 | `Channel 1` .. `Channel 6` |
| `2001` | NZXT Hue 2 | `DetectNZXTHue2` | 4 | `Channel 1` .. `Channel 4` |
| `2002` | NZXT Hue 2 Ambient | `DetectNZXTHue2Ambient` | 2 | `Channel 1`, `Channel 2` |
| `2005`, `200B` | NZXT Hue 2 Motherboard | `DetectNZXTHue2Motherboard` | 2 | `Channel 1`, `Channel 2` |
| `2007` | NZXT Kraken X3 Series | `DetectNZXTKrakenX3` | 3 | `Channel 1` (RGB header), ring and logo on the device grid |
| `2014` | NZXT Kraken X3 Series RGB | `DetectNZXTKrakenX3` | 3 | as `2007` |
| `3012` | NZXT Kraken 2024 ELITE Series RGB | `DetectNZXTKrakenElite` | 2 | `Channel 1`, `Channel 2` |
| `2012`, `2021` | NZXT RGB Controller | `DetectNZXTRGBController` | 3 | `Channel 1` .. `Channel 3` |
| `2006`, `200D`, `200F` | NZXT Smart Device V2 | `DetectNZXTSmartDeviceV2` | 2 | `Channel 1`, `Channel 2` |

The Kraken 2024 Elite's detector also sets OpenRGB's "2023 effects" flag; it changes only the
hardware-effect report, which is not ported, so its per-LED reports are the same as everyone
else's. The Kraken Z3 and the 2023 Kraken / Kraken Elite are not part of this OpenRGB family
(its detector lists only the 21 ids above, and no other OpenRGB controller at this commit
mentions them), so they are not covered.

## Transport

OpenRGB registers every id with `REGISTER_HID_DETECTOR`, which has no interface, usage page or
usage (`HID_INTERFACE_ANY`, `HID_USAGE_PAGE_ANY`, `HID_USAGE_ANY`): it runs the detector on every
HID collection of a matching device (S4), so on a device with several collections one of the
controllers it creates ends up on the right one. HueDeck does not work that way: the host calls
`Validate()` for every collection and opens **one** plugin instance on the first collection it
accepts. Accepting every collection could therefore bind the plugin to a standard collection
(keyboard, consumer control, ...) where the `22 xx` reports fail. `Validate()` accepts only
vendor-defined collections (usage page `0xFF00`..`0xFFFF`), and the manifest has no interface
filter. The only device known first hand (S5, `1E71:2007`) has a single collection of that kind:
interface 0, usage page `0xFF00`, usage `0x0001`, collection 0, with 64-byte input, output and
feature reports. The other products' descriptors are not known here (unverified: a product whose
lighting collection is not vendor-defined would not be bound at all; nothing would be sent to it).

Every request is a 64-byte **output report** (`hid_write(dev, usb_buf, 64)`, `device.write`),
zero-filled, the first byte being the report id. Replies are 64-byte input reports.

## Reports

| Report | Bytes (hex; the rest of the 64 is zero) | Source |
|--------|------------------------------------------|--------|
| firmware request | `10 01` | `SendFirmwareRequest` |
| firmware reply | `11 01 ...`, version = bytes `0x11`, `0x12`, `0x13` as `a.b.c` | `SendFirmwareRequest` |
| device-list request | `20 03` | `UpdateDeviceList` |
| device-list reply | `21 03 ...`, accessory ids of channel `c` (0-based) at bytes `0x0F + 6c` .. `0x0F + 6c + 5` | `UpdateDeviceList` |
| colours, group 0 | `22 10 <1<<c> 00` + LEDs 1..20 of channel `c`, 3 bytes each: **G, R, B** | `SendDirect(c, 0, ...)` |
| colours, group 1 | `22 11 <1<<c> 00` + LEDs 21..40, only when the channel has more than 20 LEDs | `SendDirect(c, 1, ...)` |
| apply | `22 A0 <1<<c> 00 01 00 00 28 00 00 80 00 32 00 00 01` | `SendApply` |

`<1<<c>` is the channel bit: `01`, `02`, `04`, `08`, `10`, `20` for channels 1..6.

Accessory ids in the device-list reply (LEDs from `UpdateDeviceList`, names from
`SetupZones`); any other id counts 0 LEDs:

| Id | LEDs | Accessory | Id | LEDs | Accessory |
|----|------|-----------|----|------|-----------|
| `01` | 10 | Hue 1 strip | `11` | 1 | Kraken X3 logo |
| `02` | 8 | Aer 1 fan | `13` | 18 | F120 fan (120mm) |
| `04` | 10 | Hue 2 strip (10 LEDs) | `14` | 18 | F140 fan (140mm) |
| `05` | 8 | Hue 2 strip (8 LEDs) | `15` | 20 | F120 Duo fan (120mm) |
| `06` | 6 | Hue 2 strip (6 LEDs) | `16` | 20 | F140 Duo fan (140mm) |
| `08` | 14 | Hue 2 Cable Comb (14 LEDs) | `17` | 8 | F120 Core fan (120mm) |
| `09` | 15 | Hue 2 Underglow (300mm) (15 LEDs) | `18` | 8 | F140 Core fan (140mm) |
| `0A` | 10 | Hue 2 Underglow (200mm) (10 LEDs) | `19` | 8 | F120 Core fan case version (120mm) |
| `0B` | 8 | Aer 2 fan (120mm) | `1C` | 16 | F280 Core fan case version (280mm) |
| `0C` | 8 | Aer 2 fan (140mm) | `1D` | 24 | F360 Core fan case version (360mm) |
| `10` | 8 | Kraken X3 ring | `1E` | 24 | Kraken Elite Ring |
| | | | `1F` | 24 | F420 Core fan case version |

## What the plugin sends

**Initialize.** What OpenRGB's controller constructor sends, both read-only: the firmware
request and the device-list request, each followed by a wait for its reply (the version is
logged; the accessories found on each channel are logged too). When the device sent no input
report at all while the firmware request was waited for, the device-list request is still sent
but not waited for, so a silent device delays the start by about 1 s instead of 2 s. OpenRGB's `UpdateStatus()` only
reads fan status and is disabled in OpenRGB (`if(false)`); it is not ported. Entering Direct
mode sends nothing else in OpenRGB (`DeviceUpdateMode` just sends the colours).

**Every frame** (`DeviceUpdateLEDs` -> `SetChannelLEDs` for each zone in order): for every
channel `c`, the group 0 colour report, the group 1 report if the channel has more than 20
LEDs, then the apply report. A channel with no LEDs still gets an empty group 0 report and the
apply report, as OpenRGB sends for a zone of 0 LEDs. A frame identical to the last one sent is
skipped until 1 s has passed (OpenRGB itself sends only when colours change).

**Shutdown** (both `Shutdown(false)` and `Shutdown(true)`): the same reports with every LED
black. OpenRGB has no exit step. The protocol has hardware effects, but no way to read back the
one the device showed before HueDeck started, so there is nothing to restore. OpenRGB also marks
this family `@save :robot:` (S3), which in OpenRGB's supported-devices legend means saving
happens automatically (the legend is not part of the checkout; not verified on hardware), so a
hardware effect sent on exit could become the device's remembered state. Black uses only the
reports every frame already uses.

## HueDeck mapping

- **Kraken X3 (`2007`, `2014`).** OpenRGB's zone 0 "RGB Header" is the HueDeck channel
  `Channel 1` (the name the existing HueDeck and SignalRGB setups of this cooler use, so a
  component setup carries over). Zone 1 "Ring" and zone 2 "Logo" are fixed parts of the pump
  (accessory ids `10` and `11`, 8 LEDs and 1 LED), so they are LEDs of the device itself: a
  5x5 grid with "Ring LED 1" .. "Ring LED 8" on a circle, clockwise from the top, at `(3,0)`,
  `(4,1)`, `(4,3)`, `(3,4)`, `(1,4)`, `(0,3)`, `(0,1)`, `(1,0)` (the layout of the retired
  `nzxt-kraken-x3` plugin, `docs/hardware/nzxt-kraken-x3.md`), and "Logo LED" in the centre at `(2,2)`. OpenRGB's zones are linear;
  only the placement differs, the LED order on the wire is OpenRGB's.
- **Kraken 2024 Elite (`3012`).** OpenRGB has no fixed zone for it: its 24-LED ring is an
  accessory (`1E`) detected on RGB channel 1 like any fan or strip, so HueDeck exposes it as
  `Channel 1` (default LED count 24 when the device-list reply reports it).
- **Every other product.** Zone `i` is the HueDeck channel `Channel <i+1>` (OpenRGB names it
  "Hue 2 Channel <i+1>"); the device has no LEDs of its own (1x1 grid, no LEDs).
- **LED counts.** OpenRGB sizes each zone with the LEDs the controller reports in the
  device-list reply. HueDeck takes the LED count of a channel from the user's components
  (`device.channel(name)`), at most 40 (the zones' `leds_max`, and what the two colour reports
  can carry).
  When the device-list reply arrived, its count (capped at 40) is the default LED count of a
  channel that has no components yet (`addChannel(name, 40, count)`); without a reply the host's
  default applies. `SetLedLimit` is 40 per channel.
- **Device type and name.** OpenRGB calls every product `DEVICE_TYPE_LEDSTRIP`; HueDeck has no
  LED strip type, so the plugin says `lightingcontroller`. `DeviceType()` is static metadata for
  the whole family, which is mostly controllers, so the Kraken coolers are not typed `aio`. `Initialize()` names the device with
  the detector's name for its product id (table above).

## Deliberate differences from OpenRGB

- **Waiting for replies.** OpenRGB reads the device-list reply with a 1000 ms timeout per read
  and gives up after 10 reads, so it can wait up to 10 x 1000 ms; it waits for the firmware reply
  with no bound at all. HueDeck bounds both more tightly: at most 10 input reports that are not
  the reply, and at most 1000 ms in total without any input report, polled with a non-blocking
  read every 10 ms (the device-list wait is skipped when the device stayed silent through the
  firmware wait). A device-list reply later than that only costs the default LED counts (the
  host default applies instead). A device that does not
  answer is still lit. The probes run in `Initialize()`, never per frame. When the device-list
  reply never comes, OpenRGB parses whatever its buffer last held (zeros, or the last unrelated
  input report) as accessory ids; HueDeck treats it as "no reply" and detects nothing.
- **More than 40 LEDs.** OpenRGB keeps whatever the controller detected as the zone size; above
  40 LEDs (for example three F120 RGB fans, 54 LEDs) `SetChannelLEDs` would write past its
  120-byte colour buffer. HueDeck sends at most 40 LEDs per channel.
- **Kraken X3 ring and logo sizes** are fixed at 8 and 1 instead of being taken from the
  device-list reply (OpenRGB's own table gives those sizes for the ids the X3 reports there).
- **Frame skipping and shutdown** as described above.

## Left out

- Hardware effects and modes (`SetChannelEffect`: report `28 03` and, for the Kraken 2024
  Elite, `2A 04`): HueDeck lights per LED from the canvas.
- Fan control (`SendFan`, report `62 01`) and fan status (`UpdateStatus`): HueDeck never
  changes fan or pump speeds. No report the plugin sends carries a fan or pump field.
- The serial number string (`GetSerialString`): only informational in OpenRGB.
- There is no save / flash / profile command in this OpenRGB controller, so none had to be
  left out; the Direct-mode reports are the ones OpenRGB streams on every update.

## Untested

Only the Kraken X3 (`2007`) has run with this plugin on hardware (see "Tested on hardware");
every other product id is untested. Before that test, what was known first hand was that the
user's Kraken X3 accepted, from another plugin, header and ring reports byte-identical to the
ones this plugin sends (S5, see Replay); the logo had only ever been seen lit through `2A 04`
(S5), and the OpenRGB way this plugin uses (zone 2, `22 10 04` + `22 A0 04`) was checked on the
cooler on 2026-10-22. Still never seen on real hardware: a group 1 report with colours in it
(a channel with more than 20 LEDs) and any product other than the Kraken X3.

Ownership: the single-device plugin `plugins/nzxt-kraken-x3/` also claimed `1E71:2007` until it
was retired on 2026-10-22; this plugin is now the only one that claims it.

## Replay (`huedeck compat replay`, 2026-10-18, re-run 2026-10-19)

Both recordings of the user's Kraken X3 (S5) were replayed against this plugin. The tool
reports the first difference per collection (packet #0: the firmware request, which the
recording's plugin did not send); the full comparison of the replayed traces:

| Recording | Recorded | Replayed | Differences, all explained |
|-----------|----------|----------|----------------------------|
| `trace_1e71_2007.json`, `#00ff00`, 232 frames | 1631 reports | 14 reports | start: `10 01`, `20 03` (no reply in the trace: the firmware request polled 100 x 10 ms, the device-list request read once as the device stayed silent; first frame at 1.04 s); 1 colour frame sent, the other 231 identical frames skipped (the replay renders faster than 1 frame/s); per frame `22 10 01`, `22 A0 01`, `22 10 02`, `22 A0 02` byte-identical; the recording also sent empty `22 11 01` / `22 11 02` reports (OpenRGB sends group 1 only above 20 LEDs) and the logo as `2A 04 04 04 00 32 00 G R B ... 01 00 01 03`, where OpenRGB sends `22 10 04 00 G R B` + apply `22 A0 04`; shutdown: the same four reports black and identical, logo black as zone 2, zone order header-ring-logo instead of ring-header-logo |
| `trace_multi.json`, `#ff8000`, 400 frames (with the MSI and Razer plugins replaying their own devices) | 2807 reports | 14 reports | the same, with orange (`80 FF 00` as G R B) |
