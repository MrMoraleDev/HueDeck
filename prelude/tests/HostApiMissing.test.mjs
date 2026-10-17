// SPDX-License-Identifier: Apache-2.0 OR MIT
import { test, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import defaultExport, {
  HostApiMissing,
  logOnce,
  resetLogOnce,
  loggedGapCount,
  setLogger,
  missing,
  throwMissing,
  createFacade,
  currentPluginName,
} from "../HostApiMissing.js";

let logs;
let savedHuedeck;
let savedDevice;

beforeEach(() => {
  logs = [];
  savedHuedeck = globalThis.__huedeck;
  savedDevice = globalThis.device;
  delete globalThis.__huedeck;
  delete globalThis.device;
  setLogger((m) => logs.push(m));
  resetLogOnce();
});

afterEach(() => {
  setLogger(null);
  resetLogOnce();
  if (savedHuedeck === undefined) {
    delete globalThis.__huedeck;
  } else {
    globalThis.__huedeck = savedHuedeck;
  }
  if (savedDevice === undefined) {
    delete globalThis.device;
  } else {
    globalThis.device = savedDevice;
  }
});

test("default export is the error class", () => {
  assert.equal(defaultExport, HostApiMissing);
});

test("HostApiMissing fields and message", () => {
  const e = new HostApiMissing("getFrame", { module: "@SignalRGB/lcd", plugin: "Nzxt_Kraken.js", hint: "later" });
  assert.ok(e instanceof Error);
  assert.equal(e.name, "HostApiMissing");
  assert.equal(e.code, "API_GAP");
  assert.equal(e.member, "getFrame");
  assert.equal(e.module, "@SignalRGB/lcd");
  assert.equal(e.plugin, "Nzxt_Kraken.js");
  assert.equal(e.hint, "later");
  assert.equal(e.message, "API_GAP: @SignalRGB/lcd.getFrame is not implemented by this host (plugin: Nzxt_Kraken.js) - later");
  assert.equal(e.gapKey, "Nzxt_Kraken.js:@SignalRGB/lcd:getFrame");
  const bare = new HostApiMissing("foo");
  assert.equal(bare.message, "API_GAP: foo is not implemented by this host");
  assert.equal(bare.module, undefined);
  assert.equal(bare.plugin, undefined);
  assert.equal(bare.gapKey, "?::foo");
});

test("plugin name is taken from globalThis.__huedeck when not given", () => {
  assert.equal(currentPluginName(), undefined);
  globalThis.__huedeck = { plugin: "Razer.js" };
  assert.equal(currentPluginName(), "Razer.js");
  const e = new HostApiMissing("x", { module: "m" });
  assert.equal(e.plugin, "Razer.js");
  assert.match(e.message, /plugin: Razer\.js/);
  globalThis.__huedeck = { plugin: "" };
  assert.equal(currentPluginName(), undefined);
});

test("logOnce emits once per key and counts distinct keys", () => {
  assert.equal(logOnce("a", "first"), true);
  assert.equal(logOnce("a", "again"), false);
  assert.equal(logOnce("b", "second"), true);
  assert.deepEqual(logs, ["first", "second"]);
  assert.equal(loggedGapCount(), 2);
  resetLogOnce();
  assert.equal(loggedGapCount(), 0);
  assert.equal(logOnce("a", "after reset"), true);
});

test("logOnce picks __huedeck.log, then device.log, then service.log, then console.warn", () => {
  setLogger(null);
  const host = [];
  globalThis.__huedeck = { plugin: "p", log: (lvl, m) => host.push([lvl, m]) };
  logOnce("k1", "to host");
  assert.deepEqual(host, [["warn", "to host"]]);
  delete globalThis.__huedeck;
  const dev = [];
  globalThis.device = { log: (m) => dev.push(m) };
  logOnce("k2", "to device");
  assert.deepEqual(dev, ["to device"]);
  delete globalThis.device;
  const origWarn = console.warn;
  const warned = [];
  console.warn = (m) => warned.push(m);
  try {
    logOnce("k3", "to console");
  } finally {
    console.warn = origWarn;
  }
  assert.deepEqual(warned, ["to console"]);
});

test("logOnce survives a throwing logger", () => {
  setLogger(() => {
    throw new Error("sink broke");
  });
  assert.doesNotThrow(() => logOnce("x", "m"));
});

test("missing() builds, logs once and returns; throwMissing throws", () => {
  const e1 = missing("@SignalRGB/udp", "send", { plugin: "Govee.js" });
  const e2 = missing("@SignalRGB/udp", "send", { plugin: "Govee.js" });
  assert.ok(e1 instanceof HostApiMissing);
  assert.equal(logs.length, 1, "same gap logged once");
  assert.notEqual(e1, e2, "a fresh error each time");
  missing("@SignalRGB/udp", "send", { plugin: "Other.js" });
  assert.equal(logs.length, 2, "different plugin is a different gap");
  assert.throws(
    () => throwMissing("@SignalRGB/tcp", "createSocket"),
    (e) => e instanceof HostApiMissing && e.member === "createSocket" && e.module === "@SignalRGB/tcp",
  );
});

test("createFacade: declared members throw HostApiMissing naming the member", () => {
  const f = createFacade("@SignalRGB/demo", ["alpha", "beta"], { hint: "soon" });
  assert.equal(typeof f.alpha, "function");
  assert.equal(f.alpha.name, "@SignalRGB/demo.alpha");
  assert.throws(
    () => f.alpha(1, 2),
    (e) => e instanceof HostApiMissing && e.member === "alpha" && e.module === "@SignalRGB/demo" && e.hint === "soon",
  );
  assert.throws(() => f.beta(), (e) => e.member === "beta");
  assert.equal(logs.length, 2);
  void f.alpha; // property access alone must not log
  assert.equal(logs.length, 2);
  assert.throws(() => f.alpha());
  assert.equal(logs.length, 2, "second call of the same member logs nothing new");
  assert.deepEqual([...f.__huedeckMembers], ["alpha", "beta"]);
  assert.equal(f.__huedeckFacade, "@SignalRGB/demo");
  assert.equal(Object.prototype.toString.call(f), "[object HostApiFacade(@SignalRGB/demo)]");
});

test("createFacade: undeclared members also throw (on call) and are reported with their name", async () => {
  const f = createFacade("@SignalRGB/demo", ["alpha"]);
  assert.equal(typeof f.gamma, "function");
  assert.throws(() => f.gamma(), (e) => e instanceof HostApiMissing && e.member === "gamma");
  assert.equal("gamma" in f, true);
  assert.equal(typeof f.then, "undefined", "promise probing stays inert");
  assert.equal(typeof f.toJSON, "undefined");
  assert.equal(f[Symbol.iterator], undefined);
  assert.doesNotThrow(() => JSON.stringify(f));
  assert.doesNotThrow(() => String(f));
  // Resolving a promise with the facade must not hang (no `then` thenable trap).
  const resolved = await Promise.resolve(f);
  assert.equal(resolved, f);
});

test("createFacade accessThrows: any property read throws", () => {
  const f = createFacade("@SignalRGB/appInfo", [], { accessThrows: true });
  assert.throws(() => f.version, (e) => e instanceof HostApiMissing && e.member === "version");
  assert.throws(() => f.anything, (e) => e.member === "anything");
  assert.equal(typeof f.then, "undefined");
  assert.equal(logs.length, 2);
  assert.throws(() => f.version);
  assert.equal(logs.length, 2, "logged once per member");
});

test("createFacade allows plugin monkey-patching", () => {
  const f = createFacade("@SignalRGB/demo", ["alpha"]);
  f.alpha = () => 42;
  assert.equal(f.alpha(), 42);
  f.custom = "x";
  assert.equal(f.custom, "x");
});
