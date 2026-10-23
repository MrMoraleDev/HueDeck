<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HyperX keyboards with HP product ids (`03F0`)

Plugin: `plugins/hyperx-keyboard-hp/`. Same code as `plugins/hyperx-keyboard/` (`family.js` and
`layouts.js` are identical copies, checked by a test); only the vendor id differs, because a
plugin has one. Protocol, LED layout, safety notes and the full sources table:
[hyperx-keyboard.md](hyperx-keyboard.md). Untested on hardware (`tested` is empty).

## Sources

| File | License | Taken |
|---|---|---|
| OpenRGB `Controllers/HyperXKeyboardController/*` and `RGBController/RGBControllerKeyNames.cpp` at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (listed one by one in [hyperx-keyboard.md](hyperx-keyboard.md)) | GPL-2.0-or-later | everything, as for the Kingston ids |

## Products

| Product id | OpenRGB name | Variant | Validate (Windows registration) |
|---|---|---|---|
| `03F0:058F` | HyperX Alloy Elite 2 (HP) | ALLOY_ELITE_2 | interface 3, usage page `FF90` |
| `03F0:0C8E` | HyperX Alloy Origins 60 (HP) | ALLOY_ORIGINS_60 | interface 3, any collection |
| `03F0:038F` | HyperX Alloy Origins 65 (HP) | ALLOY_ORIGINS_65 | interface 3, any collection |
| `03F0:0591` | HyperX Alloy Origins (HP) | ALLOY_ORIGINS | interface 3, usage page `FF90` |
| `03F0:08C2` | HyperX Eve 1800 (HP) | EVE_1800 | interface 2, any collection |
| `03F0:0CC2` | HyperX Origins 2 65 (HP) | ORIGINS_2_65 | interface 3, any collection |

Held back: HyperX Alloy Origins Core (HP) `03F0:098F` (see the main notes).

## Untested

Every product id.
