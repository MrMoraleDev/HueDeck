// SPDX-License-Identifier: GPL-3.0-or-later
//
// ASUS Aura USB (mainboard / addressable) plugin: for each variant in fixtures/expected.json the
// start, two frames and the shutdown are byte for byte the reports worked out from OpenRGB's C++;
// the configuration table decides the onboard LEDs and the addressable channels; unchanged frames
// are only resent as a keepalive; a device that does not answer the configuration request gets
// nothing more; Validate picks vendor-defined collections (0x19AF: exactly 0xFF72 / 0x00A1); the
// commit (EC 3F) and the shutdown-effect variants are never sent.

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

/** Every write's bytes as hex, in order; asserts nothing but output reports of 65 bytes. */
function writes(device) {
  const out = [];
  for (const { method, args } of device.calls) {
    assert.ok(!["send_report", "get_report"].includes(method), `unexpected ${method}`);
    if (method === "write") {
      assert.equal(args[1], 65);
      const bytes = Array.from({ length: 65 }, (_, i) => (i < args[0].length ? args[0][i] & 0xFF : 0));
      out.push(bytesToHex(bytes).replace(/ /g, ""));
    }
  }
  return out;
}

/** A mock that answers EC 82 and EC B0 with the variant's replies (as input reports). */
function auraDevice(variant, colours, { silent = false } = {}) {
  const fixed = colours.fixed;
  const device = createMockDevice({
    productId: Number(variant.pid),
    color: (x, y) => (y === 0 && x < fixed.length ? fixed[x] : [0xEE, 0xEE, 0xEE]),
    channelColors: colours.channels,
  });
  const queue = [];
  const write = device.write;
  device.write = (data, length) => {
    const n = write(data, length);
    if (!silent && (data[0] & 0xFF) === 0xEC && (data[1] & 0xFF) === 0x82) queue.push(hexToBytes(variant.firmware_reply));
    if (!silent && (data[0] & 0xFF) === 0xEC && (data[1] & 0xFF) === 0xB0) queue.push(hexToBytes(variant.config_reply));
    return n;
  };
  let lastSize = 0;
  device.read = (id, length, timeout) => {
    device.calls.push({ method: "read", args: [id, length, timeout] });
    const r = queue.shift();
    lastSize = r ? r.length : 0;
    return r ? r.slice() : [];
  };
  device.getLastReadSize = () => lastSize;
  return device;
}

function setColours(device, variant, colours) {
  device.setColor((x, y) => (y === 0 && x < colours.fixed.length ? colours.fixed[x] : [0xEE, 0xEE, 0xEE]));
  for (const [name, c] of Object.entries(colours.channels)) device.setChannelColors(name, c);
}

async function start(variant, opts) {
  const device = auraDevice(variant, variant.colours_a, opts);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const variant of expected.variants) {
  test(`${variant.pid} ${variant.name}: start, frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await start(variant);
    try {
      plugin.Initialize();
      assert.deepEqual(writes(device), variant.initialize, "Initialize");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(writes(device), variant.frame_a, "first frame (Direct effect, then colours)");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(writes(device), [], "an unchanged frame within a second is not resent");
      setColours(device, variant, variant.colours_b);
      plugin.Render();
      assert.deepEqual(writes(device), variant.frame_b, "frame with colour set B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(writes(device), variant.shutdown, "Shutdown: black");
    } finally {
      restore();
    }
  });

  test(`${variant.pid} ${variant.name}: geometry and channels from the configuration table`, async () => {
    const { device, plugin, restore } = await start(variant);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [variant.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.led_names, variant.led_positions]);
      for (const [x, y] of variant.led_positions) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1]);
      }
      assert.deepEqual(device.channels.map((c) => c.name), variant.channels);
      for (const c of device.channels) assert.equal(c.ledLimit, 500);
      assert.deepEqual(call("SetLedLimit"), [variant.channels.length * 500]);
    } finally {
      restore();
    }
  });
}

test("a channel that gets LEDs later is switched to Direct first", async () => {
  const variant = expected.variants.find((v) => v.pid === "0x18F3");
  const { device, plugin, restore } = await start(variant);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    device.setChannelColors("Addressable RGB Header 2", [[1, 2, 3]]);
    plugin.Render();
    const out = writes(device);
    // Header 2 is effect channel 2 (the onboard zone is 0), direct channel 1.
    assert.equal(out[0], `EC350200` + "00FF" + "00".repeat(59));
    assert.ok(out.some((h) => h.startsWith("EC408100010102 03".replace(/ /g, ""))));
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const variant = expected.variants[0];
  const { device, plugin, restore } = await start(variant);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(writes(device), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(writes(device), variant.frame_a.filter((h) => h.startsWith("EC40")));
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("without a configuration table reply nothing but the two requests is sent", async () => {
  const variant = expected.variants[0];
  const { device, plugin, restore } = await start(variant, { silent: true });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(writes(device), variant.initialize.slice(0, 2));
  } finally {
    restore();
  }
});

test("shutdown before any frame sends nothing", async () => {
  const variant = expected.variants[0];
  const { device, plugin, restore } = await start(variant);
  try {
    plugin.Initialize();
    device.calls.length = 0;
    plugin.Shutdown(true);
    assert.deepEqual(writes(device), []);
  } finally {
    restore();
  }
});

test("never a commit, save or shutdown-effect report", async () => {
  for (const variant of expected.variants) {
    const { device, plugin, restore } = await start(variant);
    try {
      plugin.Initialize();
      plugin.Render();
      setColours(device, variant, variant.colours_b);
      plugin.Render();
      plugin.Shutdown(true);
      for (const h of writes(device)) {
        const b = hexToBytes(h);
        assert.equal(b[0], 0xEC);
        assert.ok([0x82, 0xB0, 0x52, 0x35, 0x3B, 0x40].includes(b[1]), `command ${h.slice(0, 4)}`);
        if (b[1] === 0x35) assert.equal(b[4], 0x00, "no shutdown-effect flag");
        if (b[1] === 0x35) assert.equal(b[5], 0xFF, "Direct only");
        if (b[1] === 0x3B) assert.equal(b[4], 0xFF, "Direct only");
      }
    } finally {
      restore();
    }
  }
});

test("metadata: product ids equal the manifest, Validate follows the detectors", async () => {
  const device = createMockDevice({ productId: 0x18F3 });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    const ids = plugin.ProductId();
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, manifest.devices[0].pids.map(Number));
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF72, usage: 0xA1, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x01, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x06, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0x000C, usage: 0x01, collection: 0 }), false);
  } finally {
    restore();
  }
  const d2 = createMockDevice({ productId: 0x19AF });
  const restore2 = installGlobals(d2);
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF72, usage: 0xA1, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF72, usage: 0xA2, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0xA1, collection: 0 }), false);
  } finally {
    restore2();
  }
});
