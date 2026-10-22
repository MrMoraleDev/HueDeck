// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import defaultExport, {
  createColorArray,
  layoutColors,
  layoutFlatColors,
  reorderColor,
  normalizeColorOrder,
  normalizeLayout,
  COLOR_ORDERS,
  LAYOUTS,
} from "../createColorArray.js";

const R = 0x11;
const G = 0x22;
const B = 0x33;
const HEX = "#112233";

test("default export and constants", () => {
  assert.equal(defaultExport, createColorArray);
  assert.deepEqual([...COLOR_ORDERS], ["RGB", "RBG", "BGR", "BRG", "GBR", "GRB"]);
  assert.deepEqual([...LAYOUTS], ["Inline", "Separate", "Seperate"]);
  assert.ok(Object.isFrozen(COLOR_ORDERS));
});

test("Inline layout: count*3 flat array in RGB by default", () => {
  assert.deepEqual(createColorArray(HEX, 3, "Inline"), [R, G, B, R, G, B, R, G, B]);
  assert.deepEqual(createColorArray(HEX, 1, "Inline"), [R, G, B]);
  const arr = createColorArray("#ff0000", 40, "Inline");
  assert.equal(arr.length / 3, 40);
  assert.deepEqual(arr.splice(0, 3), [255, 0, 0]);
  assert.equal(arr.length, 117);
  // layout defaults to Inline when omitted
  assert.deepEqual(createColorArray(HEX, 2), [R, G, B, R, G, B]);
});

test("Separate and Seperate layouts: three planes", () => {
  assert.deepEqual(createColorArray(HEX, 3, "Separate"), [
    [R, R, R],
    [G, G, G],
    [B, B, B],
  ]);
  assert.deepEqual(createColorArray(HEX, 2, "Seperate"), [
    [R, R],
    [G, G],
    [B, B],
  ]);
  const planes = createColorArray(HEX, 0, "Separate");
  assert.deepEqual(planes, [[], [], []]);
  assert.equal(planes.length, 3);
});

test("every color order permutation, Inline and Separate", () => {
  const expected = {
    RGB: [R, G, B],
    RBG: [R, B, G],
    BGR: [B, G, R],
    BRG: [B, R, G],
    GBR: [G, B, R],
    GRB: [G, R, B],
  };
  for (const [order, triple] of Object.entries(expected)) {
    assert.deepEqual(createColorArray(HEX, 2, "Inline", order), [...triple, ...triple], order);
    assert.deepEqual(
      createColorArray(HEX, 2, "Separate", order),
      [
        [triple[0], triple[0]],
        [triple[1], triple[1]],
        [triple[2], triple[2]],
      ],
      `${order} separate`,
    );
    assert.deepEqual(reorderColor([R, G, B], order), triple);
    assert.deepEqual(reorderColor(HEX, order), triple);
  }
});

test("order and layout matching is case-insensitive; unknown values fall back", () => {
  assert.deepEqual(createColorArray(HEX, 1, "inline", "grb"), [G, R, B]);
  assert.deepEqual(createColorArray(HEX, 1, "SEPARATE", " bgr "), [[B], [G], [R]]);
  assert.deepEqual(createColorArray(HEX, 1, "Inline", "XYZ"), [R, G, B], "unknown order -> RGB");
  assert.deepEqual(createColorArray(HEX, 1, "Inline", "RGBW"), [R, G, B]);
  assert.deepEqual(createColorArray(HEX, 1, "Inline", undefined), [R, G, B]);
  assert.deepEqual(createColorArray(HEX, 1, "Inline", 7), [R, G, B]);
  assert.deepEqual(createColorArray(HEX, 1, "Bogus"), [R, G, B], "unknown layout -> Inline");
  assert.deepEqual(createColorArray(HEX, 1, undefined, "GRB"), [G, R, B]);
  assert.equal(normalizeColorOrder("rbg"), "RBG");
  assert.equal(normalizeColorOrder(null), "RGB");
  assert.equal(normalizeLayout("seperate"), "Separate");
  assert.equal(normalizeLayout("SEPARATE"), "Separate");
  assert.equal(normalizeLayout("whatever"), "Inline");
  assert.equal(normalizeLayout(null), "Inline");
});

test("invalid hex falls back to #000000; arrays are accepted as color", () => {
  for (const bad of ["", "#12345", "nope", "#gggggg", null, undefined, 123, {}]) {
    assert.deepEqual(createColorArray(bad, 2, "Inline", "GRB"), [0, 0, 0, 0, 0, 0], String(bad));
  }
  assert.deepEqual(createColorArray("112233", 1, "Inline"), [R, G, B], "missing # accepted");
  assert.deepEqual(createColorArray("#AaBbCc", 1, "Inline"), [0xaa, 0xbb, 0xcc]);
  assert.deepEqual(createColorArray([1, 2, 3], 1, "Inline", "BGR"), [3, 2, 1]);
  assert.deepEqual(createColorArray([300, -4, 1.4], 1, "Inline"), [255, 0, 1]);
  assert.deepEqual(createColorArray(Uint8Array.from([9, 8, 7]), 1, "Separate"), [[9], [8], [7]]);
  assert.deepEqual(createColorArray([1, 2], 1, "Inline"), [0, 0, 0], "short array -> black");
});

test("count coercion: zero, negative, NaN, strings, fractions", () => {
  assert.deepEqual(createColorArray(HEX, 0, "Inline"), []);
  assert.deepEqual(createColorArray(HEX, -5, "Inline"), []);
  assert.deepEqual(createColorArray(HEX, NaN, "Inline"), []);
  assert.deepEqual(createColorArray(HEX, undefined, "Inline"), []);
  assert.deepEqual(createColorArray(HEX, "2", "Inline"), [R, G, B, R, G, B]);
  assert.deepEqual(createColorArray(HEX, 2.9, "Inline"), [R, G, B, R, G, B]);
  assert.deepEqual(createColorArray(HEX, -1, "Separate"), [[], [], []]);
  const big = createColorArray(HEX, 1000, "Inline", "GRB");
  assert.equal(big.length, 3000);
  assert.equal(big[2997], G);
  assert.equal(big[2999], B);
});

test("results are fresh arrays (plugins mutate them)", () => {
  const a = createColorArray(HEX, 2, "Inline");
  const b = createColorArray(HEX, 2, "Inline");
  assert.notEqual(a, b);
  a[0] = 99;
  assert.equal(b[0], R);
  const planes = createColorArray(HEX, 2, "Separate");
  assert.notEqual(planes[0], planes[1]);
});

test("layoutColors lays out sampled triples like channel.getColors", () => {
  const triples = [
    [1, 2, 3],
    [4, 5, 6],
  ];
  assert.deepEqual(layoutColors(triples, "Inline"), [1, 2, 3, 4, 5, 6]);
  assert.deepEqual(layoutColors(triples, "Inline", "GRB"), [2, 1, 3, 5, 4, 6]);
  assert.deepEqual(layoutColors(triples, "Separate"), [
    [1, 4],
    [2, 5],
    [3, 6],
  ]);
  assert.deepEqual(layoutColors(triples, "Seperate", "BGR"), [
    [3, 6],
    [2, 5],
    [1, 4],
  ]);
  assert.deepEqual(layoutColors([], "Inline"), []);
  assert.deepEqual(layoutColors(null, "Separate"), [[], [], []]);
  assert.deepEqual(layoutColors(["#ff0000"], "Inline"), [255, 0, 0], "hex strings accepted per entry");
});

test("layoutFlatColors lays out a flat host stream like channel.getColors", () => {
  const flat = [1, 2, 3, 4, 5, 6];
  assert.deepEqual(layoutFlatColors(flat), [1, 2, 3, 4, 5, 6]);
  assert.deepEqual(layoutFlatColors(flat, "Inline", "GRB"), [2, 1, 3, 5, 4, 6]);
  assert.deepEqual(layoutFlatColors(flat, "Separate"), [
    [1, 4],
    [2, 5],
    [3, 6],
  ]);
  assert.deepEqual(layoutFlatColors(flat, "seperate", "bgr"), [
    [3, 6],
    [2, 5],
    [1, 4],
  ]);
  assert.deepEqual(layoutFlatColors([7, 8, 9, 10], "Inline"), [7, 8, 9], "partial triple dropped");
  assert.deepEqual(layoutFlatColors([300, -1, 2.6]), [255, 0, 3], "bytes clamped");
  assert.deepEqual(layoutFlatColors(undefined, "Separate"), [[], [], []]);
  const a = layoutFlatColors(flat);
  a[0] = 99;
  assert.equal(flat[0], 1, "fresh array");
});
