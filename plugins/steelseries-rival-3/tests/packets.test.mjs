// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries Rival 3 (wired) plugin: for both product ids OpenRGB's detector lists, the switch to
// Direct, two frames and the shutdown are byte for byte the packets worked out from OpenRGB's C++
// (fixtures/expected.json); unchanged frames are only resent as a keepalive; Validate takes
// interface 3 only; only the colour (0x05) and effect (0x06) commands are ever sent.

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

for (const product of expected.products) {
  test(`${product.pid} ${product.name}: Direct, frames and shutdown are the C++ packets`, async () => {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize: effect Direct");
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
      assert.deepEqual(plugin.Size(), variant.size);
      assert.deepEqual(plugin.LedNames(), variant.leds.map((l) => l.name));
      assert.deepEqual(plugin.LedPositions(), variant.leds.map((l) => l.pos));
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products[0];
  const variant = expected.variants[product.variant];
  const { device, plugin, restore } = await setup(product, variant.colours_a);
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

test("metadata lists every product id once, Validate takes interface 3 only", async () => {
  const { plugin, restore } = await setup(expected.products[0], [[0, 0, 0]]);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.equal(plugin.VendorId(), 0x1038);
    for (const p of expected.products) {
      assert.deepEqual(p.validate, { interface: 3 });
    }
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0002, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("only the colour and effect commands are sent (no save 0x09, no DPI)", async () => {
  for (const product of expected.products) {
    const { device, plugin, restore } = await setup(product, expected.variants[product.variant].colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        assert.ok(["0005", "0006"].includes(op.hex.slice(0, 4)), op.hex);
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
