// SPDX-License-Identifier: GPL-3.0-or-later
//
// Roccat mouse plugin: for every product id it claims, Initialize, two frames and Shutdown are
// byte for byte the I/O worked out from OpenRGB's C++ (fixtures/expected.json); unchanged frames
// are only resent as a keepalive; Validate takes the one collection OpenRGB's detector names;
// ProductId() equals the manifest; the profile report (0x06) and the profile select (0x04 write)
// are never sent; the held-back Burst Pro Air, Kova, Sense Aimo and Elo ids are not claimed.

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
const IO = new Set(["write", "send_report", "get_report", "read", "pause"]);

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    if (method === "get_report") return { op: "get_report", id: args[0][0], len: args[1] };
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
  const device = createMockDevice({
    productId: Number(product.pid),
    color: canvas(variant, colours),
    featureReports: { 4: [0x04, 0x01, 0x00, 0x00], 0x0D: [0x0D, 0x2E, 0x00] },
  });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore, variant };
}

for (const product of expected.products) {
  test(`${product.pid} ${product.name}: start, frames and shutdown are the C++ I/O`, async () => {
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
      assert.deepEqual(io(device), variant.shutdown, "Shutdown");
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

  test(`${product.pid} ${product.name}: Validate takes interface ${product.validate.interface}, page ${product.validate.usage_page}, usage ${product.validate.usage} only`, async () => {
    const { plugin, restore } = await setup(product, [[0, 0, 0]]);
    try {
      const v = product.validate;
      const good = { interface: v.interface, usage_page: v.usage_page, usage: v.usage, collection: 0 };
      assert.equal(plugin.Validate(good), true);
      assert.equal(plugin.Validate({ ...good, collection: 3 }), true);
      assert.equal(plugin.Validate({ ...good, interface: v.interface + 1 }), false);
      assert.equal(plugin.Validate({ ...good, usage_page: 0x0001 }), false);
      assert.equal(plugin.Validate({ ...good, usage: v.usage + 2 }), false);
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products.find((p) => p.variant === "KONE_PRO");
  const { device, plugin, restore, variant } = await setup(product, expected.variants.KONE_PRO.colours_a);
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

test("ProductId() lists every claimed product id once and equals the manifest", async () => {
  const { plugin, restore } = await setup(expected.products[0], [[0, 0, 0]]);
  try {
    const ids = plugin.ProductId();
    assert.deepEqual(ids, expected.products.map((p) => Number(p.pid)));
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(manifest.devices[0].pids.map(Number), ids);
    assert.equal(Number(manifest.devices[0].vid), plugin.VendorId());
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    for (const held of expected.held_back) {
      assert.ok(!ids.includes(Number(held.pid)), held.pid);
    }
  } finally {
    restore();
  }
});

test("no profile, profile-select or settings report is ever sent", async () => {
  for (const product of expected.products) {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        if (op.op === "send_report") {
          assert.ok(["0E06", "0D0B", "0D2E", "0D7A"].includes(op.hex.slice(0, 4)), `${product.pid}: ${op.hex.slice(0, 8)}`);
        }
        if (op.op === "write") {
          assert.equal(op.hex.slice(0, 6), "001010", `${product.pid}: only the Kone Pro Air direct report`);
        }
      }
    } finally {
      restore();
    }
  }
});

test("held-back and unknown product ids drive nothing", async () => {
  for (const pid of [...expected.held_back.map((h) => Number(h.pid)), 0x1234]) {
    const device = createMockDevice({ productId: pid });
    const restore = installGlobals(device);
    try {
      const plugin = await loadPlugin(pluginPath);
      assert.equal(plugin.Validate({ interface: 0, usage_page: 0x000B, usage: 0, collection: 0 }), false);
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(false);
      assert.deepEqual(io(device), []);
      assert.equal(device.logs.length, 1);
    } finally {
      restore();
    }
  }
});
