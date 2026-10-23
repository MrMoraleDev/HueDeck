<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Turtle Beach Vulcan II keyboards (`10F5`)

Plugin: `plugins/roccat-vulcan-turtle-beach/`. The Turtle Beach branded Roccat Vulcan II models.
Same protocol, code (`vulcan.js`) and layouts (`layouts.js`) as `plugins/roccat-vulcan/`; it is a
separate plugin only because a plugin has one vendor id. Protocol, deviations and safety notes:
[roccat-vulcan.md](roccat-vulcan.md). Untested on hardware (`tested` is empty).

## Sources

As [roccat-vulcan.md](roccat-vulcan.md) (all `GPL-2.0-or-later`, OpenRGB commit
`d4798c120659ec5ba2e664a9e87bedff8d36f880`):

| Source | License | Taken |
|---|---|---|
| `Controllers/RoccatController/RoccatControllerDetect.cpp` | GPL-2.0-or-later | Product ids, names, detector macro |
| `.../RoccatVulcanKeyboardController/RoccatVulcanKeyboardController.cpp`, `.h` (Mola19) | GPL-2.0-or-later | Protocol (shared `vulcan.js`) |
| `.../RGBController_RoccatVulcanKeyboard.cpp` (Mola19) | GPL-2.0-or-later | Layout table per product, zones |
| `.../RoccatVulcanKeyboardLayouts.h` (Mola19) | GPL-2.0-or-later | Layouts (shared `layouts.js`) |
| `RGBController/RGBControllerKeyNames.cpp` | GPL-2.0-or-later | Key names |

## Products

| PID | OpenRGB name | Control collection | LED collection | Layout table | Colour frame |
|---|---|---|---|---|---|
| `501B` | Turtle Beach Vulcan II | if 1, page `FF01` | if 3, page `FF00` | Vulcan II (UK only) | 396 bytes, protocol 2 |
| `5023` | Turtle Beach Vulcan II TKL | if 1, page `000B` | if 3, page `0001` | TKL | 436 bytes, protocol 1 (older family: info `0x0F`, switch `15 00 <on>`) |
| `5001` | Turtle Beach Vulcan II TKL Pro | if 1, page `0001` | if 4, page `FF00` | Turtle Beach TKL Pro (UK only) | 320 bytes, three colour planes |

The TKL Pro's control collection is on usage page `0x0001` of interface 1, which also carries the
keyboard's own generic-desktop collections; OpenRGB matches only interface and usage page, and so
does `Validate()`. The plugin sends only feature reports `0E 05 <on> 00 00` there.

## Untested

Every product id.
