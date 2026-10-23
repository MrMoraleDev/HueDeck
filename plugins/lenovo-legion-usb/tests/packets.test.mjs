// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lenovo Legion USB plugin: for each product and each answer the laptop can give to OpenRGB's
// layout / profile queries, Initialize, two frames and Shutdown are byte for byte the feature
// reports worked out from OpenRGB's C++ (fixtures/expected.json), the layout OpenRGB picks is the
// one placed on the grid; Validate takes usage page 0xFF89 / usage 0x07 only; ProductId() equals
// the manifest; no profile save, brightness or profile switch is ever sent.

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
    const [data, length] = args;
    if (method === "read" || method === "get_report") return { op: method, len: length };
    if (method === "pause") return { op: "pause", ms: args[0] };
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

function canvas(variant, colours) {
  const byCell = new Map(variant.leds.map(([, pos], i) => [pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

// The laptop's answers: information query id -> byte 4; profile query (0xCA) -> byte 4.
function replies(variant) {
  return (sent) => {
    if (variant.protocol === "legacy") {
      const value = variant.info?.[`0x${sent[1].toString(16).toUpperCase().padStart(2, "0")}`];
      if (sent[1] >= 0x01 && sent[1] <= 0x07 && value !== undefined) {
        const reply = new Array(192).fill(0);
        reply[0] = 0x07; reply[1] = sent[1]; reply[4] = value;
        return reply;
      }
    } else if (sent[1] === 0xCA && variant.profile_reply !== null) {
      const reply = new Array(960).fill(0);
      reply[0] = 0x07; reply[1] = 0xCA; reply[2] = 0xC0; reply[3] = 0x03; reply[4] = variant.profile_reply;
      return reply;
    }
    return undefined;
  };
}

async function setup(variant, colours) {
  const device = createMockDevice({ productId: Number(variant.pid), color: canvas(variant, colours), featureReply: replies(variant) });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const variant of expected.variants) {
  test(`${variant.pid} ${variant.label}: init, frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await setup(variant, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize");
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [variant.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l[0]), variant.leds.map((l) => l[1])]);
      for (const [, [x, y]] of variant.leds) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
      }
      assert.equal(new Set(variant.leds.map(([, p]) => p.join(","))).size, variant.leds.length, "one LED per cell");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown");
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const variant = expected.variants.find((v) => v.pid === "0xC968");
  const { device, plugin, restore } = await setup(variant, variant.colours_a);
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

test("metadata: ProductId() equals the manifest, every product has a variant, Validate 0xFF89 / 0x07", async () => {
  const { plugin, restore } = await setup(expected.variants[0], expected.variants[0].colours_a);
  try {
    const ids = plugin.ProductId();
    assert.equal(ids.length, 12);
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, manifest.devices[0].pids.map(Number));
    assert.ok(!ids.includes(0xC195), "Legion 5 Gen 10 is held back");
    assert.deepEqual(new Set(expected.variants.map((v) => Number(v.pid))), new Set(ids));
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF89, usage: 0x07, collection: 1 }), true);
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0xFF89, usage: 0x07, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF89, usage: 0xCC, collection: 2 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x06, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("only lighting reports: no SAVE_PROFILE (CB), brightness (CE), profile switch (C8)", async () => {
  const allowedLegacy = new Set([0xB2, 0xB1, 0x01, 0x04, 0x07, 0xA1, 0xA2, 0xA3, 0xA4]);
  const allowedGen7 = new Set([0xCA, 0xD0, 0xA1]);
  for (const variant of expected.variants) {
    const { device, plugin, restore } = await setup(variant, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        if (op.op === "get_report") continue;
        assert.equal(op.op, "send_report");
        assert.equal(op.hex.slice(0, 2), "07");
        const cmd = parseInt(op.hex.slice(2, 4), 16);
        assert.ok((variant.protocol === "legacy" ? allowedLegacy : allowedGen7).has(cmd), `${variant.label}: 0x${op.hex.slice(2, 4)}`);
      }
    } finally {
      restore();
    }
  }
});

test("the held-back Legion 5 Gen 10 (0xC195) drives nothing", async () => {
  const device = createMockDevice({ productId: 0xC195 });
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
