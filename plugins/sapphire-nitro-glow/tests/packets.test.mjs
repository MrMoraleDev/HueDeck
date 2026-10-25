// SPDX-License-Identifier: GPL-3.0-or-later
//
// Sapphire Nitro Glow plugin: Scan, Initialize, a frame and Shutdown send exactly the register
// writes worked out by hand from OpenRGB's SapphireNitroGlowV3Controller (fixtures/expected.json),
// only to address 0x28, only on AMD GPU buses, and only to the lighting registers.

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
const pairs = (list) => list.map(([r, v]) => [Number(r), Number(v)]);
const startup = Object.fromEntries(Object.entries(expected.startup_registers).map(([r, v]) => [Number(r), Number(v)]));
const RX6700XT = { vendor: 0x1002, device: 0x73DF, subVendor: 0x1DA2, subDevice: 0xE445 };
const NAME = "AMD ADLX Bus - AMD Radeon RX 6700 XT";
const LIGHTING = new Set([0x0F, 0x10, 0x1A, 0x1B, 0x1C]);

async function setup({ color = [0, 0, 0], pci = RX6700XT, kind = "amdgpu", registers = startup, nack = [] } = {}) {
  const device = createMockDevice({ color });
  const mock = createMockBus({ kind, pci, name: NAME, address: 0x28, registers: { 0x28: { ...registers } }, nack });
  const restore = installGlobals(device, mock.fixed);
  const plugin = await loadPlugin(pluginPath);
  return { device, mock, plugin, restore };
}

const writes = (mock) => mock.writes().map((w) => [w.reg, w.data[0]]);

test("scan finds 0x28 only on a listed card's AMD GPU bus that answers", async () => {
  const { plugin, restore } = await setup();
  try {
    const bus = (opts) => createMockBus({ name: NAME, registers: { 0x28: {} }, ...opts }).free;
    assert.deepEqual(plugin.Scan(bus({ kind: "amdgpu", pci: RX6700XT })), [0x28]);
    assert.deepEqual(plugin.Scan(bus({ kind: "amdgpu", pci: { ...RX6700XT, subDevice: 0xE446 } })), []);
    assert.deepEqual(plugin.Scan(bus({ kind: "nvidia", pci: RX6700XT })), []);
    assert.deepEqual(plugin.Scan(bus({ kind: "amd", pci: RX6700XT })), []);
    assert.deepEqual(plugin.Scan(bus({ kind: "amdgpu", pci: RX6700XT, nack: [0x28] })), [], "nobody answers");
    assert.deepEqual(plugin.SmbusAddresses(), [0x28]);
    assert.deepEqual(plugin.SmbusBuses(), ["amdgpu"]);
  } finally {
    restore();
  }
});

test("scan only reads, once, at 0x28", async () => {
  const { plugin, restore } = await setup();
  try {
    const mock = createMockBus({ kind: "amdgpu", pci: RX6700XT, registers: { 0x28: {} } });
    plugin.Scan(mock.free);
    assert.deepEqual(
      mock.ops.map((o) => [o.op, o.addr, o.reg]),
      expected.scan_probe.map((r) => ["ReadByte", 0x28, Number(r)]),
    );
    const other = createMockBus({ kind: "amdgpu", pci: { ...RX6700XT, device: 0x73BF }, registers: { 0x28: {} } });
    plugin.Scan(other.free);
    assert.deepEqual(other.ops, [], "an unlisted card is not probed");
  } finally {
    restore();
  }
});

test("every listed card is a Sapphire Radeon with the V3 address", async () => {
  const { plugin, restore } = await setup();
  try {
    const list = plugin.BrandGPUList();
    assert.equal(list.length, 28);
    for (const g of list) {
      assert.equal(g.Vendor, 0x1002);
      assert.equal(g.SubVendor, 0x1DA2);
      assert.equal(g.Address, 0x28);
      assert.match(g.Name, /^Sapphire Radeon RX /);
    }
    const tuples = new Set(list.map((g) => `${g.Device}:${g.SubDevice}`));
    assert.equal(tuples.size, list.length, "no card twice");
  } finally {
    restore();
  }
});

test("initialize reads the state then selects the custom colour mode", async () => {
  const { mock, plugin, device, restore } = await setup();
  try {
    plugin.Initialize();
    const reads = mock.ops.filter((o) => o.op === "ReadByte").map((o) => o.reg);
    assert.deepEqual(reads, expected.init_reads.map(Number));
    assert.deepEqual(writes(mock), pairs(expected.init_writes));
    assert.ok(device.calls.some((c) => c.method === "setName" && c.args[0] === "Sapphire Radeon RX 6700 XT Nitro+"));
  } finally {
    restore();
  }
});

test("frame is R G B at 0x1A 0x1B 0x1C", async () => {
  const { mock, plugin, restore } = await setup({ color: [0x12, 0x34, 0x56] });
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(writes(mock), pairs(expected.frame_123456));
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
    assert.equal(mock.writes().length, 3);
  } finally {
    restore();
  }
});

test("shutdown restores colour, mode, then external control", async () => {
  const { mock, plugin, restore } = await setup({ color: [9, 9, 9] });
  try {
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(writes(mock), pairs(expected.shutdown_restore));
    for (const [reg, value] of Object.entries(startup)) {
      assert.equal(mock.registers(0x28).get(Number(reg)), value, `register ${reg}`);
    }
  } finally {
    restore();
  }
});

test("a card whose state cannot be read gets the shutdown colour", async () => {
  const { mock, plugin, device, restore } = await setup({ nack: [0x28] });
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(writes(mock), pairs(expected.shutdown_unreadable));
    assert.ok(device.logs.length >= 1, "the failed read is logged");
  } finally {
    restore();
  }
});

test("only lighting registers at 0x28 are ever written", async () => {
  const { mock, plugin, device, restore } = await setup({ color: [5, 6, 7] });
  try {
    plugin.Initialize();
    for (let i = 0; i < 20; i++) {
      device.setColor([i * 10, 255 - i, i]);
      plugin.Render();
    }
    plugin.Shutdown(false);
    plugin.Shutdown(true);
    assert.ok(mock.writes().length > 0);
    assert.ok(mock.writes().every((w) => LIGHTING.has(w.reg)), "lighting registers only");
    assert.ok(mock.ops.every((o) => o.addr === 0x28));
    assert.ok(mock.ops.every((o) => o.op === "ReadByte" || o.op === "WriteByte"));
  } finally {
    restore();
  }
});
