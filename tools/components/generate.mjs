// SPDX-License-Identifier: CC0-1.0
//
// Generates the built-in component library components/*.json (CC0). LED counts come from
// manufacturer and retailer product pages (see components/README.md); ring geometry and wire
// order are approximations. Usage: node tools/components/generate.mjs components
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const out = process.argv[2];
mkdirSync(out, { recursive: true });

const slug = (s) => s.toLowerCase().replace(/!/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

/** One ring of n LEDs, clockwise from the top, radius as a fraction of the half-size. */
function ring(n, size, frac, prefix) {
  const c = (size - 1) / 2;
  const r = c * frac;
  const leds = [];
  for (let i = 0; i < n; i++) {
    const a = (2 * Math.PI * i) / n;
    const x = Math.min(size - 1, Math.max(0, Math.round(c + r * Math.sin(a))));
    const y = Math.min(size - 1, Math.max(0, Math.round(c - r * Math.cos(a))));
    leds.push({ xy: [x, y], name: `${prefix}${i + 1}` });
  }
  return leds;
}

function gridFor(total) {
  return total <= 8 ? 5 : total <= 16 ? 7 : total <= 24 ? 9 : 13;
}

/** rings: [{ n, frac, name }] in wire order. */
function fan(rings) {
  const outer = Math.max(...rings.map((r) => r.n));
  const size = gridFor(outer);
  const multi = rings.length > 1;
  return {
    size: [size, size],
    leds: rings.flatMap((r) => ring(r.n, size, r.frac, multi ? `${r.name} ` : "Led ")),
  };
}

function strip(n) {
  return { size: [n, 1], leds: Array.from({ length: n }, (_, i) => ({ xy: [i, 0], name: `Led ${i + 1}` })) };
}

const files = [];
function add(brand, product, type, shape, file) {
  const n = shape.leds.length;
  const doc = {
    ProductName: `Lightloom ${brand} ${product}`,
    DisplayName: `${product} - ${n} LED`,
    Brand: brand,
    Type: type,
    LedCount: n,
    Width: shape.size[0],
    Height: shape.size[1],
    LedMapping: shape.leds.map((_, i) => i),
    LedCoordinates: shape.leds.map((l) => l.xy),
    LedNames: shape.leds.map((l) => l.name),
  };
  const name = file ?? `${slug(brand)}-${slug(product)}.json`;
  const json =
    "{\n" +
    Object.entries(doc)
      .map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v)}`)
      .join(",\n") +
    "\n}\n";
  writeFileSync(join(out, name), json);
  files.push(name);
}

const single = (n) => fan([{ n, frac: 1, name: "Ring" }]);
const dual = (inner, outer, innerName = "Inner", outerName = "Outer") =>
  fan([
    { n: inner, frac: 0.45, name: innerName },
    { n: outer, frac: 1, name: outerName },
  ]);

// Generic ARGB fans (single ring).
for (const n of [6, 8, 12, 16, 18, 24]) add("Generic", `ARGB Fan 120 mm Ring ${n}`, "Fan", single(n));
for (const n of [12, 18, 24]) add("Generic", `ARGB Fan 140 mm Ring ${n}`, "Fan", single(n));
// Generic dual-ring fans (hub ring first, then the frame ring).
add("Generic", "ARGB Fan 120 mm Dual Ring 8+12", "Fan", dual(8, 12));
add("Generic", "ARGB Fan 120 mm Dual Ring 12+12", "Fan", dual(12, 12));
add("Generic", "ARGB Fan 140 mm Dual Ring 12+18", "Fan", dual(12, 18));
// Generic strips.
for (const n of [6, 8, 10, 12, 15, 18, 20, 24, 30, 36, 40, 45, 48, 54, 60]) add("Generic", `ARGB Strip ${n}`, "Strip", strip(n));

// Corsair (driven through Corsair fan controllers).
add("Corsair", "LL120 RGB", "Fan", dual(4, 12));
add("Corsair", "LL140 RGB", "Fan", dual(4, 12));
const ql = () =>
  fan([
    { n: 4, frac: 0.3, name: "Front Inner" },
    { n: 12, frac: 1, name: "Front Outer" },
    { n: 6, frac: 0.45, name: "Rear Inner" },
    { n: 12, frac: 0.85, name: "Rear Outer" },
  ]);
add("Corsair", "QL120 RGB", "Fan", ql());
add("Corsair", "QL140 RGB", "Fan", ql());
add("Corsair", "SP120 RGB ELITE", "Fan", single(8));
add("Corsair", "SP140 RGB ELITE", "Fan", single(8));
add("Corsair", "AF120 RGB ELITE", "Fan", single(8));
add("Corsair", "AF140 RGB ELITE", "Fan", single(8));

// Lian Li (LED count only: the geometry is a plain ring).
add("Lian Li", "UNI FAN SL120", "Fan", single(16));
add("Lian Li", "UNI FAN SL140", "Fan", single(16));

// NZXT.
add("NZXT", "F120 RGB Duo", "Fan", dual(8, 12));
add("NZXT", "F140 RGB Duo", "Fan", dual(8, 12));
add("NZXT", "F120 RGB Core", "Fan", single(8));
add("NZXT", "Aer RGB 2 120", "Fan", single(8));
add("NZXT", "Aer RGB 2 140", "Fan", single(8));

// ARCTIC.
add("ARCTIC", "P12 PWM PST A-RGB", "Fan", single(12));

// be quiet!
add("be quiet!", "Light Wings 120 mm", "Fan", dual(8, 10, "Rear", "Front"));

// Thermaltake.
add("Thermaltake", "Riing Plus 12", "Fan", single(12));
add("Thermaltake", "Riing Trio 12", "Fan", fan([
  { n: 6, frac: 0.35, name: "Inner" },
  { n: 12, frac: 0.7, name: "Middle" },
  { n: 12, frac: 1, name: "Outer" },
]));
add("Thermaltake", "Riing Quad 12", "Fan", fan([
  { n: 6, frac: 0.25, name: "Hub" },
  { n: 12, frac: 0.5, name: "Inner" },
  { n: 18, frac: 0.75, name: "Middle" },
  { n: 18, frac: 1, name: "Outer" },
]));

// Phanteks.
add("Phanteks", "D30-120 D-RGB", "Fan", dual(12, 18, "Front", "Frame"));

// Cooler Master.
add("Cooler Master", "SickleFlow 120 ARGB", "Fan", single(6));
add("Cooler Master", "MasterFan MF120 Halo", "Fan", dual(12, 12));

console.log(`${files.length} files`);
