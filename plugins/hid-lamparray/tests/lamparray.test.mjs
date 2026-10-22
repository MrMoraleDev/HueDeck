// SPDX-License-Identifier: GPL-3.0-or-later
//
// HID LampArray plugin against a simulated LampArray device: the report layout comes from
// fixtures/layout.json (what device.getHidReportFields() returns), the device answers attribute
// requests for its lamps, and the tests check the reports the plugin sends.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const layout = JSON.parse(readFileSync(join(here, "..", "fixtures", "layout.json"), "utf8"));

const le = (value, bytes) => Array.from({ length: bytes }, (_, i) => Math.floor(value / 256 ** i) % 256);
const u16 = (bytes, at) => bytes[at] | (bytes[at + 1] << 8);

/** Ten lamps 10 mm apart in a 100 x 40 mm box, every other one 20 mm lower; lamp 9 is not programmable. */
function lampsOf({ redLevels = 256 } = {}) {
  return Array.from({ length: 10 }, (_, id) => ({
    id,
    x: id * 10000,
    y: (id % 2) * 20000,
    levels: [redLevels, 256, 256, 1],
    programmable: id !== 9,
  }));
}

/** Answers like a LampArray device: attributes (report 1) and the lamp asked for (report 2 -> 3). */
function lampArrayDevice(lamps) {
  const attributes = [1, ...le(lamps.length, 2), ...le(100000, 4), ...le(40000, 4), ...le(5000, 4), ...le(1, 4), ...le(4000, 4)];
  const reply = (sent) => {
    if (sent[0] !== 2) return undefined;
    const lamp = lamps[u16(sent, 1)];
    return [3, ...le(lamp.id, 2), ...le(lamp.x, 4), ...le(lamp.y, 4), ...le(0, 4), ...le(1000, 4), ...le(1, 4),
      ...lamp.levels, lamp.programmable ? 1 : 0, 0];
  };
  return { featureReports: { 1: attributes }, featureReply: reply };
}

async function setup({ lamps = lampsOf(), fields = layout, color = [0, 0, 0] } = {}) {
  const device = createMockDevice({ ...lampArrayDevice(lamps), reportFields: fields, color });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

const sentWith = (device, id) => device.sent.filter((s) => s.reportId === id);
const call = (device, method) => device.calls.find((c) => c.method === method);

test("Initialize reads the attributes, asks for every lamp, places the lamps and takes control", async () => {
  const { device, plugin, restore } = await setup();
  try {
    plugin.Initialize();
    assert.deepEqual(sentWith(device, 2).map((s) => u16(s.bytes, 1)), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert.ok(sentWith(device, 2).every((s) => s.length === 3));
    // 100 mm across 32 cells: 3.125 mm per cell, so 32 x 13 cells for the 100 x 40 mm box.
    assert.deepEqual(call(device, "setSize").args[0], [32, 13]);
    const [names, positions] = call(device, "setControllableLeds").args;
    assert.equal(names.length, 10);
    assert.equal(names[3], "Lamp 3");
    assert.deepEqual(positions, [[0, 0], [3, 6], [6, 0], [9, 6], [12, 0], [16, 6], [19, 0], [22, 6], [25, 0], [28, 6]]);
    const control = sentWith(device, 6);
    assert.deepEqual(control.map((s) => s.bytes), [[6, 0]], "autonomous mode off");
    assert.ok(device.logs.some((l) => l.message.includes("10 lamps, keyboard, 100 x 40 mm")));
  } finally {
    restore();
  }
});

test("a frame is one multi-update report per 8 lamps, the last one marked complete", async () => {
  const { device, plugin, restore } = await setup({ color: (x, y) => [x, y, 7] });
  try {
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Render();
    const updates = sentWith(device, 4);
    assert.equal(updates.length, 2);
    const [first, second] = updates.map((s) => s.bytes);
    assert.equal(updates[0].length, 51);
    assert.deepEqual([first[1], first[2]], [8, 0], "8 lamps, not complete");
    assert.deepEqual([second[1], second[2]], [1, 1], "lamp 8 only (lamp 9 is not programmable), complete");
    assert.deepEqual(Array.from({ length: 8 }, (_, k) => u16(first, 3 + 2 * k)), [0, 1, 2, 3, 4, 5, 6, 7]);
    assert.equal(u16(second, 3), 8);
    // Channels start at bit 144 (byte 19 after the id): red, green, blue, intensity per lamp.
    assert.deepEqual(first.slice(19, 27), [0, 0, 7, 255, 3, 6, 7, 255], "lamps 0 and 1");
    assert.deepEqual(second.slice(19, 23), [25, 0, 7, 255], "lamp 8");
    // Unchanged colours are not sent again within a second.
    device.sent.length = 0;
    plugin.Render();
    assert.equal(device.sent.length, 0);
  } finally {
    restore();
  }
});

test("frames closer together than the device's minimum update interval are skipped", async () => {
  const lamps = lampsOf();
  const device = createMockDevice({ ...lampArrayDevice(lamps), reportFields: layout, color: [1, 2, 3] });
  // This device asks for at least 4 ms between updates (attributes report): make it 1 s.
  device.setFeatureReport(1, [1, ...le(10, 2), ...le(100000, 4), ...le(40000, 4), ...le(5000, 4), ...le(1, 4), ...le(1000000, 4)]);
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Render();
    assert.equal(sentWith(device, 4).length, 2, "first frame sent");
    device.setColor([4, 5, 6]);
    plugin.Render();
    assert.equal(sentWith(device, 4).length, 2, "a changed frame 0 ms later waits");
  } finally {
    restore();
  }
});

test("colour channels are scaled to the lamp's level count", async () => {
  const { device, plugin, restore } = await setup({ lamps: lampsOf({ redLevels: 16 }), color: [255, 128, 0] });
  try {
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Render();
    const first = sentWith(device, 4)[0].bytes;
    assert.deepEqual(first.slice(19, 23), [15, 128, 0, 255]);
  } finally {
    restore();
  }
});

test("Shutdown hands the lighting back to the device", async () => {
  const { device, plugin, restore } = await setup();
  try {
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(device.sent.map((s) => s.bytes), [[6, 1]]);
  } finally {
    restore();
  }
});

test("without the multi-update report every lamp gets a range update", async () => {
  const fields = { ...layout, fields: layout.fields.filter((f) => f.reportId !== 4) };
  const { device, plugin, restore } = await setup({ fields, color: [9, 8, 7] });
  try {
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Render();
    const ranges = sentWith(device, 5).map((s) => s.bytes);
    assert.equal(ranges.length, 9);
    assert.deepEqual(ranges[0], [5, 0, 0, 0, 0, 0, 9, 8, 7, 255]);
    assert.deepEqual(ranges[8], [5, 1, 8, 0, 8, 0, 9, 8, 7, 255], "lamp 8, complete");
  } finally {
    restore();
  }
});

test("without a report descriptor nothing is sent", async () => {
  const { device, plugin, restore } = await setup({ fields: null });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.equal(device.sent.length, 0);
    assert.ok(device.logs.some((l) => l.message.includes("report descriptor")));
  } finally {
    restore();
  }
});

test("only LampArray collections validate; the plugin claims them by usage", async () => {
  const { plugin, restore } = await setup();
  try {
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x59, usage: 0x01 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x01, usage: 0x02 }), false);
    assert.deepEqual(plugin.HidUsages(), [[0x59, 0x01]]);
  } finally {
    restore();
  }
});
