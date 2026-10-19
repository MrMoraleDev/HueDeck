// SPDX-License-Identifier: GPL-3.0-or-later
//
// Crucial Ballistix plugin: Scan (remap of a module at 0x27, detection), Initialize, a frame and
// Shutdown send exactly the transfers worked out from OpenRGB's CrucialController
// (fixtures/expected.json), only to the plugin's addresses and never to the SPD EEPROM.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, loadPlugin } from "../../_testkit/mock-device.mjs";
import { createMockBus, installGlobals } from "../../_testkit/mock-bus.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const PIIX4 = { vendor: 0x1022, device: 0x790B, subVendor: 0x1022, subDevice: 0x790B };
const FRAME = expected.frame_colors.map((c) => c.map(Number));

/** Fixture wire transfers with numbers: ["ww", 0x00, 0x0083], ["blk", 0x03, [...]], ["rx", 0x27]. */
const wire = (list) => list.map(([op, cmd, value]) => (value === undefined ? [op, Number(cmd)] : [op, Number(cmd), Array.isArray(value) ? value.map(Number) : Number(value)]));
/** Fixture register-level writes: [[0x820F, 0xCF], ...]. */
const regs = (list) => list.map(([r, v]) => [Number(r), Number(v)]);

/** The mock bus log in the fixture's wire form. */
function toWire(ops) {
  return ops.map((o) => {
    switch (o.op) {
      case "WriteWord":
        return ["ww", o.reg, o.data[0] | (o.data[1] << 8)];
      case "WriteByte":
        return ["wb", o.reg, o.data[0]];
      case "WriteBlock":
        return ["blk", o.reg, o.data.slice(1)];
      case "ReadByte":
        return ["rb", o.reg];
      case "ReadByteWithoutRegister":
        return ["rx", o.addr];
      default:
        return [o.op, o.reg];
    }
  });
}

/** Register-level writes: each Write Word 0x00 (pointer) followed by Write Byte 0x01. */
function registerWrites(ops) {
  const out = [];
  let pointer = null;
  for (const o of ops) {
    if (o.op === "WriteWord" && o.reg === 0x00) {
      pointer = (o.data[0] << 8) | o.data[1];
    } else if (o.op === "WriteByte" && o.reg === 0x01) {
      out.push([pointer, o.data[0]]);
    }
  }
  return out;
}

/** A module's identification bytes and register file: "Micron" at `micronAt`, a version string. */
function crucialModule({ micronAt = 0x1025, ident = (i) => i, version = "BL8G32C16U4WL" } = {}) {
  const direct = {};
  for (let i = 0; i < 16; i++) direct[0xA0 + i] = ident(i);
  const memory = {};
  [..."Micron", "\0"].forEach((ch, i) => (memory[micronAt + i] = ch.charCodeAt(0)));
  [...version.slice(0, 15), "\0"].forEach((ch, i) => (memory[0x1000 + i] = ch.charCodeAt(0)));
  return { direct, memory };
}

async function setup({ color = [0, 0, 0], address = 0x39, kind = "amd", pci = PIIX4, indexed, registers = {}, failBlockAt = [], params = {} } = {}) {
  const device = createMockDevice({ color });
  const mock = createMockBus({ kind, pci, name: "AMD PIIX4 Bus - 0x0b00", address, indexed: indexed ?? { [address]: crucialModule() }, registers, failBlockAt });
  const restore = installGlobals(device, mock.fixed, params);
  const plugin = await loadPlugin(pluginPath);
  return { device, mock, plugin, restore };
}

const perLed = (x) => FRAME[x];

test("scan ignores non-system buses", async () => {
  const { plugin, restore } = await setup();
  try {
    for (const opts of [{ kind: "nvidia" }, { kind: "amd", pci: { ...PIIX4, device: 0x1234 } }, { kind: "nuvoton" }]) {
      const mock = createMockBus({ pci: PIIX4, indexed: { 0x39: crucialModule() }, ...opts });
      assert.deepEqual(plugin.Scan(mock.free), [], JSON.stringify(opts));
      assert.deepEqual(mock.ops, []);
    }
    assert.deepEqual(plugin.SmbusBuses(), ["amd", "intel"]);
  } finally {
    restore();
  }
});

test("scan finds modules already at their address without writing", async () => {
  const { plugin, restore } = await setup();
  try {
    const mock = createMockBus({ kind: "amd", pci: PIIX4, indexed: { 0x39: crucialModule(), 0x3A: crucialModule() } });
    assert.deepEqual(plugin.Scan(mock.free), [0x39, 0x3A]);
    assert.ok(mock.writes().every((w) => w.op === "WriteWord" && w.reg === 0x00), "only pointer writes");
    assert.deepEqual(toWire(mock.ops.slice(0, 1)), [["rx", 0x27]], "the remap probe comes first");
  } finally {
    restore();
  }
});

test("scan remaps a DIMM from 0x27 to the first free address", async () => {
  const { plugin, restore } = await setup();
  try {
    const mock = createMockBus({ kind: "amd", pci: PIIX4, indexed: { 0x27: crucialModule() } });
    assert.deepEqual(plugin.Scan(mock.free), [0x39]);
    assert.deepEqual(mock.indexedAddresses(), [0x39]);
    const fx = expected.scan_remap_slot0;
    assert.deepEqual(toWire(mock.ops.slice(0, 2)), wire(fx.probes));
    const to27 = mock.ops.filter((o) => o.addr === 0x27 && o.op.startsWith("Write"));
    assert.deepEqual(toWire(to27), wire(fx.writes_to_0x27));
    assert.deepEqual(mock.pauses.slice(0, 1), [1]);
  } finally {
    restore();
  }
});

test("scan stops after 4 slots", async () => {
  const { plugin, restore } = await setup();
  try {
    // A Crucial controller at 0x27 that never moves (its apply command is swallowed): four
    // remaps (0x39, 0x3A, 0x3B, 0x3C), then the loop ends.
    const mock = createMockBus({ kind: "amd", pci: PIIX4, indexed: { 0x27: crucialModule() } });
    const send = mock.free.WriteByte;
    mock.free.WriteByte = (a, r, v) => (a === 0x27 && r === 0x01 && v === 0xF0 ? 0 : send(a, r, v));
    plugin.Scan(mock.free);
    const probes27 = mock.ops.filter((o) => o.op === "ReadByteWithoutRegister" && o.addr === 0x27);
    assert.equal(probes27.length, 4);
    const remapped = registerWrites(mock.ops.filter((o) => o.addr === 0x27)).filter(([r]) => r === 0x82EF).map(([, v]) => v >> 1);
    assert.deepEqual(remapped, [0x39, 0x3A, 0x3B, 0x3C]);
  } finally {
    restore();
  }
});

test("a device at 0x27 that is not a Crucial controller is never written", async () => {
  const { plugin, restore } = await setup();
  try {
    for (const registers of [{ 0x27: {} }, { 0x27: Object.fromEntries([...Array(16).keys()].map((i) => [0xA0 + i, i === 3 ? 9 : i])) }]) {
      const mock = createMockBus({ kind: "amd", pci: PIIX4, registers, indexed: {} });
      assert.deepEqual(plugin.Scan(mock.free), []);
      assert.deepEqual(mock.writes(), [], "reads only");
      const at27 = mock.ops.filter((o) => o.addr === 0x27);
      assert.deepEqual(at27.slice(1).map((o) => o.reg), [...Array(16).keys()].map((i) => 0xA0 + i), "identification read first");
      assert.ok(mock.logs.some((l) => l.includes("not remapped")));
    }
  } finally {
    restore();
  }
});

test("every register access holds the bus, in scan, initialize, frames and shutdown", async () => {
  const { mock, plugin, device, restore } = await setup({ color: (x) => perLed(x) });
  try {
    const scanBus = createMockBus({ kind: "amd", pci: PIIX4, indexed: { 0x27: crucialModule(), 0x3A: crucialModule() } });
    plugin.Scan(scanBus.free);
    plugin.Initialize();
    plugin.Render();
    device.setColor([7, 8, 9]);
    plugin.Render();
    plugin.Shutdown(false);
    const pointerAndData = (ops) => ops.filter((o) => o.op !== "ReadByteWithoutRegister" && !(o.op === "ReadByte" && o.reg >= 0xA0 && o.reg < 0xB0));
    for (const o of [...pointerAndData(scanBus.ops), ...pointerAndData(mock.ops)]) {
      assert.ok(o.locked, `${o.op} 0x${o.reg.toString(16)} at 0x${o.addr.toString(16)} outside TransactionLock`);
    }
    assert.equal(mock.isLocked(), false);
    assert.equal(scanBus.isLocked(), false);
  } finally {
    restore();
  }
});

test("nothing is sent when the bus cannot be held, and the frame goes out later", async () => {
  const device = createMockDevice({ color: [1, 2, 3] });
  const busy = createMockBus({ kind: "amd", pci: PIIX4, address: 0x39, indexed: { 0x39: crucialModule() }, lockFails: true });
  const restore = installGlobals(device, busy.fixed, {});
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Render();
    assert.deepEqual(busy.ops, [], "no pointer or plane written without the lock");
    plugin.Initialize();
    plugin.Shutdown(false);
    assert.deepEqual(busy.writes(), []);
  } finally {
    restore();
  }
  const { mock, plugin, restore: restore2 } = await setup({ color: [1, 2, 3] });
  try {
    plugin.Initialize();
    const lock = mock.fixed.TransactionLock;
    mock.fixed.TransactionLock = () => false;
    plugin.Render();
    mock.fixed.TransactionLock = lock;
    mock.ops.length = 0;
    plugin.Render();
    assert.equal(mock.writes().length, 6, "the skipped frame is sent by the next Render()");
  } finally {
    restore2();
  }
});

test("detection needs A0..AF and the Micron string at 0x1025 or 0x1030", async () => {
  const { plugin, restore } = await setup();
  try {
    const scan = (indexed, registers = {}) => {
      const mock = createMockBus({ kind: "amd", pci: PIIX4, indexed, registers });
      return { found: plugin.Scan(mock.free), mock };
    };
    assert.deepEqual(scan({ 0x3B: crucialModule({ micronAt: 0x1030 }) }).found, [0x3B]);
    assert.deepEqual(scan({ 0x3B: crucialModule({ micronAt: 0x1040 }) }).found, []);
    assert.deepEqual(scan({ 0x3B: crucialModule({ ident: (i) => (i === 5 ? 0x55 : i) }) }).found, []);
    assert.deepEqual(scan({}, { 0x20: {} }).found, [], "a plain device at 0x20 is not a Crucial module");
    const { mock } = scan({ 0x39: crucialModule() });
    const fx = expected.detect_ok;
    const at39 = mock.ops.filter((o) => o.addr === 0x39);
    assert.deepEqual(toWire(at39.slice(0, 1)), [["rx", 0x39]]);
    assert.deepEqual(at39.slice(1, 17).map((o) => o.reg), Object.keys(fx.identification).map(Number));
    assert.deepEqual(toWire(at39.slice(17, 19)), wire(fx.first_micron_read));
    const pointers = at39.filter((o) => o.op === "WriteWord").map((o) => (o.data[0] << 8) | o.data[1]);
    assert.deepEqual(pointers, fx.micron_registers.map(Number));
  } finally {
    restore();
  }
});

test("initialize enters static then writes effect colours", async () => {
  const { mock, plugin, device, restore } = await setup({ color: (x) => perLed(x) });
  try {
    plugin.Initialize();
    const fx = expected.init_effect_static;
    const pointers = mock.ops.filter((o) => o.op === "WriteWord").slice(0, 16).map((o) => (o.data[0] << 8) | o.data[1]);
    assert.deepEqual(pointers, fx.version_registers.map(Number), "the version string is read first");
    assert.deepEqual(registerWrites(mock.ops), regs(fx.registers));
    assert.ok(device.calls.some((c) => c.method === "setName" && c.args[0] === "Crucial DRAM 0x39 (BL8G32C16U4WL)"));
  } finally {
    restore();
  }
});

test("frame is three locked block writes with byte-swapped register pointers", async () => {
  const { mock, plugin, restore } = await setup({ color: (x) => perLed(x) });
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(toWire(mock.ops), wire(expected.frame_planes));
    assert.ok(mock.ops.every((o) => o.locked), "every transfer under TransactionLock");
    assert.equal(mock.isLocked(), false);
    assert.deepEqual([...Array(8).keys()].map((i) => mock.indexedRegisters(0x39).get(0x8340 + i)), FRAME.map((c) => c[1]));
  } finally {
    restore();
  }
});

test("block failure falls back to byte writes at 0x01", async () => {
  const { mock, plugin, restore } = await setup({ color: (x) => perLed(x), failBlockAt: [0x39] });
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(toWire(mock.ops), wire(expected.fallback_bytes.wire));
    assert.equal(mock.isLocked(), false);
  } finally {
    restore();
  }
});

test("lock is released even when a write throws", async () => {
  const { mock, plugin, restore } = await setup({ color: [1, 2, 3] });
  try {
    plugin.Initialize();
    mock.fixed.WriteBlock = () => {
      throw new RangeError("WriteBlock(): boom");
    };
    assert.throws(() => plugin.Render(), RangeError);
    assert.equal(mock.isLocked(), false);
  } finally {
    restore();
  }
});

test("unchanged frame writes nothing", async () => {
  const { mock, plugin, device, restore } = await setup({ color: [1, 2, 3] });
  try {
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(mock.ops, []);
    device.setColor([1, 2, 4]);
    plugin.Render();
    assert.equal(mock.writes().length, 6);
  } finally {
    restore();
  }
});

test("shutdown leaves the shutdown colour in static mode", async () => {
  for (const [params, suspending] of [[{}, false], [{ shutdownColor: "#000000" }, false], [{ shutdownColor: "#FF8000" }, true]]) {
    const { mock, plugin, restore } = await setup({ color: [9, 9, 9], params });
    try {
      plugin.Initialize();
      plugin.Render();
      mock.ops.length = 0;
      plugin.Shutdown(suspending);
      assert.deepEqual(registerWrites(mock.ops), regs(expected.shutdown.registers), JSON.stringify(params));
    } finally {
      restore();
    }
  }
  const { mock, plugin, restore } = await setup({ params: { shutdownColor: "#FF8000" } });
  try {
    plugin.Shutdown(false);
    const writes = registerWrites(mock.ops);
    assert.deepEqual(writes.filter(([r]) => r === 0x82ED).map(([, v]) => v), Array(8).fill(0xFF));
    assert.deepEqual(writes.filter(([r]) => r === 0x82EE).slice(0, 8).map(([, v]) => v), Array(8).fill(0x80));
  } finally {
    restore();
  }
});

test("no SPD (0x50-0x57) or write-protect (0x30-0x37) address is ever touched", async () => {
  const { mock, plugin, device, restore } = await setup({ color: (x) => perLed(x) });
  try {
    const scanBus = createMockBus({ kind: "amd", pci: PIIX4, indexed: { 0x27: crucialModule(), 0x3A: crucialModule() }, registers: { 0x50: {}, 0x36: {} } });
    plugin.Scan(scanBus.free);
    plugin.Initialize();
    for (let i = 0; i < 5; i++) {
      device.setColor([i, 2 * i, 3 * i]);
      plugin.Render();
    }
    plugin.Shutdown(false);
    const window = new Set(plugin.SmbusAddresses());
    for (const o of [...scanBus.ops, ...mock.ops]) {
      assert.ok(!(o.addr >= 0x50 && o.addr <= 0x57) && !(o.addr >= 0x30 && o.addr <= 0x37), `0x${o.addr.toString(16)}`);
      assert.ok(window.has(o.addr), `0x${o.addr.toString(16)} is in SmbusAddresses()`);
    }
  } finally {
    restore();
  }
});
