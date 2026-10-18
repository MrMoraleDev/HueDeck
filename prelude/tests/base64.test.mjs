// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import defaultExport, { encode, decode, Encode, Decode, decodeToString, createFeatureGlobal } from "../base64.js";

const ref = (bytes) => Buffer.from(bytes).toString("base64");

test("module named exports and feature-global aliases", () => {
  assert.equal(Encode, encode);
  assert.equal(Decode, decode);
  assert.equal(defaultExport.encode, encode);
  assert.equal(defaultExport.Decode, decode);
  const g = createFeatureGlobal();
  assert.ok(Object.isFrozen(g));
  assert.equal(g.Encode, encode);
  assert.equal(g.Decode, decode);
  assert.equal(g.encode, encode);
  assert.equal(g.decode, decode);
});

test("encode matches the standard alphabet for every padding case", () => {
  assert.equal(encode([]), "");
  assert.equal(encode([0]), "AA==");
  assert.equal(encode([0, 0]), "AAA=");
  assert.equal(encode([0, 0, 0]), "AAAA");
  assert.equal(encode([255]), "/w==");
  assert.equal(encode([255, 255]), "//8=");
  assert.equal(encode([255, 255, 255]), "////");
  assert.equal(encode([0xfb, 0xff, 0xbf]), "+/+/");
  for (let n = 0; n < 40; n++) {
    const bytes = Array.from({ length: n }, (_, i) => (i * 37 + 11) & 0xff);
    assert.equal(encode(bytes), ref(bytes), `length ${n}`);
  }
});

test("encode accepts typed arrays, ArrayBuffer, strings (UTF-8) and masks numbers", () => {
  const bytes = [1, 2, 3, 250];
  assert.equal(encode(Uint8Array.from(bytes)), ref(bytes));
  assert.equal(encode(Uint8Array.from(bytes).buffer), ref(bytes));
  const view = new Uint8Array(Uint8Array.from([9, 9, 1, 2, 3, 250, 9]).buffer, 2, 4);
  assert.equal(encode(view), ref(bytes));
  assert.equal(encode("hello"), ref(Buffer.from("hello", "utf8")));
  assert.equal(encode("héllo €"), Buffer.from("héllo €", "utf8").toString("base64"));
  assert.equal(encode("😀"), Buffer.from("😀", "utf8").toString("base64"));
  // numbers outside 0..255 are masked, like a Uint8Array would
  assert.equal(encode([256, -1, 0x1ff]), ref([0, 255, 255]));
  assert.equal(encode(["65", null, undefined, 1.9]), ref([65, 0, 0, 1]));
  assert.equal(encode(null), "");
  assert.equal(encode(undefined), "");
  assert.equal(encode(new Set([1, 2, 3])), ref([1, 2, 3]));
  assert.throws(() => encode(42), TypeError);
});

test("encode urlSafe uses -_ and keeps padding", () => {
  assert.equal(encode([0xfb, 0xff, 0xbf], true), "-_-_");
  assert.equal(encode([0xfb, 0xff], true), "-_8=");
  assert.equal(encode([0xfb], true), "-w==");
  assert.equal(encode([0xfb, 0xff, 0xbf], false), "+/+/");
  for (let n = 1; n < 20; n++) {
    const bytes = Array.from({ length: n }, (_, i) => (i * 97 + 3) & 0xff);
    assert.equal(encode(bytes, true), Buffer.from(bytes).toString("base64url").padEnd(Math.ceil(n / 3) * 4, "="));
  }
});

test("decode returns plain arrays and round-trips", () => {
  const out = decode("AQID");
  assert.ok(Array.isArray(out));
  assert.deepEqual(out, [1, 2, 3]);
  assert.deepEqual(decode(""), []);
  assert.deepEqual(decode(null), []);
  assert.deepEqual(decode(undefined), []);
  for (let n = 0; n < 40; n++) {
    const bytes = Array.from({ length: n }, (_, i) => (i * 53 + 7) & 0xff);
    assert.deepEqual(decode(encode(bytes)), bytes, `length ${n}`);
    assert.deepEqual(decode(encode(bytes, true)), bytes, `urlSafe length ${n}`);
  }
  // Twinkly-style token round trip
  const token = encode(Array.from({ length: 32 }, (_, i) => i));
  assert.deepEqual(decode(token), Array.from({ length: 32 }, (_, i) => i));
  assert.deepEqual(decode(token).concat([1]), [...Array.from({ length: 32 }, (_, i) => i), 1]);
});

test("decode is lenient: missing padding, whitespace, mixed alphabet, foreign chars, stops at =", () => {
  assert.deepEqual(decode("AQID\n"), [1, 2, 3]);
  assert.deepEqual(decode("AQ"), [1]);
  assert.deepEqual(decode("AQI"), [1, 2]);
  assert.deepEqual(decode("AQ=="), [1]);
  assert.deepEqual(decode(" A Q I D "), [1, 2, 3]);
  assert.deepEqual(decode("-_-_"), [0xfb, 0xff, 0xbf]);
  assert.deepEqual(decode("+/-_"), [0xfb, 0xff, 0xbf]);
  assert.deepEqual(decode("AQ*ID"), [1, 2, 3]);
  assert.deepEqual(decode("AQID=AQID"), [1, 2, 3], "data after '=' is ignored");
  assert.deepEqual(decode("A"), [], "a single sextet carries no full byte");
  assert.deepEqual(decode("ÿÿ"), [], "non-latin1-range code units are ignored");
  assert.deepEqual(decode(12), decode("12"), "non-string input is stringified");
  assert.deepEqual(decode(12), [215]);
});

test("decodeToString handles UTF-8", () => {
  assert.equal(decodeToString(encode("héllo € 😀")), "héllo € 😀");
  assert.equal(decodeToString(""), "");
});
