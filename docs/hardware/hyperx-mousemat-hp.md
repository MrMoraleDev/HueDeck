<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HyperX Pulsefire Mat with the HP product id (`03F0:0F8D`)

Plugin: `plugins/hyperx-mousemat-hp/`. Same code as `plugins/hyperx-mousemat/` (`family.js` is an
identical copy, checked by a test); only the vendor id differs, because a plugin has one.
Protocol and safety notes: [hyperx-mousemat.md](hyperx-mousemat.md). Untested on hardware.

## Sources

| File | License | Taken |
|---|---|---|
| OpenRGB `Controllers/HyperXMousematController/*` at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (listed in [hyperx-mousemat.md](hyperx-mousemat.md)) | GPL-2.0-or-later | everything |

## Products

| Product id | OpenRGB name | LEDs | Validate |
|---|---|---|---|
| `03F0:0F8D` | HyperX Pulsefire Mat | 15 underglow + 5 strip | interface 1, usage page `FF90`, usage `FF00` |

## Untested

The product id.
