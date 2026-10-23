// SPDX-License-Identifier: GPL-3.0-or-later
//
// ASUS Aura mouse plugin: for each direct-mode mouse in fixtures/expected.json the start, two
// frames and the shutdown are byte for byte the reports worked out from OpenRGB's C++; unchanged
// frames are only resent as a keepalive; Validate takes interface 0 / usage page 0xFF01 only; the
// save command, hardware modes and anything but 00 51 29 are never sent; the mice without a direct
// mode are not claimed.

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

const canvas = (v, colours) => (x, y) => (y === 0 && x < colours.length ? colours[x] : [0xEE, 0xEE, 0xEE]);

async function start(v) {
  const device = createMockDevice({ productId: Number(v.pid), color: canvas(v, v.colours_a) });
  const restore = installGlobals(device);
  return { device, plugin: await loadPlugin(pluginPath), restore };
}

for (const v of expected.variants) {
  test(`${v.pid} ${v.name}: start, frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await start(v);
    try {
      plugin.Initialize();
      assert.deepEqual(writes(device), v.initialize);
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [v.name]);
      assert.deepEqual(call("setSize"), [v.size]);
      assert.deepEqual(call("setControllableLeds"), [v.zones, v.cells]);
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(writes(device), v.frame_a);
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(writes(device), [], "unchanged frame not resent within a second");
      device.setColor(canvas(v, v.colours_b));
      plugin.Render();
      assert.deepEqual(writes(device), v.frame_b);
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(writes(device), v.shutdown);
      for (const h of [...v.frame_a, ...v.frame_b, ...v.shutdown]) assert.ok(h.startsWith("005129"));
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const v = expected.variants.find((x) => x.pid === "0x1A18");
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

test("metadata: ids equal the manifest, Validate takes interface 0 / 0xFF01, older mice not claimed", async () => {
  const device = createMockDevice({ productId: 0x1A18 });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    const ids = plugin.ProductId();
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, manifest.devices[0].pids.map(Number));
    assert.deepEqual(ids, expected.variants.map((v) => Number(v.pid)));
    assert.deepEqual(manifest.devices[0].tested, []);
    for (const old of [0x1845, 0x18E5, 0x195C, 0x1910, 0x1898, 0x1847]) assert.ok(!ids.includes(old));
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF01, usage: 0x01, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x02, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF01, usage: 0x01, collection: 0 }), false);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
});
