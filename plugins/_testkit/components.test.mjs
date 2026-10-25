// SPDX-License-Identifier: GPL-3.0-or-later
//
// The built-in component library (components/*.json) against schemas/component.schema.json,
// plus the invariants the schema cannot express and the loader checks: array lengths equal
// LedCount, coordinates inside Width x Height, LedMapping a permutation, unique ProductName.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { validate } from "../../sdk/lint/schema.mjs";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const schema = JSON.parse(readFileSync(join(repo, "schemas", "component.schema.json"), "utf8"));
const dir = join(repo, "components");
const files = readdirSync(dir).filter((f) => f.endsWith(".json")).sort();

test("the library has fans and strips", () => {
  assert.ok(files.length >= 40, `found ${files.length}`);
  const types = new Set(files.map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")).Type));
  assert.ok(types.has("Fan") && types.has("Strip"), [...types].join(", "));
});

test("ProductName is unique and prefixed", () => {
  const seen = new Map();
  for (const f of files) {
    const { ProductName } = JSON.parse(readFileSync(join(dir, f), "utf8"));
    assert.ok(ProductName.startsWith("Lightloom "), `${f}: ${ProductName}`);
    assert.ok(!seen.has(ProductName), `${f} and ${seen.get(ProductName)} share ${ProductName}`);
    seen.set(ProductName, f);
  }
});

for (const f of files) {
  test(`${f} follows the component schema`, () => {
    const text = readFileSync(join(dir, f), "utf8");
    assert.ok(!text.includes("\r"), "LF line endings");
    const c = JSON.parse(text);
    assert.deepEqual(validate(schema, c), []);
    assert.equal(c.LedMapping.length, c.LedCount);
    assert.equal(c.LedCoordinates.length, c.LedCount);
    assert.equal(c.LedNames.length, c.LedCount);
    for (const [x, y] of c.LedCoordinates) {
      assert.ok(x < c.Width && y < c.Height, `[${x}, ${y}] outside ${c.Width}x${c.Height}`);
    }
    assert.deepEqual([...c.LedMapping].sort((a, b) => a - b), [...Array(c.LedCount).keys()]);
    assert.ok(c.DisplayName.endsWith(` - ${c.LedCount} LED`), c.DisplayName);
  });
}
