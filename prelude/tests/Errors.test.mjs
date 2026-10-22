// SPDX-License-Identifier: GPL-3.0-or-later
import { test, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import defaultExport, { Assert, ContextError, globalContext } from "../Errors.js";

let logs;
let savedDevice;
let savedHuedeck;

beforeEach(() => {
  logs = [];
  savedDevice = globalThis.device;
  savedHuedeck = globalThis.__huedeck;
  delete globalThis.__huedeck;
  globalThis.device = { log: (m) => logs.push(m) };
  globalContext.clear();
});

afterEach(() => {
  globalThis.device = savedDevice;
  if (savedHuedeck === undefined) {
    delete globalThis.__huedeck;
  } else {
    globalThis.__huedeck = savedHuedeck;
  }
  globalContext.clear();
});

test("default export bundles the three named exports", () => {
  assert.equal(defaultExport.Assert, Assert);
  assert.equal(defaultExport.ContextError, ContextError);
  assert.equal(defaultExport.globalContext, globalContext);
  assert.ok(Object.isFrozen(Assert));
});

test("Assert.isOk passes truthy values through and throws ContextError on falsy", () => {
  const obj = { a: 1 };
  assert.equal(Assert.isOk(obj, "msg"), obj);
  assert.equal(Assert.isOk(1, "msg"), 1);
  assert.equal(Assert.isOk("x", "msg"), "x");
  for (const falsy of [undefined, null, 0, "", false, NaN]) {
    assert.throws(
      () => Assert.isOk(falsy, "Channel not found", { id: 3 }),
      (e) => e instanceof ContextError && e instanceof Error && e.name === "ContextError" && e.message === "Channel not found" && e.context.id === 3,
      String(falsy),
    );
  }
  // default message when omitted
  assert.throws(() => Assert.isOk(0), /expected a truthy value/);
});

test("Assert.isEqual uses strict equality and records actual/expected", () => {
  assert.equal(Assert.isEqual(3, 3, "same"), true);
  assert.equal(Assert.isEqual("a", "a"), true);
  assert.throws(
    () => Assert.isEqual(3, "3", "Device LEDs mapping size is wrong", { cfg: "x" }),
    (e) => e instanceof ContextError && e.context.actual === 3 && e.context.expected === "3" && e.context.cfg === "x",
  );
  assert.throws(() => Assert.isEqual([1], [1], "arrays are compared by reference"), ContextError);
  assert.throws(() => Assert.isEqual(NaN, NaN), ContextError);
});

test("Assert.unreachable and Assert.fail always throw", () => {
  assert.throws(
    () => Assert.unreachable("Invalid Led Count", { leds: 999 }),
    (e) => e instanceof ContextError && e.message === "Invalid Led Count" && e.context.leds === 999,
  );
  assert.throws(() => Assert.unreachable(), /Unreachable/);
  assert.throws(
    () => Assert.fail("Failed to fetch motherboard name from WMI after 10 attempts!"),
    (e) => e instanceof ContextError && /WMI/.test(e.message),
  );
  assert.throws(() => Assert.fail(), /Assertion failed/);
});

test("Assert.softIsDefined never throws, logs once per call on null/undefined", () => {
  assert.equal(Assert.softIsDefined({}, "x"), true);
  assert.equal(Assert.softIsDefined(0, "x"), true);
  assert.equal(Assert.softIsDefined("", "x"), true);
  assert.equal(Assert.softIsDefined(false, "x"), true);
  assert.equal(logs.length, 0);
  assert.equal(Assert.softIsDefined(undefined, "Permissions object doesn't contain: MSI"), false);
  assert.equal(Assert.softIsDefined(null, "nope", { target: "Lian-Li" }), false);
  assert.equal(logs.length, 2);
  assert.match(logs[0], /softIsDefined: Permissions object doesn't contain: MSI/);
  assert.match(logs[1], /"target":"Lian-Li"/);
});

test("softIsDefined falls back to console.warn when no device/service/host logger exists", () => {
  delete globalThis.device;
  const origWarn = console.warn;
  const warned = [];
  console.warn = (m) => warned.push(m);
  try {
    Assert.softIsDefined(undefined, "fallback");
  } finally {
    console.warn = origWarn;
  }
  assert.equal(warned.length, 1);
  assert.match(warned[0], /fallback/);
});

test("softIsDefined prefers the __huedeck host logger", () => {
  const got = [];
  globalThis.__huedeck = { plugin: "p", log: (level, m) => got.push([level, m]) };
  Assert.softIsDefined(null, "hosted");
  assert.equal(logs.length, 0);
  assert.deepEqual(got.length, 1);
  assert.equal(got[0][0], "warn");
  assert.match(got[0][1], /hosted/);
});

test("globalContext.set/get/has/delete/keys/snapshot and chaining", () => {
  assert.equal(globalContext.set("Motherboard Config", { leds: 4 }), globalContext);
  globalContext.set("k2", "v2");
  assert.deepEqual(globalContext.get("Motherboard Config"), { leds: 4 });
  assert.equal(globalContext.has("k2"), true);
  assert.deepEqual(globalContext.keys().sort(), ["Motherboard Config", "k2"]);
  const snap = globalContext.snapshot();
  assert.deepEqual(snap, { "Motherboard Config": { leds: 4 }, k2: "v2" });
  snap.k2 = "mutated";
  assert.equal(globalContext.get("k2"), "v2", "snapshot is a copy");
  assert.equal(globalContext.delete("k2"), true);
  assert.equal(globalContext.has("k2"), false);
  globalContext.clear();
  assert.deepEqual(globalContext.keys(), []);
  assert.ok(Object.isFrozen(globalContext));
});

test("ContextError carries call-site context over the globalContext snapshot", () => {
  globalContext.set("Motherboard Config", "B550");
  globalContext.set("shared", "global");
  const e = new ContextError("boom", { shared: "local", extra: 1 });
  assert.equal(e.message, "boom");
  assert.deepEqual(e.context, { "Motherboard Config": "B550", shared: "local", extra: 1 });
  assert.match(e.toString(), /^ContextError: boom \{.*"extra":1.*\}$/);
  assert.ok(e.stack);
  // thrown by Assert as well
  try {
    Assert.isOk(null, "with global");
  } catch (err) {
    assert.equal(err.context["Motherboard Config"], "B550");
  }
});

test("ContextError with no context, a primitive context, or a circular context", () => {
  const plain = new ContextError("plain");
  assert.deepEqual(plain.context, {});
  assert.equal(plain.toString(), "ContextError: plain");
  const prim = new ContextError("prim", 42);
  assert.equal(prim.context.value, 42);
  const circ = { name: "c" };
  circ.self = circ;
  const c = new ContextError("circ", circ);
  assert.match(c.toString(), /\[Circular\]/);
  const withFn = new ContextError("fn", { f() {} });
  assert.match(withFn.toString(), /\[Function f\]/);
  const nomsg = new ContextError();
  assert.equal(nomsg.message, "ContextError");
});
