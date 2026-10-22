// SPDX-License-Identifier: GPL-3.0-or-later
//
// Razer Kraken plugin: the output reports of Initialize / Render / Shutdown against the ones
// worked out from OpenRGB's C++ code (fixtures/openrgb-packets.json), Validate, geometry.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";
import { DETECTORS } from "../devices.js";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const fixtures = JSON.parse(readFileSync(join(here, "..", "fixtures", "openrgb-packets.json"), "utf8"));
const hex = (h) => bytesToHex(hexToBytes(h));
const canvas = (x, y) => [(x * 10 + 1) & 0xFF, (y * 20 + 2) & 0xFF, (x * 3 + y * 7 + 5) & 0xFF];

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

function drain(device) {
  const out = [...device.written, ...device.sent].map((s) => bytesToHex(s.bytes));
  device.written.length = 0;
  device.sent.length = 0;
  return out;
}

for (const c of fixtures.cases) {
  test(`${c.name} (${c.pid}): packets of Initialize, a frame and Shutdown`, async () => {
    const { device, plugin, restore } = await setup({ productId: Number(c.pid), color: canvas });
    try {
      plugin.Initialize();
      assert.deepEqual(drain(device), c.initialize.map(hex), "initialize");
      plugin.Render();
      assert.deepEqual(drain(device), c.frame.map(hex), "frame");
      plugin.Render();
      assert.deepEqual(drain(device), [], "an unchanged frame is not sent again at once");
      plugin.Shutdown(false);
      assert.deepEqual(drain(device), c.shutdown.map(hex), "shutdown");
    } finally {
      restore();
    }
  });
}

test("Validate: interface 3, usage page 0x000C, usage 0x0001 for every product", async () => {
  for (const [, pid] of DETECTORS) {
    const { plugin, restore } = await setup({ productId: pid });
    try {
      assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0C, usage: 0x01, collection: 0 }), true);
      assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0C, usage: 0x02, collection: 0 }), false);
      assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0C, usage: 0x01, collection: 0 }), false);
      assert.equal(plugin.Validate({ interface: 3, usage_page: 0x59, usage: 0x01, collection: 0 }), false);
    } finally {
      restore();
    }
  }
  const { plugin, restore } = await setup({ productId: 0x0084 });
  try {
    assert.equal(plugin.ProductId().length, 10);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0C, usage: 0x01, collection: 0 }), false, "not a Kraken");
  } finally {
    restore();
  }
});

test("geometry: Kraken 7.1 is one LED, the Kitty V2 Pro four", async () => {
  for (const [pid, names] of [[0x0501, ["Headset"]], [0x0560, ["Headset"]],
    [0x0554, ["Left Cat Ear", "Right Cat Ear", "Left Logo", "Right Logo"]]]) {
    const { device, plugin, restore } = await setup({ productId: pid });
    try {
      plugin.Initialize();
      const leds = device.calls.find((c) => c.method === "setControllableLeds").args;
      assert.deepEqual(leds[0], names);
      assert.deepEqual(leds[1], names.map((_, i) => [0, i]));
    } finally {
      restore();
    }
  }
});

test("Kraken 7.1: a changed colour within 250 ms of the last write is held back (memory writes)", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0504, color: [255, 0, 0] });
  try {
    plugin.Initialize();
    plugin.Render();
    drain(device);
    device.setColor([0, 255, 0]);
    plugin.Render();
    assert.deepEqual(drain(device), [], "changed frame inside the minimum interval is not written");
  } finally {
    restore();
  }
});
