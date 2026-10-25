// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lenovo Legion 4-zone keyboard plugin: for every product id OpenRGB's detector lists, frames and
// the Spectrum Cycle hand-back are byte for byte the feature reports worked out from OpenRGB's C++
// (fixtures/expected.json); the brightness parameter picks the keyboard's own step; Validate takes
// usage page 0xFF89 / usage 0xCC only; ProductId() equals the manifest; only CC 16 reports are sent.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
const IO = new Set(["write", "send_report", "read", "get_report", "pause"]);

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    const [data, length] = args;
    if (method === "read" || method === "get_report") return { op: method, len: length };
    if (method === "pause") return { op: "pause", ms: args[0] };
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

const canvas = (colours) => (x, y) => (y === 0 && x < 4 ? colours[x] : [0xEE, 0xEE, 0xEE]);

async function setup(pid, colours, params = {}) {
  const device = createMockDevice({ productId: Number(pid), color: canvas(colours) });
  const restore = installGlobals(device, params);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const product of expected.products) {
  test(`${product.pid} ${product.name}: frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await setup(product.pid, expected.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), [], "Initialize sends nothing");
      assert.deepEqual(device.calls.find((c) => c.method === "setName").args, [product.name]);
      plugin.Render();
      assert.deepEqual(io(device), expected.frame_a_high, "frame A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      device.setColor(canvas(expected.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), expected.frame_b_high, "frame B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), expected.shutdown_high, "Shutdown: Spectrum Cycle");
    } finally {
      restore();
    }
  });
}

test("Backlight Level Low sends OpenRGB's default brightness step 1", async () => {
  const { device, plugin, restore } = await setup(0xC965, expected.colours_a, { keyboardBrightness: "Low" });
  try {
    plugin.Initialize();
    plugin.Render();
    assert.deepEqual(io(device), expected.frame_a_low);
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup(0xC965, expected.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(io(device), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(io(device), expected.frame_a_high);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("metadata: ProductId() equals the manifest, Validate takes 0xFF89 / 0xCC only, LEDs in the grid", async () => {
  const { plugin, restore } = await setup(0xC965, expected.colours_a);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.deepEqual(plugin.ProductId(), manifest.devices[0].pids.map(Number));
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF89, usage: 0xCC, collection: 2 }), true);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0xFF89, usage: 0xCC, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF89, usage: 0x07, collection: 1 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x06, collection: 0 }), false);
    const [w, h] = plugin.Size();
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    for (const [x, y] of plugin.LedPositions()) assert.ok(x >= 0 && y >= 0 && x < w && y < h);
  } finally {
    restore();
  }
});

test("only CC 16 feature reports are sent, never anything else", async () => {
  const { device, plugin, restore } = await setup(0xC955, expected.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    for (const op of io(device)) {
      assert.equal(op.op, "send_report");
      assert.ok(op.hex.startsWith("CC16"), op.hex.slice(0, 6));
    }
  } finally {
    restore();
  }
});

test("an unknown product id sends nothing", async () => {
  const { device, plugin, restore } = await setup(0xC964, expected.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});
