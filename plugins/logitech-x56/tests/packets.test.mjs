// SPDX-License-Identifier: GPL-3.0-or-later
//
// Logitech X56 plugin: the colour reports are byte for byte OpenRGB's (fixtures/expected.json),
// unchanged colours are only resent as a keepalive, Shutdown is black, the save command (01 01) is
// never sent, Validate takes interface 2, usage page 0xFF00, usage 1 or 3.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));

function io(device) {
  return device.calls.filter((c) => ["write", "send_report", "read"].includes(c.method)).map(({ method, args }) => {
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

async function setup(pid, colour) {
  const device = createMockDevice({ productId: Number(pid), color: colour });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const product of expected.products) {
  test(`${product.pid} ${product.name}: frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await setup(product.pid, expected.colour_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), [], "Initialize sends nothing");
      assert.deepEqual(device.calls.find((c) => c.method === "setName")?.args, [product.name]);
      plugin.Render();
      assert.deepEqual(io(device), expected.frame_a);
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "unchanged frame not resent within a second");
      device.setColor(expected.colour_b);
      plugin.Render();
      assert.deepEqual(io(device), expected.frame_b);
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), expected.shutdown);
      for (const op of [...expected.frame_a, ...expected.frame_b, ...expected.shutdown]) {
        assert.ok(!op.hex.startsWith("0101"), "save command");
      }
    } finally {
      restore();
    }
  });

  test(`${product.pid}: Validate takes interface 2, usage page 0xFF00, usage 1 or 3`, async () => {
    const { plugin, restore } = await setup(product.pid, [0, 0, 0]);
    try {
      assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF00, usage: 1, collection: 1 }), true);
      assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF00, usage: 3, collection: 3 }), true);
      assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF00, usage: 2, collection: 2 }), false);
      assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 1, collection: 0 }), false);
      assert.equal(plugin.Validate({ interface: 2, usage_page: 0x0001, usage: 4, collection: 0 }), false);
    } finally {
      restore();
    }
  });
}

test("ProductId() lists every claimed id once, equal to the manifest", async () => {
  const { plugin, restore } = await setup("0x2221", [0, 0, 0]);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.equal(Number(manifest.devices[0].vid), plugin.VendorId());
    assert.deepEqual(manifest.devices[0].pids.map(Number), plugin.ProductId());
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
  } finally {
    restore();
  }
});
