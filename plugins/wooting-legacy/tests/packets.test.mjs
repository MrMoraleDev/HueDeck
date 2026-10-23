// SPDX-License-Identifier: GPL-3.0-or-later
//
// Wooting keyboards (identical file in plugins/wooting and plugins/wooting-legacy): for every
// product id of this directory's vendor id and every report format its usage pages select,
// Initialize (colour init), two frames and Shutdown (reset-all) are byte for byte the packets
// worked out from OpenRGB's C++ (fixtures/expected.json); unchanged frames are resent only after
// a second; the V3 collection wins over the V2 one; Validate takes the Wooting usage pages only;
// no command other than colour init, raw colours and reset-all is sent; the shared modules are
// identical in both directories.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { basename, dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const sibling = join(root, "..", basename(root) === "wooting" ? "wooting-legacy" : "wooting");
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

/** A mock keyboard whose host lists the given collections. */
async function setup(product, colours, pages) {
  const device = createMockDevice({ productId: Number(product.pid), vendorId: Number(product.vid), color: colours });
  const endpoints = pages.map((page, i) => ({ interface: 1 + i, usage: 0x01, usage_page: page, collection: 0 }));
  device.getHidEndpoints = () => endpoints;
  device.set_endpoint = (...args) => device.calls.push({ method: "set_endpoint", args });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const product of expected.products) {
  for (const [page, proto] of Object.entries(product.protocols)) {
    const variant = expected.variants[`${product.type}_${proto}`];
    test(`${product.pid} ${product.name}, ${proto} (usage page ${page}): the C++ packets`, async () => {
      const { device, plugin, restore } = await setup(product, canvas(variant, variant.colours_a), [0x0001, Number(page)]);
      mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
      try {
        plugin.Initialize();
        const ep = device.calls.find((c) => c.method === "set_endpoint")?.args;
        assert.equal(ep[2], Number(page), "the Wooting collection is made active");
        assert.deepEqual(io(device), variant.initialize, "Initialize: colour init");
        device.calls.length = 0;
        plugin.Render();
        assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
        device.calls.length = 0;
        mock.timers.tick(500);
        plugin.Render();
        assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
        mock.timers.tick(500);
        plugin.Render();
        assert.deepEqual(io(device), variant.frame_a, "resent after a second");
        device.calls.length = 0;
        device.setColor(canvas(variant, variant.colours_b));
        plugin.Render();
        assert.deepEqual(io(device), variant.frame_b, "frame with colour set B");
        device.calls.length = 0;
        plugin.Shutdown(false);
        assert.deepEqual(io(device), variant.shutdown, "Shutdown: reset-all");
      } finally {
        restore();
        mock.timers.reset();
      }
    });
  }

  test(`${product.pid} ${product.name}: name and LED grid`, async () => {
    const proto = Object.values(product.protocols)[0];
    const variant = expected.variants[`${product.type}_${proto}`];
    const { device, plugin, restore } = await setup(product, [0, 0, 0], Object.keys(product.protocols).map(Number));
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [product.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
      const cells = new Set();
      for (const [x, y] of variant.leds.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
        assert.ok(!cells.has(`${x},${y}`));
        cells.add(`${x},${y}`);
      }
    } finally {
      restore();
    }
  });

  test(`${product.pid} ${product.name}: Validate takes the Wooting usage pages, any interface`, async () => {
    const { plugin, restore } = await setup(product, [0, 0, 0], []);
    try {
      for (const page of product.validate.usage_page_any_of) {
        for (const iface of [0, 1, 2, 3]) {
          assert.equal(plugin.Validate({ interface: iface, usage_page: page, usage: 0x01, collection: 0 }), true);
        }
      }
      for (const page of [0x0001, 0x000C, 0xFF00, 0x1338, 0xFF54].concat(product.vid === "0x03EB" ? [0xFF55] : [])) {
        assert.equal(plugin.Validate({ interface: 0, usage_page: page, usage: 0x01, collection: 0 }), false, `page ${page}`);
      }
    } finally {
      restore();
    }
  });
}

test("a keyboard showing both a V3 and a V2 collection is driven over V3", async () => {
  const product = expected.products.find((p) => p.protocols["0xFF55"]);
  if (!product) return;
  const variant = expected.variants[`${product.type}_V3`];
  const { device, plugin, restore } = await setup(product, canvas(variant, variant.colours_a), [0x1337, 0xFF55]);
  try {
    plugin.Initialize();
    assert.equal(device.calls.find((c) => c.method === "set_endpoint").args[2], 0xFF55);
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_a);
  } finally {
    restore();
  }
});

test("without a Wooting collection nothing is sent", async () => {
  const { device, plugin, restore } = await setup(expected.products[0], [0, 0, 0], [0x0001]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});

test("only colour init, raw colours and reset-all are sent", async () => {
  for (const product of expected.products) {
    for (const page of Object.keys(product.protocols)) {
      const { device, plugin, restore } = await setup(product, [1, 2, 3], [Number(page)]);
      try {
        plugin.Initialize();
        plugin.Render();
        plugin.Shutdown(true);
        for (const op of io(device).filter((o) => o.hex)) {
          const ok = op.hex === "00D0DA2100000000" || op.hex === "00D0DA2000000000"
            || op.hex.startsWith("00D0DA0B") || op.hex.startsWith("04D1DA0B");
          assert.ok(ok, `${product.pid}: ${op.hex.slice(0, 16)}`);
        }
      } finally {
        restore();
      }
    }
  }
});

test("ProductId() lists this vendor id's products, as the manifest does", async () => {
  const { plugin, restore } = await setup(expected.products[0], [0, 0, 0], []);
  try {
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(plugin.ProductId().slice().sort(), expected.products.map((p) => Number(p.pid)).sort());
    assert.deepEqual(manifest.devices[0].pids.map(Number).sort(), expected.products.map((p) => Number(p.pid)).sort());
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.equal(new Set(plugin.ProductId()).size, plugin.ProductId().length);
  } finally {
    restore();
  }
});

test("family.js, layouts.js and this test are identical in both Wooting directories", () => {
  for (const file of ["family.js", "layouts.js", join("tests", "packets.test.mjs")]) {
    assert.equal(readFileSync(join(root, file), "utf8"), readFileSync(join(sibling, file), "utf8"), file);
  }
});
