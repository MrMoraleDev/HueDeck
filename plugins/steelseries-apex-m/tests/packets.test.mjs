// SPDX-License-Identifier: GPL-3.0-or-later
//
// SteelSeries Apex M750 plugin: the start (enable LED control), two frames and the shutdown are
// byte for byte the packets worked out from OpenRGB's C++ (fixtures/expected.json, which puts
// each key's colour into its keys_m slot, the numeric-pad entries corrected to the keys their
// labels name); the LED grid is OpenRGB's 22 x 6 ANSI matrix; numeric-pad slots are fed from
// their own key's cell;
// unchanged frames are only resent as a keepalive; Validate takes interface 2 only.

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
const product = expected.products[0];
const variant = expected.variants[product.variant];

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

function canvas(colours) {
  const byCell = new Map(variant.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(colours) {
  const device = createMockDevice({ productId: Number(product.pid), color: canvas(colours) });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

test(`${product.pid} ${product.name}: start, frames and shutdown are the C++ packets`, async () => {
  const { device, plugin, restore } = await setup(variant.colours_a);
  try {
    plugin.Initialize();
    assert.deepEqual(io(device), variant.initialize, "Initialize: EnableLEDControl");
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
    device.setColor(canvas(variant.colours_b));
    plugin.Render();
    assert.deepEqual(io(device), variant.frame_b, "frame with colour set B");
    device.calls.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(io(device), variant.shutdown, "Shutdown: black");
  } finally {
    restore();
  }
});

test("LED names and grid: OpenRGB's names at their ANSI matrix cells, wire-slot order", async () => {
  const { device, plugin, restore } = await setup(variant.colours_a);
  try {
    plugin.Initialize();
    const call = (m) => device.calls.find((c) => c.method === m)?.args;
    assert.deepEqual(call("setName"), [product.name]);
    assert.deepEqual(call("setSize"), [variant.size]);
    assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
    assert.deepEqual(plugin.Size(), variant.size);
    assert.deepEqual(plugin.LedNames(), variant.leds.map((l) => l.name));
    assert.deepEqual(plugin.LedPositions(), variant.leds.map((l) => l.pos));
    const cells = new Set(variant.leds.map((l) => l.pos.join(",")));
    assert.equal(cells.size, variant.leds.length, "every LED has its own cell");
    for (const [x, y] of variant.leds.map((l) => l.pos)) {
      assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
    }
  } finally {
    restore();
  }
});

test("numeric pad: each slot is fed from the cell of the key its keys_m label names", async () => {
  // keys_m slot -> label (SteelSeriesApexMController.cpp comments) -> expected key and ANSI cell.
  const numpad = [
    [19, "Number Pad 0", [18, 5]], [21, "Number Pad .", [20, 5]],
    [40, "Number Pad 1", [18, 4]], [41, "Number Pad 2", [19, 4]], [42, "Number Pad 3", [20, 4]],
    [43, "Number Pad Enter", [21, 4]],
    [62, "Number Pad 4", [18, 3]], [63, "Number Pad 5", [19, 3]], [64, "Number Pad 6", [20, 3]],
    [84, "Number Pad 7", [18, 2]], [85, "Number Pad 8", [19, 2]], [86, "Number Pad 9", [20, 2]],
    [87, "Number Pad +", [21, 2]],
    [106, "Num Lock", [18, 1]], [107, "Number Pad /", [19, 1]], [108, "Number Pad *", [20, 1]],
    [109, "Number Pad -", [21, 1]],
  ];
  const marker = [0x12, 0x34, 0x56];
  for (const [slot, name, cell] of numpad) {
    const device = createMockDevice({
      productId: Number(product.pid),
      color: (x, y) => (x === cell[0] && y === cell[1] ? marker : [0, 0, 0]),
    });
    const restore = installGlobals(device);
    try {
      const plugin = await loadPlugin(pluginPath);
      const i = plugin.LedNames().indexOf(name);
      assert.ok(i >= 0, name);
      assert.deepEqual(plugin.LedPositions()[i], cell, `${name} cell`);
      plugin.Initialize();
      device.calls.length = 0;
      plugin.Render();
      const [{ args: [buf] }] = device.calls.filter((c) => c.method === "send_report");
      const lit = [];
      for (let s = 0; s < 132; s++) {
        if (buf[9 + s * 3] === marker[0] && buf[10 + s * 3] === marker[1] && buf[11 + s * 3] === marker[2]) {
          lit.push(s);
        }
      }
      assert.deepEqual(lit, [slot], `${name} at ${cell} lights slot ${slot} only`);
    } finally {
      restore();
    }
  }
});

test("no Japanese key and no extra row: every LED is on the 22 x 6 ANSI matrix", async () => {
  const { plugin, restore } = await setup(variant.colours_a);
  try {
    assert.deepEqual(plugin.Size(), [22, 6]);
    assert.equal(plugin.LedNames().length, 104);
    for (const name of ["_", "かな", "¥", "変換", "無変換"]) {
      assert.ok(!plugin.LedNames().includes(name), name);
    }
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup(variant.colours_a);
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

test("metadata, Validate (interface 2 only) and only 513-byte feature reports", async () => {
  const { device, plugin, restore } = await setup(variant.colours_a);
  try {
    assert.deepEqual(plugin.ProductId(), [0x0616]);
    assert.equal(plugin.VendorId(), 0x1038);
    assert.deepEqual(product.validate, { interface: 2 });
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0x000C, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFFC0, usage: 0x0001, collection: 1 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0006, collection: 0 }), false);
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    for (const op of io(device)) {
      assert.equal(op.op, "send_report");
      assert.equal(op.len, 513);
    }
  } finally {
    restore();
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
