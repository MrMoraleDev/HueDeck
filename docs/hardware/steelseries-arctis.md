<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# SteelSeries Arctis 5 (OpenRGB protocol): ear-cup lights

Plugin: `plugins/steelseries-arctis/`. Untested on hardware (`tested` is empty). It claims
`1038:1250` only.

**`1038:12AA` belongs to `plugins/steelseries-arctis-5/`** (HueDeck's own plugin, written from
recordings and confirmed on the reference headset, see
[steelseries-arctis-5.md](steelseries-arctis-5.md)). OpenRGB lists 12AA too, but its stream
differs from the recorded working one (see "Replay of the recorded trace"), and the host's plugin
index lets the last scanned plugin win a VID:PID (`crates/huedeck-plugin-host/src/loader.rs`,
`PluginIndex::insert`), so a second claim would make the choice depend on scan order. This plugin
therefore leaves 12AA out of `ProductId()` and the manifest; `fixtures/expected.json` keeps it
with a `held_back` reason and a test checks that it is not claimed. Revisit only if the OpenRGB
form of the last report (`06 8A 42 00 20 05`) is shown to work on a 12AA.

## Sources

OpenRGB commit `d4798c120659ec5ba2e664a9e87bedff8d36f880`
(https://gitlab.com/CalcProgrammer1/OpenRGB), all `SPDX-License-Identifier: GPL-2.0-or-later`:

- `Controllers/SteelSeriesController/SteelSeriesArctis5Controller/SteelSeriesArctis5Controller.cpp`, `.h` (Morgan Guimard, 04 Mar 2022)
- `Controllers/SteelSeriesController/SteelSeriesArctis5Controller/RGBController_SteelSeriesArctis5.cpp`, `.h` (Morgan Guimard, 04 Mar 2022)
- `Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp`, `SteelSeriesDevices.h`

## Products

| Product id | OpenRGB name | Endpoint (OpenRGB detector) | Detect.cpp line |
|---|---|---|---|
| `1038:1250` | SteelSeries Arctis 5 | interface 5, any collection (`REGISTER_HID_DETECTOR_I`) | 524 |
| `1038:12AA` | SteelSeries Arctis 5 (`STEELSERIES_ARCTIS_5_V2_PID`): **held back**, owned by `steelseries-arctis-5` | interface 5, any collection | 525 |

## Protocol

Output reports (`hid_write`), report id `0x06`, 37 bytes, zero padded. Nothing is read.

LEDs: zone 0 "Left", zone 1 "Right", one LED each (grid `[2, 1]`: Left at (0,0), Right at
(1,0)).

**Start.** OpenRGB's only mode, Direct, has an empty `DeviceUpdateMode`: nothing is sent.

**Every frame** (`DeviceUpdateLEDs`): zone 0 then zone 1, each `SetColor(zone, colour)`:

| # | Bytes (hex) | OpenRGB comment |
|---|---|---|
| 1 | `06 81 43 01 22` | "Two packets are sent before a color change" |
| 2 | `06 81 43 01 23` | |
| 3 | `06 8A 42 00 20 41 00 R G B FF 32 C8 C8` | "First packet: send color" |
| 4 | `06 8A 42 00 20 41 08 <zone> 01` | "Second packet: apply to zone" |
| 5 | `06 8A 42 00 20 60 <zone>` | "Thrid packet: apply to zone" |
| 6 | `06 8A 42 00 20 05` | "Last packet: apply" |

12 reports per frame. HueDeck sends every frame (as OpenRGB does on every update, and as the
headset was recorded taking).

**Shutdown.** OpenRGB has no exit step and no onboard-lighting command for this headset: both
zones are set black with the same six reports.

## Left out

Nothing lighting-related. The serial-number string read is not needed. Volume, sidetone,
microphone, equaliser and every other headset setting are never touched (OpenRGB sends none).

## Replay of the recorded trace

The trace of the user's 12AA headset (green canvas, recorded with the SignalRGB-compatible plugin
of that session, 232 frames) was replayed against this plugin's OpenRGB stream. Since the plugin
no longer claims 12AA, that replay uses a scratch copy of the plugin whose `ProductId()` and
manifest add 12AA (nothing else changed); replayed straight against `plugins/steelseries-arctis`,
no plugin matches 12AA, as intended; with `--plugins plugins/steelseries-arctis-5 --plugins
plugins/steelseries-arctis` (this plugin scanned last), `steelseries-arctis-5` drives 12AA and the
replay matches the recording (2800 of 2800 packets). The replay of the OpenRGB stream diverges at
packet 0. Comparing the whole streams (2800 recorded packets, 2796 replayed):

- the recording starts with four reports OpenRGB does not send (`04 40 06 10 F6 60 09 2C FF 7F`
  and `04 40 01 11 54 9B`, 39 bytes; `06 8A 42 00 20 05`; `06 81 43 01 23`);
- the last report of each zone is `06 8A 42 20 05` in the recording and `06 8A 42 00 20 05`
  (OpenRGB) in the replay (466 times each);
- every other report (466 x five per zone: the two `81 43` reports, the colour `00 FF 00`, the
  zone and the apply, plus the black shutdown) is identical, in the same order.

Which form of the last report the headset expects (or whether it accepts both) is not known; the
recorded form is what HueDeck's own `steelseries-arctis-5` plugin sends and is confirmed working on
the reference headset.

## Untested

`1038:1250` (first-generation Arctis 5) has never been seen by HueDeck.
