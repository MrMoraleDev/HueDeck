<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# HueDeck brand

The HueDeck logo is an "H" made of three rows of light bars, filled with a hue gradient
that runs diagonally from red through magenta, violet and cyan to green.

| File | Use |
|---|---|
| `huedeck-icon.svg` | App icon: the mark with its neon glow on the dark tile (`#1b1c21`), 640×640. |
| `huedeck-icon-512.png` | The same icon rendered at 512×512, for places that need a bitmap. |
| `huedeck-mark.svg` | The mark alone, flat, transparent background (favicon, small sizes, any background). |
| `huedeck-logotype.svg` | Mark plus "uedeck" in white, transparent background: for dark backgrounds. |
| `huedeck-logotype-light.svg` | The same with dark lettering (`#1b1c21`): for light backgrounds. |

Copies in use elsewhere in the tree:
- `ui/public/favicon.svg` is `huedeck-mark.svg`.
- `ui/src/assets/huedeck-logotype.svg` is `huedeck-logotype.svg`.
- `packaging/wix/huedeck.ico` holds the icon at 16, 24, 32, 48, 64, 128 and 256 px.

When the logo changes, update these files from here, which is the source.

## Colours

The gradient stops, in order: `#ff0a1a`, `#fe1121`, `#f61a86`, `#ee1be6`, `#aa48f7`,
`#8f5ef8`, `#3ea7fa`, `#1bd8f8`, `#06fff8`, `#15ffa6`, `#15fe4a`, `#1cff3c`. The
connectors between the bars use the same gradient at 42 % opacity.

## Lettering

The lettering is Manrope ExtraBold, converted to outlines, so no font is needed to
display it. Manrope is licensed under the SIL Open Font License 1.1, which allows
embedding it in a logo.
