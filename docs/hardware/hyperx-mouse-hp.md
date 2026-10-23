<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HyperX Pulsefire mice with HP product ids (`03F0`)

Plugin: `plugins/hyperx-mouse-hp/`. Same code as `plugins/hyperx-mouse/` (`family.js` is an
identical copy, checked by a test); only the vendor id differs, because a plugin has one.
Protocol, LEDs, safety notes and the full sources table: [hyperx-mouse.md](hyperx-mouse.md).
Untested on hardware (`tested` is empty).

## Sources

| File | License | Taken |
|---|---|---|
| OpenRGB `Controllers/HyperXMouseController/*` at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (listed in [hyperx-mouse.md](hyperx-mouse.md)) | GPL-2.0-or-later | everything, as for the Kingston ids |

## Products

| Product id | OpenRGB name | Variant | Validate |
|---|---|---|---|
| `03F0:0490` | HyperX Pulsefire Surge (HP) | SURGE | interface 1, usage page `FF01` |
| `03F0:0D8F` | HyperX Pulsefire Core (HP) | FPS_PRO | interface 1, usage page `FF01` |
| `03F0:068E` | HyperX Pulsefire Dart (Wireless) | DART | interface 2, usage page `FF00` |
| `03F0:088E` | HyperX Pulsefire Dart (Wired) | DART | interface 1, usage page `FF13` |
| `03F0:0F8F` | HyperX Pulsefire Haste | HASTE | interface 3, usage page `FF90` |

## Untested

Every product id.
