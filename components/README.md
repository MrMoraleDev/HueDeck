# Built-in component library

Descriptors of common addressable-RGB fans and strips, for the channel editor
(a motherboard ARGB header, a fan controller channel). Each file follows
[`schemas/component.schema.json`](../schemas/component.schema.json), the
SignalRGB-compatible component format, so files from other sources load the same way.

- **License:** CC0-1.0. Copy, change and redistribute them freely.
- **Source data:** public product pages (manufacturer and retailer spec sheets) for the
  LED counts. None of these files is derived from another program's component library.
- **Generated:** by [`tools/components/generate.mjs`](../tools/components/generate.mjs).
  Edit the generator and run `node tools/components/generate.mjs components` rather than
  editing the JSON by hand.
- **Shipped:** next to `huedeckd.exe` in `components\` (`tools/package/stage-windows.ps1`).
  The daemon loads the SignalRGB directories when that option is on, then this
  directory (`components` next to the executable, or the source tree's in a dev build),
  then the user's `components` directory. A later file with the same `ProductName`
  replaces an earlier one, so a user can correct any entry by copying it into the user
  directory and editing it.
- **Checked:** by `plugins/_testkit/components.test.mjs` (schema and the loader's
  invariants) and by the `huedeck-daemon` test `builtin_components_load_cleanly`.

`ProductName` is prefixed with `HueDeck ` so that these entries never replace another
library's entry of the same product.

## Geometry

The LED counts come from the product pages. The geometry is approximate: rings are
drawn clockwise from the top on a small square grid, a hub or rear ring is drawn inside
the frame ring, and the wire order (`LedMapping`) is the identity in the order listed
below. If a fan lights in the wrong order, the fix is a corrected `LedMapping`.

| File | LEDs | Layout (wire order) | Count source |
| --- | --- | --- | --- |
| `generic-argb-fan-120-mm-ring-{6,8,12,16,18,24}` | 6–24 | one ring | generic |
| `generic-argb-fan-140-mm-ring-{12,18,24}` | 12–24 | one ring | generic |
| `generic-argb-fan-*-dual-ring-*` | 20, 24, 30 | inner ring, outer ring | generic |
| `generic-argb-strip-{6..60}` | 6–60 | a row | generic |
| `corsair-ll120-rgb`, `corsair-ll140-rgb` | 16 | hub 4, frame 12 | Corsair product page |
| `corsair-ql120-rgb`, `corsair-ql140-rgb` | 34 | front hub 4, front frame 12, rear hub 6, rear frame 12 | Corsair product page and launch coverage (four loops: 4 + 12 + 6 + 12) |
| `corsair-sp120-rgb-elite`, `corsair-sp140-rgb-elite` | 8 | hub ring | Corsair product page |
| `corsair-af120-rgb-elite`, `corsair-af140-rgb-elite` | 8 | hub ring | Corsair product page |
| `lian-li-uni-fan-sl120`, `lian-li-uni-fan-sl140` | 16 | one ring (count only) | L-Connect channel size; marketing material counts both sides. To verify |
| `nzxt-f120-rgb-duo`, `nzxt-f140-rgb-duo` | 20 | inner 8, outer 12 | NZXT product page (20 LEDs) |
| `nzxt-f120-rgb-core` | 8 | hub ring | NZXT product page |
| `nzxt-aer-rgb-2-120`, `nzxt-aer-rgb-2-140` | 8 | one ring | NZXT product page |
| `arctic-p12-pwm-pst-a-rgb` | 12 | hub ring | ARCTIC product page (12 digital LEDs in the hub) |
| `be-quiet-light-wings-120-mm` | 18 | rear 8, front 10 | be quiet! product page (18 LEDs, front and rear). Split to verify |
| `thermaltake-riing-plus-12` | 12 | one ring | Thermaltake product page. To verify |
| `thermaltake-riing-trio-12` | 30 | inner 6, middle 12, outer 12 | Thermaltake product page (30 LEDs, three rings) |
| `thermaltake-riing-quad-12` | 54 | hub 6, inner 12, middle 18, outer 18 | Thermaltake product page (54 LEDs, four rings) |
| `phanteks-d30-120-d-rgb` | 30 | front 12, frame 18 | Phanteks product page (30 LED chips). Split to verify |
| `cooler-master-sickleflow-120-argb` | 6 | hub ring | retailer spec sheet. To verify |
| `cooler-master-masterfan-mf120-halo` | 24 | inner 12, outer 12 | dual-loop design. To verify |

"To verify" marks counts that rest on a single source or on an inference. Corrections
with a link to the manufacturer's data are welcome.
