// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries QCK Prism plugin: for every product id OpenRGB's detector lists, two frames and the
// shutdown are byte for byte the packets worked out from OpenRGB's C++ (fixtures/expected.json);
// unchanged colours are never resent and changes go out at most once a second; Validate takes
// interface 0 only; only the 0x0E colour report and the 0x0D report that follows it are ever sent.

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
  test(`${product.pid} ${product.name}: frames and shutdown are the C++ packets`, async () => {
    mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize sends nothing of its own");
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "unchanged colours are not resent");
      mock.timers.tick(1000);
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame with colour set B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown: black");
    } finally {
      restore();
      mock.timers.reset();
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
    } finally {
      restore();
    }
  });
}

test("no keepalive: unchanged colours stay unsent however long", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products[0];
  const { device, plugin, restore } = await setup(product, expected.variants[product.variant].colours_a);
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

test("changes go out at most once a second, the latest colours winning", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products[0];
  const variant = expected.variants[product.variant];
  const { device, plugin, restore } = await setup(product, variant.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    // An animated effect: a new colour every frame for 999 ms sends nothing.
    for (let i = 0; i < 30; i++) {
      mock.timers.tick(33);
      device.setColor(() => [i, 2 * i, 3 * i]);
      plugin.Render();
    }
    assert.deepEqual(io(device), [], "no report within the second after the last one");
    mock.timers.tick(10);
    device.setColor(canvas(variant, variant.colours_b));
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_b, "after a second: the colours of that moment");
    device.calls.length = 0;
    mock.timers.tick(500);
    device.setColor(canvas(variant, variant.colours_a));
    plugin.Render();
    assert.deepEqual(io(device), [], "half a second later: waits");
    mock.timers.tick(500);
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_a, "the waiting change goes out once the second is over");
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("metadata lists every product id once, Validate takes interface 0 only", async () => {
  const { plugin, restore } = await setup(expected.products[0], [[0, 0, 0], [0, 0, 0]]);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.equal(new Set(plugin.ProductId()).size, plugin.ProductId().length);
    assert.equal(plugin.VendorId(), 0x1038);
    assert.equal(plugin.DeviceType(), "mousepad");
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    for (const p of expected.products) {
      assert.deepEqual(p.validate, { interface: 0 });
    }
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0002, collection: 1 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("only the 0x0E colour report and the 0x0D report are sent", async () => {
  for (const product of expected.products) {
    const { device, plugin, restore } = await setup(product, [[1, 2, 3], [4, 5, 6]]);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        if (op.op === "send_report") {
          assert.equal(op.len, 525);
          assert.equal(op.hex.slice(0, 4), "000E");
        } else {
          assert.equal(op.op, "write");
          assert.equal(op.hex, "000D" + "00".repeat(63));
        }
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
