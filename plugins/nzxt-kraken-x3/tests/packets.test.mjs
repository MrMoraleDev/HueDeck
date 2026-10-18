// SPDX-License-Identifier: MIT
//
// NZXT Kraken X3 plugin: the packets match what the device received in Lightloom's own recording
// (fixtures/recorded.json), the pump ring keeps the device's LED order, and the header channel
// is padded or cut to 40 LEDs.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const recorded = JSON.parse(readFileSync(join(here, "..", "fixtures", "recorded.json"), "utf8"));

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

function written(device) {
  return device.written.map((w) => bytesToHex(w.bytes));
}

test("a green frame and the shutdown are byte for byte what the device received", async () => {
  const green = [0, 255, 0];
  const { device, plugin, restore } = await setup({
    color: green,
    channelColors: { "Channel 1": new Array(20).fill(green) },
  });
  try {
    plugin.Initialize();
    assert.deepEqual(device.channels, [{ name: "Channel 1", ledLimit: 40, defaultLedCount: undefined }]);
    plugin.Render();
    assert.deepEqual(written(device), recorded.frame.map((h) => bytesToHex(hexToBytes(h))));
    device.written.length = 0;
    plugin.Shutdown();
    assert.deepEqual(written(device), recorded.shutdown.map((h) => bytesToHex(hexToBytes(h))));
  } finally {
    restore();
  }
});

test("the pump ring goes out in the device's order and the logo in its own packet", async () => {
  // Every grid cell gets its own colour: R = x * 10, G = y * 10, B = 1.
  const { device, plugin, restore } = await setup({ color: (x, y) => [x * 10, y * 10, 1] });
  try {
    plugin.Initialize();
    plugin.Render();
    const ring = device.written[3].bytes;
    const positions = plugin.LedPositions();
    for (let i = 0; i < 8; i++) {
      const [x, y] = positions[i];
      assert.deepEqual(ring.slice(4 + i * 3, 7 + i * 3), [y * 10, x * 10, 1], `ring LED ${i}`);
    }
    assert.deepEqual(device.written[6].bytes.slice(0, 10), [0x2A, 0x04, 0x04, 0x04, 0x00, 0x32, 0x00, 20, 20, 1]);
    assert.equal(device.written.length, 7);
    assert.ok(device.written.every((w) => w.length === 64));
  } finally {
    restore();
  }
});

test("the header channel is padded to 40 LEDs and never longer", async () => {
  const many = Array.from({ length: 45 }, (_, i) => [i, 0, 0]);
  const { device, plugin, restore } = await setup({ channelColors: { "Channel 1": many } });
  try {
    plugin.Initialize();
    plugin.Render();
    const first = device.written[0].bytes;
    const second = device.written[1].bytes;
    // LED 0 is (r=0): G R B = 0 0 0; LED 39 sits at the end of the second packet.
    assert.deepEqual(first.slice(4, 10), [0, 0, 0, 0, 1, 0]);
    assert.deepEqual(second.slice(4 + 19 * 3, 4 + 20 * 3), [0, 39, 0]);
  } finally {
    restore();
  }
});

test("only interface 0 (or a device without interfaces) validates", async () => {
  const { plugin, restore } = await setup();
  try {
    assert.equal(plugin.Validate({ interface: 0 }), true);
    assert.equal(plugin.Validate({ interface: -1 }), true);
    assert.equal(plugin.Validate({ interface: 1 }), false);
    assert.deepEqual(plugin.ProductId(), [0x2007]);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
});
