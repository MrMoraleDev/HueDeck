// SPDX-License-Identifier: GPL-3.0-or-later
//
// MSI GPU plugin: Scan, Initialize, a frame and Shutdown send exactly the register writes worked
// out by hand from OpenRGB's MSIGPUController (fixtures/expected.json), only to address 0x68,
// and never the flash-save register 0x3F.

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
const RTX3060 = { vendor: 0x10DE, device: 0x2504, subVendor: 0x1462, subDevice: 0x3903 };

async function setup({ color = [0, 0, 0], pci = RTX3060, kind = "nvidia", registers = startup, nack = [] } = {}) {
  const device = createMockDevice({ color });
  const mock = createMockBus({ kind, pci, name: "NVIDIA GeForce RTX 3060", address: 0x68, registers: { 0x68: { ...registers } }, nack });
  const restore = installGlobals(device, mock.fixed);
  const plugin = await loadPlugin(pluginPath);
  return { device, mock, plugin, restore };
}

const writes = (mock) => mock.writes().map((w) => [w.reg, w.data[0]]);

test("scan returns 0x68 only on the matching card", async () => {
  const { plugin, restore } = await setup();
  try {
    const bus = (opts) => createMockBus({ address: 0x68, registers: { 0x68: {} }, ...opts }).free;
    assert.deepEqual(plugin.Scan(bus({ kind: "nvidia", pci: RTX3060 })), [0x68]);
    assert.deepEqual(plugin.Scan(bus({ kind: "nvidia", pci: { ...RTX3060, subDevice: 0x3904 } })), []);
    assert.deepEqual(plugin.Scan(bus({ kind: "amd", pci: RTX3060 })), []);
    assert.deepEqual(plugin.SmbusAddresses(), [0x68]);
    assert.deepEqual(plugin.SmbusBuses(), ["nvidia"]);
    assert.deepEqual(plugin.SmbusDeniedRegisters(), [0x3F], "MSI_GPU_REG_SAVE is refused by the host too");
  } finally {
    restore();
  }
});

test("scan does no I/O", async () => {
  const { plugin, restore } = await setup();
  try {
    const mock = createMockBus({ kind: "nvidia", pci: RTX3060, registers: { 0x68: {} } });
    plugin.Scan(mock.free);
    assert.deepEqual(mock.ops, []);
  } finally {
    restore();
  }
});

test("initialize reads then enters static at full brightness", async () => {
  const { mock, plugin, device, restore } = await setup();
  try {
    plugin.Initialize();
    const reads = mock.ops.filter((o) => o.op === "ReadByte").map((o) => o.reg);
    assert.deepEqual(reads, expected.init_reads.map(Number));
    assert.deepEqual(writes(mock), pairs(expected.init_writes));
    assert.ok(device.calls.some((c) => c.method === "setName" && c.args[0] === "MSI GeForce RTX 3060 Gaming X Trio LHR"));
  } finally {
    restore();
  }
});

test("frame is 0x26 then R1 G1 B1", async () => {
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
    assert.equal(mock.writes().length, 4);
  } finally {
    restore();
  }
});

test("shutdown restores the startup registers, mode last", async () => {
  const { mock, plugin, restore } = await setup({ color: [9, 9, 9] });
  try {
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(writes(mock), pairs(expected.shutdown_restore));
    for (const [reg, value] of Object.entries(startup)) {
      assert.equal(mock.registers(0x68).get(Number(reg)), value, `register ${reg}`);
    }
  } finally {
    restore();
  }
});

test("a card whose state cannot be read gets the static shutdown colour", async () => {
  const { mock, plugin, device, restore } = await setup({ nack: [0x68] });
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

test("register 0x3F is never written", async () => {
  const { mock, plugin, device, restore } = await setup();
  try {
    plugin.Initialize();
    for (let i = 0; i < 20; i++) {
      device.setColor([i * 10, 255 - i, i]);
      plugin.Render();
    }
    plugin.Shutdown(false);
    plugin.Shutdown(true);
    assert.ok(mock.writes().length > 0);
    assert.ok(mock.writes().every((w) => w.reg !== 0x3F));
  } finally {
    restore();
  }
});

test("only address 0x68 is touched", async () => {
  const { mock, plugin, restore } = await setup({ color: [5, 6, 7] });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.ok(mock.ops.every((o) => o.addr === 0x68));
    assert.ok(mock.writes().every((w) => w.op === "WriteByte"), "byte writes only");
  } finally {
    restore();
  }
});
