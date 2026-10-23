// SPDX-License-Identifier: GPL-3.0-or-later
//
// Cooler Master MM712 plugin: start, two frames and shutdown are byte for byte the packets worked
// out from OpenRGB's C++ (fixtures/expected.json); unchanged frames are only resent as a
// keepalive; Validate takes interface 3 / 0xFF0A / 2 only; only 44 81 02 and 5A 81 xx are ever
// written (never the save 54 81 or a mode / profile write 4C 81 / 44 81 01).

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

async function setup(pid, colour) {
  const device = createMockDevice({ productId: pid, color: colour });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

test("0x0169 MM712: start, frames and shutdown are the C++ packets", async () => {
  const { device, plugin, restore } = await setup(0x0169, expected.colours_a[0]);
  try {
    plugin.Initialize();
    assert.deepEqual(io(device), expected.initialize, "Initialize");
    assert.deepEqual(device.calls.find((c) => c.method === "setName")?.args, [expected.products[0].name]);
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), expected.frame_a, "frame A");
    device.calls.length = 0;
    plugin.Render();
    assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
    device.setColor(expected.colours_b[0]);
    plugin.Render();
    assert.deepEqual(io(device), expected.frame_b, "frame B");
    device.calls.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(io(device), expected.shutdown, "Shutdown: direct mode off");
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup(0x0169, expected.colours_a[0]);
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

test("metadata: product id equals the manifest, Validate takes interface 3 / 0xFF0A / 2 only", async () => {
  const { plugin, restore } = await setup(0x0169, [0, 0, 0]);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.deepEqual(plugin.ProductId(), manifest.devices[0].pids.map(Number));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.Size(), expected.size);
    assert.deepEqual(plugin.LedNames(), expected.leds.map((l) => l.name));
    assert.deepEqual(plugin.LedPositions(), expected.leds.map((l) => l.pos));
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0xFF0A, usage: 0x0002, collection: 2 }), true);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0xFF0A, usage: 0x0001, collection: 1 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0002, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("only 44 81 02 and 5A 81 01 / 02 / 03 are ever written", async () => {
  const { device, plugin, restore } = await setup(0x0169, [1, 2, 3]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    for (const op of io(device).filter((o) => o.op === "write")) {
      assert.ok(["00448102", "005A8101", "005A8102", "005A8103"].includes(op.hex.slice(0, 8)), op.hex.slice(0, 8));
    }
  } finally {
    restore();
  }
});

test("another product id drives nothing", async () => {
  const { device, plugin, restore } = await setup(0x0101, [1, 2, 3]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});
