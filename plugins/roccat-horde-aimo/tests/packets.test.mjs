// SPDX-License-Identifier: GPL-3.0-or-later
//
// Roccat Horde Aimo plugin: Initialize, two frames and Shutdown are byte for byte the I/O worked
// out from OpenRGB's C++ (fixtures/expected.json); unchanged frames are only resent as a
// keepalive; Validate takes interface 1, usage page 0x0B, usage 0 only; ProductId() equals the
// manifest.

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

function canvas(colours) {
  const byCell = new Map(expected.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(colours, productId = 0x303E) {
  const device = createMockDevice({ productId, color: canvas(colours) });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

test("start, frames and shutdown are the C++ I/O", async () => {
  const { device, plugin, restore } = await setup(expected.colours_a);
  try {
    plugin.Initialize();
    assert.deepEqual(io(device), expected.initialize);
    const call = (m) => device.calls.find((c) => c.method === m)?.args;
    assert.deepEqual(call("setSize"), [expected.size]);
    assert.deepEqual(call("setControllableLeds"), [expected.leds.map((l) => l.name), expected.leds.map((l) => l.pos)]);
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), expected.frame_a);
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), []);
    device.setColor(canvas(expected.colours_b));
    plugin.Render();
    assert.deepEqual(io(device), expected.frame_b);
    device.calls.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(io(device), expected.shutdown);
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup(expected.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(io(device), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(io(device), expected.frame_a);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("metadata, ProductId() equals the manifest, Validate takes the detector's collection only", async () => {
  const { plugin, restore } = await setup(expected.colours_a);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.deepEqual(manifest.devices[0].pids.map(Number), plugin.ProductId());
    assert.equal(Number(manifest.devices[0].vid), plugin.VendorId());
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000B, usage: 0, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000B, usage: 0, collection: 2 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x000B, usage: 0, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x0001, usage: 6, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000B, usage: 1, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("an unknown product id drives nothing", async () => {
  const { device, plugin, restore } = await setup(expected.colours_a, 0x1234);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
    assert.equal(device.logs.length, 1);
  } finally {
    restore();
  }
});
