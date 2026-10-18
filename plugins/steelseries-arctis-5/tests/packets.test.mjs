// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries Arctis 5 plugin: start, a green frame and the shutdown are what the headset
// received in HueDeck's own recording (fixtures/recorded.json); each ear cup gets its own colour.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const recorded = JSON.parse(readFileSync(join(here, "..", "fixtures", "recorded.json"), "utf8"));
const hex = (list) => list.map((h) => bytesToHex(hexToBytes(h)));

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

test("start, a green frame and the shutdown are byte for byte what the headset received", async () => {
  const { device, plugin, restore } = await setup({ color: [0, 255, 0] });
  try {
    plugin.Initialize();
    assert.deepEqual(device.written.map((w) => bytesToHex(w.bytes)), hex(recorded.start));
    device.written.length = 0;
    plugin.Render();
    assert.deepEqual(device.written.map((w) => bytesToHex(w.bytes)), hex(recorded.frame));
    device.written.length = 0;
    plugin.Shutdown();
    assert.deepEqual(device.written.map((w) => bytesToHex(w.bytes)), hex(recorded.shutdown));
  } finally {
    restore();
  }
});

test("each ear cup gets the colour of its own cell", async () => {
  const { device, plugin, restore } = await setup({ color: (x) => (x === 0 ? [1, 2, 3] : [4, 5, 6]) });
  try {
    plugin.Initialize();
    device.written.length = 0;
    plugin.Render();
    const colours = device.written.filter((w) => w.bytes[1] === 0x8A && w.bytes[5] === 0x41 && w.bytes[6] === 0x00);
    assert.deepEqual(colours.map((w) => w.bytes.slice(7, 10)), [[1, 2, 3], [4, 5, 6]]);
    assert.ok(device.written.every((w) => w.length === 37));
  } finally {
    restore();
  }
});

test("only interface 5 validates", async () => {
  const { plugin, restore } = await setup();
  try {
    assert.equal(plugin.Validate({ interface: 5 }), true);
    assert.equal(plugin.Validate({ interface: 0 }), false);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
});
