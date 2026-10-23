// SPDX-License-Identifier: GPL-3.0-or-later
//
// Thermaltake Riing / Riing Quad / Riing Trio plugin: for every product id OpenRGB's detector
// lists, Initialize, frames and Shutdown are byte for byte the packets worked out from OpenRGB's
// C++ (fixtures/expected.json), including the bytes the Quad's and Trio's persistent buffers keep
// when a header gets fewer LEDs; unchanged frames are only resent as a keepalive; ProductId()
// equals the manifest; nothing but the init and RGB reports is ever written.

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
const IO = new Set(["write", "send_report", "read", "get_report", "pause"]);

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    if (method === "get_report") return { op: "get_report", len: args[1] };
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

const channelColors = (sets) => Object.fromEntries(sets.map((leds, ch) => [`Fan Header ${ch + 1}`, leds]));

async function setup(product, colours) {
  const device = createMockDevice({ productId: Number(product.pid), channelColors: channelColors(colours) });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

function setChannels(device, sets) {
  for (const [name, leds] of Object.entries(channelColors(sets))) device.setChannelColors(name, leds);
}

for (const product of expected.products) {
  test(`${product.pid} ${product.variant}: init, frames and shutdown are the C++ packets`, async () => {
    const v = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, v.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), v.initialize, "Initialize");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), v.frame_a, "frame A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      setChannels(device, v.colours_b);
      plugin.Render();
      assert.deepEqual(io(device), v.frame_b, "frame B");
      device.calls.length = 0;
      if (product.variant === "RIING") {
        plugin.Shutdown(false);
        assert.deepEqual(io(device), v.shutdown, "Shutdown: Spectrum");
      } else {
        setChannels(device, v.colours_shrink);
        plugin.Render();
        assert.deepEqual(io(device), v.frame_shrink, "fewer LEDs: persistent buffer bytes stay");
        device.calls.length = 0;
        plugin.Shutdown(true);
        assert.deepEqual(io(device), v.shutdown_after_shrink, "Shutdown: black in Direct");
      }
    } finally {
      restore();
    }
  });
}

test("five channels with OpenRGB's per-variant LED maximum, name set", async () => {
  const max = { RIING: 20, QUAD: 60, TRIO: 54 };
  for (const variant of Object.keys(max)) {
    const product = expected.products.find((p) => p.variant === variant);
    const { device, plugin, restore } = await setup(product, [[], [], [], [], []]);
    try {
      plugin.Initialize();
      assert.deepEqual(device.channels.map((c) => [c.name, c.ledLimit]),
        [1, 2, 3, 4, 5].map((n) => [`Fan Header ${n}`, max[variant]]));
      assert.deepEqual(device.calls.find((c) => c.method === "SetLedLimit").args, [5 * max[variant]]);
      assert.deepEqual(device.calls.find((c) => c.method === "setName").args, [product.name.replace(/ \(PID .*\)$/, "")]);
    } finally {
      restore();
    }
  }
});

test("more LEDs than the header maximum are cut off (no buffer overrun)", async () => {
  const product = expected.products.find((p) => p.variant === "RIING");
  const many = Array.from({ length: 25 }, () => [1, 2, 3]);
  const { device, plugin, restore } = await setup(product, [many, [], [], [], []]);
  try {
    plugin.Initialize();
    device.calls.length = 0;
    plugin.Render();
    const first = io(device)[0];
    assert.equal(first.len, 65);
    assert.equal(first.hex, "0032520118" + "020103".repeat(20));
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const product = expected.products.find((p) => p.variant === "QUAD");
  const v = expected.variants.QUAD;
  const { device, plugin, restore } = await setup(product, v.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(io(device), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(io(device), v.frame_a);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("metadata: ProductId() equals the manifest, Validate takes any collection", async () => {
  const { plugin, restore } = await setup(expected.products[0], [[], [], [], [], []]);
  try {
    const ids = plugin.ProductId();
    assert.equal(ids.length, 51);
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, expected.products.map((p) => Number(p.pid)));
    assert.deepEqual(ids, manifest.devices[0].pids.map(Number));
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 1, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: -1, usage_page: 0x0001, usage: 0, collection: 0 }), true);
  } finally {
    restore();
  }
});

test("only init (FE 33) and RGB (32 52) reports are written", async () => {
  for (const product of expected.products) {
    const v = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product, v.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(false);
      for (const op of io(device)) {
        if (op.op === "read") continue;
        assert.equal(op.op, "write");
        assert.ok(op.hex.startsWith("00FE33") || op.hex.startsWith("003252"), op.hex.slice(0, 10));
      }
    } finally {
      restore();
    }
  }
});

test("an unknown product id sends nothing", async () => {
  const device = createMockDevice({ productId: 0x2140 });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});
