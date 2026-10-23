// SPDX-License-Identifier: GPL-3.0-or-later
//
// HyperX mousemats (identical file in plugins/hyperx-mousemat and plugins/hyperx-mousemat-hp): for
// every product id of this directory's vendor id, Initialize, two frames and Shutdown are byte for
// byte the packets worked out from OpenRGB's C++ (fixtures/expected.json); a frame goes out on
// every Render (OpenRGB's 50 ms keepalive); Validate takes exactly the detector's collection; only
// Direct reports are ever sent; the shared module is identical in both directories.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { basename, dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const dir = basename(root);
const sibling = join(root, "..", dir.endsWith("-hp") ? dir.slice(0, -3) : `${dir}-hp`);
const pluginPath = join(root, "plugin.js");
const expected = JSON.parse(readFileSync(join(root, "fixtures", "expected.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(root, "manifest.json"), "utf8"));
const IO = new Set(["write", "send_report", "get_report", "read", "pause"]);

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read" || method === "get_report") return { op: method, len: args[1] };
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
  const device = createMockDevice({ productId: Number(product.pid), vendorId: Number(product.vid), color: canvas(variant, colours) });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore, variant };
}

const live = expected.products.filter((p) => !p.held_back);

for (const product of live) {
  test(`${product.pid} ${product.name}: frames and shutdown are the C++ packets`, async () => {
    mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
      device.calls.length = 0;
      mock.timers.tick(33);
      plugin.Render();
      if (variant.resend_ms === 0) {
        assert.deepEqual(io(device), variant.frame_a, "a mat that needs a stream gets every frame");
      } else {
        assert.deepEqual(io(device), [], "an unchanged frame within the keepalive is not resent");
        mock.timers.tick(variant.resend_ms);
        plugin.Render();
        assert.deepEqual(io(device), variant.frame_a, "resent after the keepalive");
      }
      device.calls.length = 0;
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame with colour set B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown: a black frame");
    } finally {
      restore();
      mock.timers.reset();
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
      const cells = new Set();
      for (const [x, y] of variant.leds.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
        assert.ok(!cells.has(`${x},${y}`), "one LED per cell");
        cells.add(`${x},${y}`);
      }
    } finally {
      restore();
    }
  });

  test(`${product.pid} ${product.name}: Validate takes the detector's collection only`, async () => {
    const { plugin, restore } = await setup(product, [[0, 0, 0]]);
    try {
      const v = product.validate;
      const page = v.usage_page ?? 0xFF00;
      const usage = v.usage ?? 0x01;
      assert.equal(plugin.Validate({ interface: v.interface, usage_page: page, usage, collection: 0 }), true);
      assert.equal(plugin.Validate({ interface: v.interface + 1, usage_page: page, usage, collection: 0 }), false);
      assert.equal(plugin.Validate({ interface: v.interface - 1, usage_page: page, usage, collection: 0 }), false);
      if (v.usage_page !== undefined) {
        assert.equal(plugin.Validate({ interface: v.interface, usage_page: 0x0001, usage, collection: 1 }), false);
      } else {
        assert.equal(plugin.Validate({ interface: v.interface, usage_page: 0x000C, usage: 0x01, collection: 2 }), true,
          "an interface-only registration takes any collection of the interface");
      }
      if (v.usage !== undefined) {
        assert.equal(plugin.Validate({ interface: v.interface, usage_page: page, usage: usage + 1, collection: 1 }), false);
      }
    } finally {
      restore();
    }
  });
}

test("ProductId() lists this vendor id's live products, as the manifest does", async () => {
  const { plugin, restore } = await setup(live[0], [[0, 0, 0]]);
  try {
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(plugin.ProductId().slice().sort(), live.map((p) => Number(p.pid)).sort());
    assert.deepEqual(manifest.devices[0].pids.map(Number).sort(), live.map((p) => Number(p.pid)).sort());
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.equal(new Set(plugin.ProductId()).size, plugin.ProductId().length);
    for (const p of live) {
      assert.equal(Number(p.vid), plugin.VendorId());
    }
  } finally {
    restore();
  }
});

test("only Direct reports are sent", async () => {
  const allowed = [/^0004F20{12}02(00)+$/, /^0081/];
  for (const product of live) {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device).filter((o) => o.hex)) {
        assert.ok(allowed.some((re) => re.test(op.hex)), `${product.pid}: ${op.hex.slice(0, 12)}`);
      }
    } finally {
      restore();
    }
  }
});

test("family.js and this test are identical in both HyperX mousemat directories", () => {
  for (const file of ["family.js", join("tests", "packets.test.mjs")]) {
    assert.equal(readFileSync(join(root, file), "utf8"), readFileSync(join(sibling, file), "utf8"), file);
  }
});
