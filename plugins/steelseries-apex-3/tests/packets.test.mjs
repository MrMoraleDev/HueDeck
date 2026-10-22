// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries Apex 3 plugin: for both product ids OpenRGB's detector lists (Apex 3: ten zones,
// 33-byte reports; Apex 3 TKL: eight zones, 65-byte reports), the switch to Direct, two frames
// and the shutdown are byte for byte the packets worked out from OpenRGB's C++
// (fixtures/expected.json); unchanged frames are only resent as a keepalive; Validate follows the
// two detector lines; the save command is never sent.

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

for (const product of expected.products) {
  test(`${product.pid} ${product.name}: Direct, frames and shutdown are the C++ packets`, async () => {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize: OpenRGB's DeviceUpdateMode");
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
      assert.deepEqual(io(device), variant.shutdown, "Shutdown");
    } finally {
      restore();
    }
  });

  test(`${product.pid} ${product.name}: name, LED grid and Validate`, async () => {
    const { device, plugin, restore, variant } = await setup(product, []);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [product.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
      const v = product.validate;
      const good = { interface: v.interface, usage_page: v.usage_page ?? 0x000C, usage: v.usage ?? 0x0001, collection: 0 };
      assert.equal(plugin.Validate(good), true);
      assert.equal(plugin.Validate({ ...good, interface: 0 }), false);
      if (v.usage_page !== undefined) {
        assert.equal(plugin.Validate({ ...good, usage_page: 0x0001, usage: 0x0006 }), false);
      } else {
        assert.equal(plugin.Validate({ ...good, usage_page: 0x0001, usage: 0x0006, collection: 2 }), true);
      }
    } finally {
      restore();
    }
  });
}

test("detector lines: Apex 3 any collection of interface 3, TKL interface 1 collection 0xFFC0/0x0001", () => {
  assert.deepEqual(expected.products.map((p) => [p.pid, p.validate]), [
    ["0x161A", { interface: 3 }],
    ["0x1622", { interface: 1, usage_page: 0xFFC0, usage: 1 }],
  ]);
});

test("Validate takes only the bound product's own detector line, nothing for an unknown product", async () => {
  for (const [pid, other] of [[0x161A, { interface: 1, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }],
    [0x1622, { interface: 3, usage_page: 0x0001, usage: 0x0006, collection: 0 }]]) {
    const device = createMockDevice({ productId: pid });
    const restore = installGlobals(device);
    try {
      const plugin = await loadPlugin(pluginPath);
      assert.equal(plugin.Validate(other), false, `0x${pid.toString(16)} must not take the other product's endpoint`);
    } finally {
      restore();
    }
  }
  const device = createMockDevice({ productId: 0 });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0001, usage: 0x0006, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  for (const product of expected.products) {
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
      // The TKL's brightness report went out with the switch to Direct and is not repeated.
      assert.deepEqual(io(device), variant.frame_a);
    } finally {
      restore();
    }
  }
  mock.timers.reset();
});

test("metadata and commands: only colour, brightness and rainbow-wave reports, never save", async () => {
  const { plugin, restore } = await setup(expected.products[0], []);
  try {
    assert.deepEqual(plugin.ProductId(), [0x161A, 0x1622]);
    assert.equal(plugin.VendorId(), 0x1038);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
  for (const product of expected.products) {
    const { device, plugin, restore: undo } = await setup(product, Array(10).fill([1, 2, 3]));
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        assert.equal(op.op, "write");
        assert.ok(["000A", "000B", "0021", "0022", "0023"].includes(op.hex.slice(0, 4)), op.hex);
      }
    } finally {
      undo();
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
