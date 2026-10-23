// SPDX-License-Identifier: GPL-3.0-or-later
//
// Cooler Master ARGB monitor / GD160 desk plugin: for every product id, two frames (the first one
// turning software mode on) and the shutdown (software mode off) are byte for byte the packets
// worked out from OpenRGB's C++ (fixtures/expected.json); Initialize sends nothing; unchanged
// frames are only resent as a keepalive; Validate takes interface 0 / 0xFF00 / 1 only; only the
// software-mode switch (80 07) and the direct colour packets are ever written.

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
  test(`${product.pid} ${product.name}: frames and shutdown are the C++ packets`, async () => {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize sends nothing");
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [product.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
      for (const [x, y] of variant.leds.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
      }
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame A (software mode on first)");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown: software mode off");
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products.find((p) => p.variant === "MONITOR");
  const variant = expected.variants.MONITOR;
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
    assert.deepEqual(io(device), variant.frame_a.slice(1), "keepalive: colours only, software mode stays on");
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("metadata: product ids equal the manifest, Validate takes interface 0 / 0xFF00 / 1 only", async () => {
  const { plugin, restore } = await setup(expected.products[0], [[0, 0, 0]]);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.deepEqual(plugin.ProductId(), manifest.devices[0].pids.map(Number));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.equal(plugin.VendorId(), 0x2516);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x0002, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("only the software-mode switch and direct colour packets are written (no 80 0B/0F/11/18, no 10 custom colours)", async () => {
  for (const product of expected.products) {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device).filter((o) => o.op === "write")) {
        const head = op.hex.slice(0, 6);
        const ok = head === "008007" || head === "000007" || ["0001", "0002", "0003", "0004", "0005", "0086"].includes(op.hex.slice(0, 4));
        assert.ok(ok, `${product.pid}: ${head}`);
      }
    } finally {
      restore();
    }
  }
});
