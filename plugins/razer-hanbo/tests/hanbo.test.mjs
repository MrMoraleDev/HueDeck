// SPDX-License-Identifier: GPL-3.0-or-later
//
// Razer Hanbo Chroma plugin: the output reports of Initialize / Render / Shutdown against the ones
// worked out from OpenRGB's C++ code (fixtures/openrgb-packets.json), Validate, geometry.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const fx = JSON.parse(readFileSync(join(here, "..", "fixtures", "openrgb-packets.json"), "utf8"));
const hex = (h) => bytesToHex(hexToBytes(h));
const canvas = (x, y) => [(x * 10 + 1) & 0xFF, (y * 20 + 2) & 0xFF, (x * 3 + y * 7 + 5) & 0xFF];

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

function drain(device) {
  const out = device.written.map((s) => bytesToHex(s.bytes));
  device.written.length = 0;
  return out;
}

test("direct mode, a frame (pump, three fans; G R B) and the spectrum cycle on exit", async () => {
  const { device, plugin, restore } = await setup({ productId: Number(fx.pid), color: canvas });
  try {
    plugin.Initialize();
    assert.deepEqual(drain(device), fx.initialize.map(hex), "initialize");
    assert.equal(device.calls.filter((c) => c.method === "read").length, 4, "each direct-mode command is answered");
    plugin.Render();
    assert.deepEqual(drain(device), fx.frame.map(hex), "frame");
    plugin.Shutdown(false);
    assert.deepEqual(drain(device), fx.shutdown.map(hex), "shutdown");
    assert.ok(device.written.every((w) => w.length === 65));
    assert.equal(device.sent.length, 0);
  } finally {
    restore();
  }
});

test("Validate: interface 0, usage page 0xFF00, usage 0x0001 only", async () => {
  const { plugin, restore } = await setup({ productId: 0x0F35 });
  try {
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x01, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x01, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x01, collection: 0 }), false);
    assert.deepEqual(plugin.ProductId(), [0x0F35]);
  } finally {
    restore();
  }
});

test("geometry: 16 pump LEDs and 18 per fan, one row each", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0F35 });
  try {
    plugin.Initialize();
    assert.deepEqual(device.calls.find((c) => c.method === "setSize").args[0], [18, 4]);
    const [names, positions] = device.calls.find((c) => c.method === "setControllableLeds").args;
    assert.equal(names.length, 70);
    assert.equal(names[0], "Pump LED 1");
    assert.equal(names[16], "Fan 1 LED 1");
    assert.deepEqual(positions[16], [0, 1]);
    assert.deepEqual(plugin.Size(), [18, 4]);
  } finally {
    restore();
  }
});
