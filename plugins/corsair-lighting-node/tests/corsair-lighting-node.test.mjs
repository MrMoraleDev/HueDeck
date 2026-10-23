// SPDX-License-Identifier: GPL-3.0-or-later
//
// Corsair Lighting Node plugin: start, two frames on the two channels (empty, short, over 50 and
// full 204-LED channels) and the shutdown are byte for byte the I/O of OpenRGB's own code
// (fixtures/expected.json); unchanged frames are resent only as a keepalive; Validate takes
// vendor-defined collections only; ProductId() equals the manifest; only port state, direct,
// commit and firmware packets are ever written.

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
const IO = new Set(["write", "send_report", "read", "pause", "get_report"]);
const CHANNELS = ["Corsair RGB Header 1", "Corsair RGB Header 2"];

function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

async function setup(variant, set, productId = 0x0C0B) {
  const device = createMockDevice({
    productId,
    channelColors: { [CHANNELS[0]]: variant[`channel1_${set}`], [CHANNELS[1]]: variant[`channel2_${set}`] },
    inputReports: variant.answers ? { 0: [0x00, 0x00, 0x00, 0x0A, 0x06] } : {},
  });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const [label, variant] of Object.entries(expected.variants)) {
  test(`${label}: start, frames and shutdown are OpenRGB's I/O`, async () => {
    const { device, plugin, restore } = await setup(variant, "a");
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize");
      assert.deepEqual(device.channels.map((c) => [c.name, c.ledLimit]), variant.zones.map((z) => [z, variant.leds_max]));
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      device.setChannelColors(CHANNELS[0], variant.channel1_b);
      device.setChannelColors(CHANNELS[1], variant.channel2_b);
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown: hardware control and commit");
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const variant = expected.variants.TWO_CHANNELS;
  const { device, plugin, restore } = await setup(variant, "a");
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

test("ProductId() equals the manifest; Validate takes vendor-defined collections only", async () => {
  const device = createMockDevice({ productId: 0x0C0B });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.deepEqual(plugin.ProductId(), manifest.devices[0].pids.map(Number));
    assert.deepEqual(plugin.ProductId(), [0x0C1A, 0x0C0B, 0x0C10, 0x0C1E, 0x1D00, 0x1D04, 0x0C23]);
    assert.equal(plugin.VendorId(), 0x1B1C);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFFC0, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0006, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("every product gets its name and two 204-LED channels; an unknown id is never driven", async () => {
  for (const pid of manifest.devices[0].pids.map(Number)) {
    const { device, plugin, restore } = await setup(expected.variants.TWO_CHANNELS, "a", pid);
    try {
      plugin.Initialize();
      assert.equal(device.calls.find((c) => c.method === "setName").args[0].startsWith("Corsair"), true);
      assert.equal(device.channels.length, 2);
    } finally {
      restore();
    }
  }
  const { device, plugin, restore } = await setup(expected.variants.TWO_CHANNELS, "a", 0x0C1B);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});

test("only firmware request, port state, direct and commit packets are written", async () => {
  for (const variant of Object.values(expected.variants)) {
    const { device, plugin, restore } = await setup(variant, "a");
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        if (op.op !== "write") continue;
        const b = op.hex.match(/../g).map((h) => parseInt(h, 16));
        assert.equal(op.len, 65);
        assert.equal(b[0], 0);
        assert.ok([0x02, 0x32, 0x33, 0x38].includes(b[1]), `packet id ${b[1]}`);
        if (b[1] === 0x38) assert.ok(b[2] < 2 && (b[3] === 1 || b[3] === 2));
        if (b[1] === 0x33) assert.equal(b[2], 0xFF);
      }
    } finally {
      restore();
    }
  }
});
