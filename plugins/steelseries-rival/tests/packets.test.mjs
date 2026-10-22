// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries Rival (older protocol) plugin: for every product id OpenRGB's detector lists, the
// switch to Direct, two frames and the shutdown are byte for byte the packets worked out from
// OpenRGB's C++ (fixtures/expected.json); unchanged frames are only resent as a keepalive;
// Validate takes interface 0 only; the save command is never sent; the held-back Rival 650 ids
// are not claimed and drive nothing.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const IO = new Set(["write", "send_report", "read", "pause"]);

/** The I/O calls a plugin made, in order, in the fixture's shape. */
function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

/** A canvas giving each LED cell of `variant` its colour from `colours`. */
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
  test(`${product.pid} ${product.name}: Direct, frames and shutdown are the C++ packets`, async () => {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame with colour set B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown: black");
    } finally {
      restore();
    }
  });

  test(`${product.pid} ${product.name}: name and LED grid`, async () => {
    const { device, plugin, restore, variant } = await setup(product, [[0, 0, 0]]);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [product.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
      for (const [x, y] of variant.leds.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
      }
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products.find((p) => p.variant === "RIVAL_300");
  const { device, plugin, restore, variant } = await setup(product, expected.variants.RIVAL_300.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(io(device), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_a);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("metadata lists every product id once, Validate takes interface 0 only", async () => {
  const { plugin, restore } = await setup(expected.products[0], [[0, 0, 0]]);
  try {
    assert.deepEqual(plugin.ProductId(), live.map((p) => Number(p.pid)));
    assert.equal(new Set(plugin.ProductId()).size, plugin.ProductId().length);
    assert.equal(plugin.VendorId(), 0x1038);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    for (const p of live) {
      assert.deepEqual(p.validate, { interface: 0 });
    }
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0002, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFFC0, usage: 0x0001, collection: 2 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("the save command (0x09) is never sent", async () => {
  for (const product of live) {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        assert.ok(!(op.op === "write" && op.hex.startsWith("0009")), `${product.pid}: ${op.hex.slice(0, 8)}`);
      }
    } finally {
      restore();
    }
  }
});

test("the held-back Rival 650 ids are not claimed and drive nothing", async () => {
  assert.deepEqual(heldBack.map((p) => p.pid).sort(), ["0x1726", "0x172B"]);
  for (const product of heldBack) {
    const device = createMockDevice({ productId: Number(product.pid) });
    const restore = installGlobals(device);
    try {
      const plugin = await loadPlugin(pluginPath);
      assert.ok(!plugin.ProductId().includes(Number(product.pid)), product.pid);
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(false);
      assert.deepEqual(io(device), [], product.pid);
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
