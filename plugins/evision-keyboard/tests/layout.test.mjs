// SPDX-License-Identifier: GPL-3.0-or-later
//
// EVision V2 keyboards: metadata, key geometry and Validate against OpenRGB's detectors
// (EVisionV2KeyboardControllerDetect.cpp: interface 1, usage page 0xFF1C, any usage) and its
// layout tables (RGBController_EVisionV2Keyboard.cpp: 21 x 6 matrix maps, 106 / 104 keys).
// Cells and slots spot-checked here were read off the C++ tables by hand.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin } from "../../_testkit/mock-device.mjs";
import { EVISION_V2_LAYOUT, ENDORFY_LAYOUT } from "../layouts.js";

const here = dirname(fileURLToPath(import.meta.url));
const pluginDir = join(here, "..");
const manifest = JSON.parse(readFileSync(join(pluginDir, "manifest.json"), "utf8"));

const MODELS = [
  {
    file: "plugin.js", name: "EVision Keyboard 320F:5041", vid: 0x320F, pids: [0x5041], leds: 106,
    layout: EVISION_V2_LAYOUT,
    // [name, column, row, slot] from evisionv2_matrix / evisionv2_map.
    keys: [
      ["Esc", 0, 0, 0], ["F1", 2, 0, 12], ["Pause Break", 16, 0, 96], ["`", 0, 1, 1],
      ["NumLock", 17, 1, 103], ["Num -", 20, 1, 121], ["\\", 13, 2, 80], ["Num +", 20, 2, 122],
      ["ISO_#", 12, 3, 75], ["Enter", 13, 3, 81], ["ISO_<", 1, 4, 10], ["Right Shift", 13, 4, 82],
      ["Up Arrow", 15, 4, 94], ["Num Enter", 20, 4, 124], ["Space", 6, 5, 41], ["Right Win", 11, 5, 71],
      ["Fn", 12, 5, 77], ["Num 0", 18, 5, 113], ["Num .", 19, 5, 119],
    ],
  },
  {
    file: "endorfy-omnis.js", name: "Endorfy Omnis", vid: 0x3299, pids: [0x0012], leds: 104,
    layout: ENDORFY_LAYOUT,
    // [name, column, row, slot] from endorfy_matrix / endorfy_map.
    keys: [
      ["Esc", 0, 0, 0], ["F1", 2, 0, 2], ["F5", 6, 0, 6], ["Pause Break", 16, 0, 16], ["`", 0, 1, 21],
      ["Num -", 20, 1, 41], ["\\", 13, 2, 55], ["Num +", 20, 2, 62], ["Enter", 13, 3, 76],
      ["Left Shift", 0, 4, 84], ["Z", 2, 4, 86], ["Right Shift", 12, 4, 97], ["Up Arrow", 15, 4, 99],
      ["Num Enter", 20, 4, 104], ["Space", 6, 5, 111], ["Fn", 11, 5, 116], ["Menu", 12, 5, 117],
      ["Num 0", 17, 5, 123], ["Num .", 19, 5, 124],
    ],
  },
];

async function load(model) {
  const device = createMockDevice({ productId: model.pids[0], vendorId: model.vid });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(join(pluginDir, model.file));
  return { plugin, restore };
}

for (const model of MODELS) {
  test(`${model.file}: metadata`, async () => {
    const { plugin, restore } = await load(model);
    try {
      assert.equal(plugin.Name(), model.name);
      assert.equal(plugin.Publisher(), "HueDeck");
      assert.equal(plugin.Version(), "0.1.0");
      assert.equal(plugin.Documentation(), "hardware/evision-keyboard");
      assert.equal(plugin.VendorId(), model.vid);
      assert.deepEqual(plugin.ProductId(), model.pids);
      assert.equal(plugin.Type(), "Hid");
      assert.equal(plugin.DeviceType(), "keyboard");
      assert.equal(plugin.ImageUrl(), "");
      const claim = manifest.devices.find((d) => Number(d.vid) === model.vid);
      assert.ok(claim, "the manifest claims the vendor id");
      assert.deepEqual(claim.pids.map(Number), model.pids);
      assert.deepEqual(claim.interfaces, [1]);
      assert.deepEqual(claim.tested, []);
    } finally {
      restore();
    }
  });

  test(`${model.file}: ${model.leds} keys on OpenRGB's 21 x 6 matrix`, async () => {
    const { plugin, restore } = await load(model);
    try {
      const [w, h] = plugin.Size();
      assert.deepEqual([w, h], [21, 6]);
      const names = plugin.LedNames();
      const positions = plugin.LedPositions();
      assert.equal(names.length, model.leds);
      assert.equal(positions.length, model.leds);
      assert.equal(new Set(names).size, names.length, "key names are unique");
      assert.equal(new Set(positions.map(String)).size, positions.length, "one key per cell");
      for (const [x, y] of positions) {
        assert.ok(Number.isInteger(x) && Number.isInteger(y) && x >= 0 && x < w && y >= 0 && y < h, `[${x}, ${y}] inside Size`);
      }
      const slots = model.layout.leds.map((led) => led[3]);
      assert.equal(new Set(slots).size, slots.length, "one key per buffer slot");
      for (const [name, x, y, slot] of model.keys) {
        const i = names.indexOf(name);
        assert.ok(i >= 0, `${name} is a key`);
        assert.deepEqual(positions[i], [x, y], `${name} cell`);
        assert.equal(model.layout.leds[i][3], slot, `${name} slot`);
      }
      // The OpenRGB LED order: Esc first, Num . last.
      assert.equal(names[0], "Esc");
      assert.equal(names[names.length - 1], "Num .");
    } finally {
      restore();
    }
  });

  test(`${model.file}: Validate takes the vendor collection, interface 1 usage page 0xFF1C, any usage`, async () => {
    const { plugin, restore } = await load(model);
    try {
      assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF1C, usage: 0x0092, collection: 4 }), true);
      assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF1C, usage: 0x0001, collection: 0 }), true);
      assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF1C, usage: 0x0092, collection: 0 }), false);
      assert.equal(plugin.Validate({ interface: 1, usage_page: 0x0001, usage: 0x0006, collection: 1 }), false);
      assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000C, usage: 0x0001, collection: 2 }), false);
      assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF1C, usage: 0x0092, collection: 0 }), false);
    } finally {
      restore();
    }
  });
}

test("the EVision V2 key buffer is column-major with cell 6 blank; the Endorfy one is row-major", () => {
  for (const [, x, y, slot] of EVISION_V2_LAYOUT.leds) {
    assert.equal(slot, 6 * x + y);
  }
  assert.ok(!EVISION_V2_LAYOUT.leds.some((led) => led[3] === 6), "OpenRGB's BLANK_SPACE is free");
  assert.deepEqual(ENDORFY_LAYOUT.leds.filter((led) => led[3] === 6).map((led) => led[0]), ["F5"]);
  // Endorfy slots follow row * 21 + column except where OpenRGB draws a wide key one cell left.
  const offGrid = ENDORFY_LAYOUT.leds.filter(([, x, y, slot]) => slot !== 21 * y + x).map((led) => led[0]);
  assert.deepEqual(offGrid, ["Right Shift", "Num 0"]);
});
