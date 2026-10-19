// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries Siberia 350 plugin: two frames and the shutdown are byte for byte the reports worked
// out from OpenRGB's C++ (fixtures/expected.json); an unchanged colour is never resent (no
// keepalive); Validate takes interface 3 only; only OpenRGB's four commands are ever sent.

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

async function setup(product, colour) {
  const device = createMockDevice({ productId: Number(product.pid), color: colour });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

const product = expected.products[0];
const variant = expected.variants[product.variant];

test(`${product.pid} ${product.name}: frames and shutdown are the C++ packets`, async () => {
  const { device, plugin, restore } = await setup(product, variant.colours_a[0]);
  try {
    plugin.Initialize();
    assert.deepEqual(io(device), variant.initialize, "Initialize sends nothing of its own");
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_a, "frame with colour A");
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), [], "an unchanged colour is not resent");
    device.setColor(variant.colours_b[0]);
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_b, "frame with colour B");
    device.calls.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(io(device), variant.shutdown, "Shutdown: black");
  } finally {
    restore();
  }
});

test("an unchanged colour is never resent (no keepalive)", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup(product, variant.colours_a[0]);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(60_000);
    plugin.Render();
    assert.deepEqual(io(device), []);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("name, LED, metadata and Validate (interface 3 only)", async () => {
  const { device, plugin, restore } = await setup(product, [0, 0, 0]);
  try {
    plugin.Initialize();
    const call = (m) => device.calls.find((c) => c.method === m)?.args;
    assert.deepEqual(call("setName"), [product.name]);
    assert.deepEqual(call("setSize"), [variant.size]);
    assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
    assert.deepEqual(plugin.ProductId(), [0x1229]);
    assert.equal(plugin.VendorId(), 0x1038);
    assert.deepEqual(product.validate, { interface: 3 });
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x000C, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("only OpenRGB's four commands are sent, in report 0x01", async () => {
  const { device, plugin, restore } = await setup(product, [9, 8, 7]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    for (const op of io(device)) {
      assert.equal(op.op, "write");
      assert.equal(op.len, 16);
      assert.ok(["010095", "010080", "010083", "010093"].includes(op.hex.slice(0, 6)), op.hex);
    }
  } finally {
    restore();
  }
});
