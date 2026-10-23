// SPDX-License-Identifier: GPL-3.0-or-later
//
// ASUS Aura mousemat plugin: for both Balteus models the start, two frames and the shutdown are
// byte for byte the reports worked out from OpenRGB's C++; unchanged frames are only resent as a
// keepalive; Validate takes usage page 0xFF06 / usage 1 only; only EE C0 81 is ever sent.

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
const PREFIX = "EEC081";

function writes(device) {
  const out = [];
  for (const { method, args } of device.calls) {
    assert.ok(!["send_report", "get_report", "read"].includes(method), `unexpected ${method}`);
    if (method === "write") {
      assert.equal(args[1], 65);
      out.push(bytesToHex(Array.from({ length: 65 }, (_, i) => (i < args[0].length ? args[0][i] & 0xFF : 0))).replace(/ /g, ""));
    }
  }
  return out;
}

function canvas(colours) {
  const byCell = new Map(expected.cells.map((c, i) => [c.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function start(v) {
  const device = createMockDevice({ productId: Number(v.pid), color: canvas(v.colours_a) });
  const restore = installGlobals(device);
  return { device, plugin: await loadPlugin(pluginPath), restore };
}

for (const v of expected.variants) {
  test(`${v.pid} ${v.name}: start, frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await start(v);
    try {
      plugin.Initialize();
      assert.deepEqual(writes(device), v.initialize);
      assert.deepEqual(device.calls.find((c) => c.method === "setName")?.args, [v.name]);
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(writes(device), v.frame_a);
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(writes(device), [], "unchanged frame not resent within a second");
      device.setColor(canvas(v.colours_b));
      plugin.Render();
      assert.deepEqual(writes(device), v.frame_b);
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(writes(device), v.shutdown);
      for (const h of [...v.frame_a, ...v.frame_b, ...v.shutdown]) assert.ok(h.startsWith(PREFIX));
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const v = expected.variants[0];
  const { device, plugin, restore } = await start(v);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(writes(device), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(writes(device), v.frame_a);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("metadata: ids equal the manifest, geometry, Validate follows the detector", async () => {
  const device = createMockDevice({ productId: Number(expected.variants[0].pid) });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.deepEqual(plugin.ProductId(), manifest.devices[0].pids.map(Number));
    assert.deepEqual(plugin.ProductId(), expected.variants.map((v) => Number(v.pid)));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.LedNames(), expected.led_names);
    assert.deepEqual(plugin.LedPositions(), expected.cells);
    const [w, h] = plugin.Size();
    for (const [x, y] of expected.cells) assert.ok(x < w && y < h);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF06, usage: 0x01, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF06, usage: 0x01, collection: 1 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF06, usage: 0x02, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x01, collection: 0 }), false);
  } finally {
    restore();
  }
});
