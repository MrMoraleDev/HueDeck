// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries Arctis 5 (OpenRGB protocol) plugin: for 1038:1250, frames (two colour sets and plain
// green) and the shutdown are byte for byte the packets worked out from OpenRGB's C++
// (fixtures/expected.json); 1038:12AA, which OpenRGB also lists, is held back (owned by
// plugins/steelseries-arctis-5): not claimed, drives nothing; every frame is sent (OpenRGB's Direct mode has
// no keepalive logic and the headset was recorded taking every frame); Validate takes interface 5
// only; nothing but the six lighting reports of SetColor is ever written.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const IO = new Set(["write", "send_report", "read", "pause"]);

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

function canvas(variant, colours) {
  const byCell = new Map(variant.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(product, colours) {
  const variant = expected.variants[product.variant];
  const device = createMockDevice({ productId: Number(product.pid), color: canvas(variant, colours) });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore, variant };
}

const live = expected.products.filter((p) => !p.held_back);
const heldBack = expected.products.filter((p) => p.held_back);

for (const product of live) {
  test(`${product.pid} ${product.name}: frames and shutdown are the C++ packets`, async () => {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize sends nothing (DeviceUpdateMode is empty)");
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "the same frame again is sent again");
      device.calls.length = 0;
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame with colour set B");
      device.calls.length = 0;
      device.setColor(canvas(variant, variant.colours_green));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_green, "green frame");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown: black");
    } finally {
      restore();
    }
  });

  test(`${product.pid} ${product.name}: name and LED grid`, async () => {
    const { device, plugin, restore, variant } = await setup(product, [[0, 0, 0], [0, 0, 0]]);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [product.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
      assert.deepEqual(plugin.Size(), variant.size);
      assert.deepEqual(plugin.LedNames(), variant.leds.map((l) => l.name));
      assert.deepEqual(plugin.LedPositions(), variant.leds.map((l) => l.pos));
    } finally {
      restore();
    }
  });
}

test("metadata lists 0x1250 only, Validate takes interface 5 only", async () => {
  const { plugin, restore } = await setup(live[0], [[0, 0, 0], [0, 0, 0]]);
  try {
    assert.deepEqual(plugin.ProductId(), live.map((p) => Number(p.pid)));
    assert.deepEqual(plugin.ProductId(), [0x1250]);
    assert.equal(plugin.VendorId(), 0x1038);
    for (const p of expected.products) {
      assert.deepEqual(p.validate, { interface: 5 });
    }
    assert.equal(plugin.Validate({ interface: 5, usage_page: 0x000C, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 5, usage_page: 0xFFC0, usage: 0x0001, collection: 1 }), true);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("1038:12AA is held back: not claimed (manifest or ProductId), drives nothing", async () => {
  assert.deepEqual(heldBack.map((p) => p.pid), ["0x12AA"]);
  const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
  assert.ok(manifest.devices.every((d) => !d.pids.map((p) => p.toUpperCase()).includes("0X12AA")));
  const device = createMockDevice({ productId: 0x12AA });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.ok(!plugin.ProductId().includes(0x12AA));
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});

test("only the lighting reports of SetColor are written (report 0x06, 37 bytes)", async () => {
  for (const product of live) {
    const { device, plugin, restore } = await setup(product, [[1, 2, 3], [4, 5, 6]]);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        assert.equal(op.op, "write");
        assert.equal(op.len, 37);
        assert.ok(["0681430122", "0681430123", "068A420020"].includes(op.hex.slice(0, 10)), op.hex);
      }
    } finally {
      restore();
    }
  }
});

test("an unknown product id drives nothing", async () => {
  const device = createMockDevice({ productId: 0x1234 });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
    assert.equal(device.logs.length, 1);
  } finally {
    restore();
  }
});
