// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import defaultExport, { hexToRgb, rgbToHex, isHexColor, clampByte, normalizeHex } from "../hexToRgb.js";

test("default export is hexToRgb", () => {
  assert.equal(defaultExport, hexToRgb);
});

test("hexToRgb parses 6-digit forms in any case with or without #", () => {
  assert.deepEqual(hexToRgb("#1A2b3C"), [26, 43, 60]);
  assert.deepEqual(hexToRgb("1a2b3c"), [26, 43, 60]);
  assert.deepEqual(hexToRgb("#FFFFFF"), [255, 255, 255]);
  assert.deepEqual(hexToRgb("#000000"), [0, 0, 0]);
  assert.deepEqual(hexToRgb("#009EDB"), [0, 158, 219]);
  assert.deepEqual(hexToRgb("  #ff0000 "), [255, 0, 0]);
});

test("hexToRgb expands 3-digit shorthand", () => {
  assert.deepEqual(hexToRgb("#abc"), [170, 187, 204]);
  assert.deepEqual(hexToRgb("f00"), [255, 0, 0]);
});

test("hexToRgb falls back to black on invalid input instead of throwing", () => {
  for (const bad of ["", "#", "#12345", "#1234567", "#gggggg", "red", "#12 34 56", "0x123456", null, undefined, 42, {}, [], true]) {
    assert.deepEqual(hexToRgb(bad), [0, 0, 0], String(bad));
  }
});

test("rgbToHex accepts tuples, objects and separate arguments; clamps and rounds", () => {
  assert.equal(rgbToHex(26, 43, 60), "#1a2b3c");
  assert.equal(rgbToHex([26, 43, 60]), "#1a2b3c");
  assert.equal(rgbToHex(Uint8Array.from([255, 0, 128])), "#ff0080");
  assert.equal(rgbToHex({ r: 0, g: 158, b: 219 }), "#009edb");
  assert.equal(rgbToHex(-5, 300, 12.6), "#00ff0d");
  assert.equal(rgbToHex(NaN, undefined, "7"), "#000007");
  assert.equal(rgbToHex([]), "#000000");
});

test("round trips", () => {
  for (const hex of ["#000000", "#ffffff", "#123456", "#abcdef", "#0a0b0c"]) {
    assert.equal(rgbToHex(hexToRgb(hex)), hex);
  }
  assert.deepEqual(hexToRgb(rgbToHex(1, 2, 3)), [1, 2, 3]);
});

test("isHexColor / clampByte / normalizeHex", () => {
  assert.equal(isHexColor("#abcdef"), true);
  assert.equal(isHexColor("abcdef"), true);
  assert.equal(isHexColor("#abc"), true);
  assert.equal(isHexColor("#abcd"), false);
  assert.equal(isHexColor("zzzzzz"), false);
  assert.equal(isHexColor(123456), false);
  assert.equal(isHexColor(null), false);
  assert.equal(clampByte(-1), 0);
  assert.equal(clampByte(256), 255);
  assert.equal(clampByte(12.5), 13);
  assert.equal(clampByte("x"), 0);
  assert.equal(clampByte(Infinity), 0);
  assert.equal(normalizeHex("ABCDEF"), "#abcdef");
  assert.equal(normalizeHex("#abc"), "#aabbcc");
  assert.equal(normalizeHex("nope"), "#000000");
});
