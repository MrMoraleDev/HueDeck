// SPDX-License-Identifier: GPL-3.0-or-later
//
// ASUS Aura keyboard plugin: for each variant in fixtures/expected.json the start (layout probe,
// Claymore remote-control commands), two frames and the shutdown are byte for byte the I/O worked
// out from OpenRGB's C++, including the clear / write / read sequence of the TUF protocol; the
// layout reply picks the LED list as OpenRGB does (8-bit truncation and UK / US fallback
// included); unchanged frames are only resent as a keepalive; Validate takes the vendor collection
// of the detector's interface; the save command and hardware effects are never sent.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));

/** The I/O a plugin made, in the fixture's shape. */
function io(device) {
  const out = [];
  for (const { method, args } of device.calls) {
    assert.ok(!["send_report", "get_report"].includes(method), `unexpected ${method}`);
    if (method === "clearReadBuffer") out.push(["clear"]);
    if (method === "read") out.push(["read", args[1], args[2]]);
    if (method === "write") {
      assert.equal(args[1], 65);
      const bytes = Array.from({ length: 65 }, (_, i) => (i < args[0].length ? args[0][i] & 0xFF : 0));
      out.push(["write", bytesToHex(bytes).replace(/ /g, "")]);
    }
  }
  return out;
}

function canvas(variant, colours) {
  const byCell = new Map(variant.leds.map((pos, i) => [pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

/** A keyboard that answers the first read after a probe write with the variant's reply. */
function keyboard(variant) {
  const device = createMockDevice({ productId: Number(variant.pid), color: canvas(variant, variant.colours_a) });
  let pending = null;
  const write = device.write;
  device.write = (data, length) => {
    const n = write(data, length);
    const cmd = ((data[1] & 0xFF) << 8) | (data[2] & 0xFF);
    if (variant.reply_data && (cmd === 0x1212 || cmd === 0x4060)) pending = [0x00, ...hexToBytes(variant.reply_data)];
    return n;
  };
  let size = 0;
  device.read = (id, length, timeout) => {
    device.calls.push({ method: "read", args: [id, length, timeout] });
    const r = pending;
    pending = null;
    size = r ? r.length : 0;
    return r ? r.slice() : [];
  };
  device.getLastReadSize = () => size;
  return device;
}

async function start(variant) {
  const device = keyboard(variant);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const variant of expected.variants) {
  test(`${variant.pid} ${variant.name} (${variant.note || variant.proto}): start, frames and shutdown are the C++ I/O`, async () => {
    const { device, plugin, restore } = await start(variant);
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

  test(`${variant.pid} ${variant.name}: LED grid`, async () => {
    const { device, plugin, restore } = await start(variant);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [variant.name]);
      if (variant.size === null) {
        assert.deepEqual(call("setControllableLeds"), [[], []]);
        return;
      }
      assert.deepEqual(call("setSize"), [variant.size]);
      const [names, positions] = call("setControllableLeds");
      assert.deepEqual(positions, variant.leds);
      assert.equal(names.length, positions.length);
      const cells = new Set(positions.map((p) => p.join(",")));
      assert.equal(cells.size, positions.length, "one LED per cell");
      for (const [x, y] of positions) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
      }
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const variant = expected.variants.find((v) => v.pid === "0x18AA");
  const { device, plugin, restore } = await start(variant);
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

test("only lighting reports and read-only probes are ever written", async () => {
  for (const variant of expected.variants) {
    const { device, plugin, restore } = await start(variant);
    try {
      plugin.Initialize();
      plugin.Render();
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device).filter((o) => o[0] === "write")) {
        const b = hexToBytes(op[1]);
        assert.equal(b[0], 0x00);
        const cmd = (b[1] << 8) | b[2];
        assert.ok([0xC081, 0x1212, 0x4060, 0x4101, 0x4103].includes(cmd), `${variant.pid}: ${op[1].slice(0, 6)}`);
        assert.notEqual(b[1], 0x50, "never the save command");
        assert.notEqual(b[1], 0x51, "never a hardware effect");
      }
    } finally {
      restore();
    }
  }
});

test("metadata: product ids equal the manifest, Validate follows the detectors", async () => {
  const device = createMockDevice({ productId: 0x18AA });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    const ids = plugin.ProductId();
    assert.equal(ids.length, 29);
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, manifest.devices[0].pids.map(Number));
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x01, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF00, usage: 0x01, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x0001, usage: 0x06, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x01, collection: 0 }), false);
  } finally {
    restore();
  }
  for (const pid of [0x1945, 0x1899]) {
    const d = createMockDevice({ productId: pid });
    const r = installGlobals(d);
    try {
      const plugin = await loadPlugin(pluginPath);
      assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF00, usage: 0x01, collection: 0 }), true, "K1 / K5: interface 2");
      assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x01, collection: 0 }), false);
    } finally {
      r();
    }
  }
});
