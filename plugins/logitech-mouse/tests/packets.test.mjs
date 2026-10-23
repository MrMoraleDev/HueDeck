// SPDX-License-Identifier: GPL-3.0-or-later
//
// Logitech fixed-feature mice plugin: for every product id, the start, two frames and the
// shutdown are byte for byte the reports worked out from OpenRGB's C++ (fixtures/expected.json);
// unchanged frames are only resent as a keepalive; Validate takes the collections OpenRGB opens;
// nothing but the lighting reports is ever written.

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
const IO = new Set(["write", "send_report", "read", "pause", "set_endpoint"]);

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    if (method === "set_endpoint") return { op: "set_endpoint", interface: args[0], usage: args[1], usage_page: args[2] };
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

function canvas(variant, colours) {
  const byCell = new Map(variant.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(pid, colours, variant) {
  const device = createMockDevice({ productId: Number(pid), color: variant ? canvas(variant, colours) : [0, 0, 0] });
  device.set_endpoint = (...args) => { device.calls.push({ method: "set_endpoint", args }); return true; };
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const product of expected.products) {
  const variant = expected.variants[product.variant];

  test(`${product.pid} ${product.name}: start, frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await setup(product.pid, variant.colours_a, variant);
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
    const { device, plugin, restore } = await setup(product.pid, variant.colours_a, variant);
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

  test(`${product.pid}: Validate takes interface 1, usage page ${product.validate.usage_page}, usage ${product.validate.usages}`, async () => {
    const { plugin, restore } = await setup(product.pid, [], null);
    try {
      const page = Number(product.validate.usage_page);
      const usages = product.validate.usages === "any" ? [0x0001, 0x0002, 0x0080] : product.validate.usages.map(Number);
      for (const usage of usages) {
        assert.equal(plugin.Validate({ interface: 1, usage_page: page, usage, collection: 1 }), true, `usage ${usage}`);
        assert.equal(plugin.Validate({ interface: 0, usage_page: page, usage, collection: 0 }), false, "interface 0");
        assert.equal(plugin.Validate({ interface: 2, usage_page: page, usage, collection: 0 }), false, "interface 2");
      }
      assert.equal(plugin.Validate({ interface: 1, usage_page: 0x0001, usage: 0x0002, collection: 0 }), false, "the mouse collection");
      assert.equal(plugin.Validate({ interface: 1, usage_page: page ^ 0x0080, usage: 0x0002, collection: 0 }), false, "other vendor page");
      if (product.validate.usages !== "any") {
        assert.equal(plugin.Validate({ interface: 1, usage_page: page, usage: 0x0004, collection: 0 }), false, "usage 4");
        if (!product.validate.usages.includes("0x0001")) {
          assert.equal(plugin.Validate({ interface: 1, usage_page: page, usage: 0x0001, collection: 0 }), false, "usage 1");
        }
      }
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products.find((p) => p.variant === "LIGHTSYNC_2ZONE_3A");
  const variant = expected.variants[product.variant];
  const { device, plugin, restore } = await setup(product.pid, variant.colours_a, variant);
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

test("ProductId() lists every claimed id once, equal to the manifest", async () => {
  const { plugin, restore } = await setup(expected.products[0].pid, [], null);
  try {
    const ids = plugin.ProductId();
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, expected.products.map((p) => Number(p.pid)));
    assert.equal(manifest.devices.length, 1);
    assert.equal(Number(manifest.devices[0].vid), plugin.VendorId());
    assert.deepEqual(manifest.devices[0].pids.map(Number), ids);
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
});

test("a product id the plugin does not know drives nothing", async () => {
  const { device, plugin, restore } = await setup("0xC08B", [], null);
  try {
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x0002, collection: 0 }), false);
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});

test("only lighting reports are written: no hardware effect, no leaving software control", async () => {
  const allowed = {
    LIGHTSYNC_1ZONE_3A: [/^10FF0E8A0101/, /^11FF0E3A0001[0-9A-F]{6}02/],
    LIGHTSYNC_1ZONE_3C: [/^10FF0E8A0101/, /^11FF0E3C0001[0-9A-F]{6}02/],
    LIGHTSYNC_2ZONE_3A: [/^10FF0E8A0101/, /^11FF0E3A0[01]01[0-9A-F]{6}02/],
    G203L: [/^11FF0E50010307/, /^11FF121001[0-9A-F]{6}02[0-9A-F]{6}03[0-9A-F]{6}FF/, /^11FF1270/],
    G600: [/^F1[0-9A-F]{6}00000000$/],
  };
  for (const product of expected.products) {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product.pid, variant.colours_a, variant);
    try {
      plugin.Initialize();
      plugin.Render();
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device).filter((o) => o.hex)) {
        assert.ok(allowed[product.variant].some((re) => re.test(op.hex)), `${product.pid}: ${op.hex}`);
      }
    } finally {
      restore();
    }
  }
});
