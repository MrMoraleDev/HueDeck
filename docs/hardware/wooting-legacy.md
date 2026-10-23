<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Wooting keyboards with legacy (V1) firmware (`03EB`)

Plugin: `plugins/wooting-legacy/`. Same code as `plugins/wooting/` (`family.js` and `layouts.js`
are identical copies, checked by a test); only the vendor id differs, because a plugin has one.
Protocol (V1 frame), LEDs, safety notes and the full sources table: [wooting.md](wooting.md).
Untested on hardware (`tested` is empty).

## Sources

| File | License | Taken |
|---|---|---|
| OpenRGB `Controllers/WootingKeyboardController/*` at commit `d4798c120659ec5ba2e664a9e87bedff8d36f880` (listed in [wooting.md](wooting.md)) | GPL-2.0-or-later | everything, V1 report format |

## Products

| Product id | OpenRGB name | Type | Validate |
|---|---|---|---|
| `03EB:FF01` | Wooting One (Legacy) | TKL | usage page `1337`, any interface (`REGISTER_HID_DETECTOR_P`) |
| `03EB:FF02` | Wooting Two (Legacy) | FULL | usage page `1337`, any interface |

`03EB` is Atmel's vendor id; only these two product ids are claimed.

## Untested

Both product ids, including the reset-all shutdown command (see [wooting.md](wooting.md)).
