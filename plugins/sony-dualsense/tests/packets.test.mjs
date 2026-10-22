// SPDX-License-Identifier: GPL-3.0-or-later
//
// Sony DualSense plugin: the start, a frame and the shutdown match the reports written by hand
// from the documented field table (fixtures/expected.json), and no report ever enables a field
// other than the light bar (volume, audio, vibration, triggers, microphone and player LEDs).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const hex = (list) => list.map((h) => bytesToHex(hexToBytes(h)));

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

test("start, a frame and the shutdown are the documented reports", async () => {
  const { device, plugin, restore } = await setup({ color: [0x12, 0x34, 0x56] });
  try {
    plugin.Initialize();
    assert.deepEqual(device.written.map((w) => bytesToHex(w.bytes)), hex(expected.start));
    device.written.length = 0;
    plugin.Render();
    assert.deepEqual(device.written.map((w) => bytesToHex(w.bytes)), hex(expected.frame_123456));
    device.written.length = 0;
    plugin.Shutdown();
    assert.deepEqual(device.written.map((w) => bytesToHex(w.bytes)), hex(expected.shutdown));
  } finally {
    restore();
  }
});

test("only the light bar is ever enabled", async () => {
  const { device, plugin, restore } = await setup({ color: [255, 128, 1] });
  try {
    plugin.Initialize();
    for (let i = 0; i < 5; i++) plugin.Render();
    plugin.Shutdown();
    assert.ok(device.written.length > 0);
    for (const { bytes, length } of device.written) {
      assert.equal(length, 48);
      assert.equal(bytes[0], 0x02, "report id");
      assert.equal(bytes[1], 0x00, "valid_flag0: no vibration, triggers, volume or audio routing");
      assert.equal(bytes[2] & ~0x04, 0x00, "valid_flag1: light bar colour only");
      assert.equal(bytes[39] & ~0x02, 0x00, "valid_flag2: light bar setup only");
      assert.deepEqual(bytes.slice(3, 11), [0, 0, 0, 0, 0, 0, 0, 0], "motors, volumes, audio, mute LED, power save");
      assert.equal(bytes[44], 0x00, "player LEDs");
    }
  } finally {
    restore();
  }
});

test("only the gamepad collection of interface 3 validates", async () => {
  const { plugin, restore } = await setup();
  try {
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0001, usage: 0x0005 }), true);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x000C, usage: 0x0001 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0005 }), false);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
});
