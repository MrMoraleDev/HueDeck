<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ASUS Aura USB: motherboard and addressable controllers (`0B05`)

Plugin: `plugins/asus-aura-usb/`. Untested on hardware (`tested` is empty).

It drives, per LED, the ASUS "AURA LED Controller" USB device of motherboards (onboard LEDs
and 12 V RGB headers as one row on the device grid, every addressable header as a Lightloom
channel), the older "Aura Addressable" header controllers, the ROG Strix LC III cooler (which
OpenRGB drives with the mainboard protocol) and the ROG AURA Terminal. It never sends a hardware
effect, the commit (save) command or the "shutdown effect" variants.

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB). Every file below carries
`SPDX-License-Identifier: GPL-2.0-or-later`.

| File (under `Controllers/AsusAuraUSBController/`) | License | Authors (header) | Taken |
|---|---|---|---|
| `AsusAuraUSBControllerDetect.cpp` | GPL-2.0-or-later | none named | Product ids, names, detector macros (Validate), which controller class each id gets |
| `AsusAuraUSBController/AsusAuraUSBController.cpp`, `.h` | GPL-2.0-or-later | Martin Hartl (inlart), 25 Apr 2020 | Report layout (`0xEC`, 65 bytes), firmware and config-table requests and reply checks, `SendDirect` (20 LEDs per report, apply flag, 8-bit overflow flag) |
| `AsusAuraUSBController/AsusAuraMainboardController.cpp`, `.h` | GPL-2.0-or-later | Martin Hartl (inlart), 25 Apr 2020 | Zones from the config table (bytes 0x02, 0x1B, 0x1D), `SetGen1`, `SendEffect` (Direct). The commit and `SendColor` are read but not used |
| `AsusAuraUSBController/AsusAuraAddressableController.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 18 Jan 2020 | Zones from the config table (byte 0x02), `SendEffect` (Direct) |
| `AsusAuraUSBController/RGBController_AsusAuraUSB.cpp`, `.h` | GPL-2.0-or-later | Adam Honse (CalcProgrammer1), 18 Jan 2020 | Zone and LED names, `AURA_ADDRESSABLE_MAX_LEDS` (500), update order (`DeviceUpdateMode` before the first `DeviceUpdateLEDs`) |
| `AsusAuraUSBController/RGBController_AsusAuraMainboard.cpp` | GPL-2.0-or-later | rytypete, 30 Aug 2023 | What the save path does (`DeviceSaveMode`: shutdown effect + commit), to leave it out |
| `DetectionManager.h`, `.cpp` | GPL-2.0-or-later | | Detector macro semantics. Read only |

## Products

| PID | OpenRGB name | Controller | Detector macro | Lightloom Validate |
|---|---|---|---|---|
| `1867`, `1872`, `18A3`, `18A5` | ASUS Aura Addressable (OpenRGB appends the DMI board name) | addressable | `REGISTER_HID_DETECTOR` | vendor-defined usage page (0xFF00..0xFFFF) |
| `1B29` | ASUS ROG Strix LC III | mainboard | `REGISTER_HID_DETECTOR` | vendor-defined usage page |
| `18F3`, `1939`, `1AA6`, `1BED` | ASUS Aura Motherboard (OpenRGB: "ASUS " + DMI board name) | mainboard | `REGISTER_HID_DETECTOR` | vendor-defined usage page |
| `19AF` | ASUS Aura Motherboard | mainboard | `REGISTER_HID_DETECTOR_PU(0xFF72, 0x00A1)` | usage page 0xFF72, usage 0x00A1 |
| `1889` | ASUS ROG AURA Terminal | addressable | `REGISTER_HID_DETECTOR` | vendor-defined usage page |

`REGISTER_HID_DETECTOR` runs OpenRGB's detector on every HID collection; Lightloom binds a plugin
to one collection, so it takes the vendor-defined one (where the `0xEC` reports go). The
descriptors of these devices were not seen by Lightloom.

The other ids of OpenRGB's `AsusAuraUSBControllerDetect.cpp` (keyboards, mice, mousemats,
monitors, headset stands, Ryuo / Ryujin AIO, Strix LC, ROG Ally) use other protocols; see the
other `asus-aura-*` notes for the ones ported.

## Protocol

Every report is a 65-byte output report (`hid_write`), report id `0xEC` first, zero-padded.

**Start** (`AuraUSBController` constructor):

1. `EC 82`: firmware request. Reply (input report) `EC 02 <16-byte version string>`. Lightloom logs
   it; a missing reply is not an error (as in OpenRGB).
2. `EC B0`: configuration-table request. Reply `EC 30 xx xx <60-byte table>` (table = reply bytes
   4..63). Without it OpenRGB does not add the device; Lightloom then sends nothing more.
   OpenRGB's `hid_read` blocks until a report arrives and takes the first one; Lightloom polls
   (non-blocking reads every 10 ms, at most 1 s without a report, at most 10 reports that are not
   the reply) and takes the first report whose byte 1 is the reply id.
3. Mainboard controllers only (`SetGen1`): `EC 52 53 00 01`. OpenRGB sends it on every start and
   does not explain it; Lightloom sends it the same way.

**Zones** (from the table):

- Mainboard controllers: `total = table[0x1B]` onboard LEDs, of which the last
  `table[0x1D]` are 12 V RGB headers (0 when `total < table[0x1D]`); if `total > 0` this is zone
  0 ("Aura Mainboard, LED n" then "Aura Mainboard, RGB Header n"), effect channel 0, direct
  channel `0x04`. Then `table[0x02]` addressable headers, effect channels counting on, direct
  channel = header index (0-based).
- Addressable controllers: `table[0x02]` headers; effect channel and direct channel = header
  index.
- Lightloom: the onboard zone is a row of the device grid (`[total, 1]`); each addressable header
  is a channel "Addressable RGB Header n" with up to 500 LEDs (OpenRGB's `leds_max`; its user-set
  zone size is Lightloom's channel LED count).

**Direct effect**, once per zone that has LEDs, before the first colours (OpenRGB's
`DeviceUpdateMode`, run by the first `DeviceUpdateLEDs`):

- mainboard: `EC 35 <effect channel> 00 00 FF` (byte 4 = 0: not the shutdown effect);
- addressable: `EC 3B <header index> 00 FF 00 00 00`.

OpenRGB sends it once for the zones that have LEDs at that time. A Lightloom channel can get LEDs
later, so Lightloom sends it for a zone the first time it has LEDs.

**Every frame** (`SendDirect(direct channel, colours)`, zones in order, zones without LEDs send
nothing): `EC 40 <command> <offset> <count> R G B ...`, at most 20 (0x14) LEDs per report. The
last report of a zone has `command |= 0x80` (apply). Once more than 255 LEDs have been sent,
`command |= 0x10` and the offset continues from `sent - 256` (unsigned char arithmetic, kept
exactly). Lightloom skips a frame whose reports did not change, resending them after 1 s.

**Shutdown.** OpenRGB has no exit step. The board's own effect lives in its saved settings,
which cannot be read back over this protocol and which only the commit command writes, so
Lightloom sets every zone it switched to Direct to black (same reports as a frame), on exit and
on suspend. A zone Lightloom never switched to Direct is left alone. After a power cycle the board
shows its saved effect again.

## Safety: left out

- `SendCommit` (`EC 3F 55`): commits (saves) the current settings; OpenRGB sends it only from
  `DeviceSaveMode`. Never sent.
- The "shutdown effect" (`EC 35 ... 01 <mode>` and `EC 36 <mask> 01 ...`, what the board shows
  while the PC is off): only part of OpenRGB's save path. Never sent.
- Every hardware effect (`EC 35` / `EC 3B` with a mode other than `FF`, `EC 36` effect colours).
- Nothing in this protocol touches fans or other non-lighting settings.

## Untested

Every product id. Open questions for the first hardware test: which HID collection each id
exposes (Lightloom assumes a vendor-defined one), and what `EC 52 53 00 01` does (OpenRGB sends it
on every start).
