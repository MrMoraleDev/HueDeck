<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HID LampArray: any Dynamic Lighting device

HID LampArray is the open USB HID standard behind Windows Dynamic Lighting (HID Usage Tables,
usage page `0x59` "Lighting and Illumination"). A LampArray device describes its own lamps
(count, position, colour levels) and takes colour updates in feature reports, so one plugin
drives every such device: `plugins/hid-lamparray/`.

## Sources

| # | Source | Used for |
|---|--------|----------|
| S1 | USB-IF HID Usage Tables 1.4, "Lighting and Illumination Page (0x59)" | Usage ids, report roles |
| S2 | Microsoft's public LampArray firmware sample (MIT), `microsoft/ArduinoHidForWindows` (`LampArrayReportDescriptor.h`, `Microsoft_HidLampArray.cpp`, `.h`), read 2026-10-18 | Usage ids (agree with S1), the six reports and their field sizes, the request/response sequence, LampUpdateFlags bit 0 = update complete, autonomous mode on by default, LampArrayKind and LampPurposes values |
| S3 | Lightloom's own runs against the Razer DeathAdder V2 Pro's LampArray collection (`1532:007C`, interface 3), 2026-10-18 | That a real device matches S1/S2 (see "Bench") |

No code of S2 is in the plugin; the report layouts are not hard-coded at all (they come from
each device's report descriptor).

## Usages (page 0x59)

| Id | Usage | | Id | Usage |
|----|-------|-|----|-------|
| `01` | LampArray (application collection) | | `29` | GreenLevelCount |
| `02` | LampArrayAttributesReport | | `2A` | BlueLevelCount |
| `03` | LampCount | | `2B` | IntensityLevelCount |
| `04` | BoundingBoxWidthInMicrometers | | `2C` | IsProgrammable |
| `05` | BoundingBoxHeightInMicrometers | | `2D` | InputBinding |
| `06` | BoundingBoxDepthInMicrometers | | `50` | LampMultiUpdateReport |
| `07` | LampArrayKind | | `51` | RedUpdateChannel |
| `08` | MinUpdateIntervalInMicroseconds | | `52` | GreenUpdateChannel |
| `20` | LampAttributesRequestReport | | `53` | BlueUpdateChannel |
| `21` | LampId | | `54` | IntensityUpdateChannel |
| `22` | LampAttributesResponseReport | | `55` | LampUpdateFlags |
| `23` | PositionXInMicrometers | | `60` | LampRangeUpdateReport |
| `24` | PositionYInMicrometers | | `61` | LampIdStart |
| `25` | PositionZInMicrometers | | `62` | LampIdEnd |
| `26` | LampPurposes | | `70` | LampArrayControlReport |
| `27` | UpdateLatencyInMicroseconds | | `71` | AutonomousMode |
| `28` | RedLevelCount | | | |

Each report sits in a logical collection with the report's usage; LampArrayKind: 1 keyboard,
2 mouse, 3 game controller, 4 peripheral, 5 scene, 6 notification, 7 chassis, 8 wearable,
9 furniture, 10 art.

## What the plugin does

1. `device.getHidReportFields()` (Lightloom host extension): the collection's reports and fields,
   from its report descriptor. The six reports are found by their logical collection's usage.
2. GET the attributes report: lamp count, bounding box, kind, minimum update interval.
3. For each lamp: SET the request report with its LampId, GET the response report: position,
   colour level counts, whether it is programmable.
4. Lamps go onto a grid of up to 32 cells along the bounding box's longer side (lamps in the
   same cell share its colour); `device.setSize` and `device.setControllableLeds`.
5. SET the control report with AutonomousMode 0: the host drives the lamps.
6. Every frame: multi-update reports with as many lamps as the report holds, the last one with
   LampUpdateFlags = 1 (update complete); colour channels scaled to each lamp's level count,
   intensity at its highest level. Devices without the multi-update report get one range
   update per lamp. Unchanged colours are resent once a second; frames closer together than
   3/4 of the device's minimum update interval are skipped.
7. Shutdown: AutonomousMode 1, the device takes its own lighting back.

A plugin that claims the device by VID:PID always wins over this one (on the reference
machine the Razer DeathAdder V2 Pro is claimed by `razer`); run with `--plugins plugins/hid-lamparray`
to use LampArray on such a device.

Windows' own Dynamic Lighting drives LampArray devices when it is on (Settings > Personalization
> Dynamic Lighting); it was off on the reference machine. With it on, Windows and Lightloom would
both send updates.

## Bench (2026-10-18, Razer DeathAdder V2 Pro, interface 3, SignalRGB closed)

- The report descriptor (hidapi's reconstruction on Windows) has the six reports with the field
  sizes of S2: attributes 23 bytes, request 3, response 29, multi update 51 (8 lamps),
  range update 10, control 2 (report id included).
- Attributes: 1 lamp, a mouse, 80.1 x 126.8 x 42.7 mm, at most one update every 33.3 ms.
- The lamp reports 255 levels per channel; red is sent as `FE` for full red.
- `lightloom light red` (4 s, recorded) and `lightloom effect effects/rainbow-wave.html` (10 s):
  287 feature reports, 0 errors, canvas to wire p50 1.8 ms / p99 2.4 ms.
