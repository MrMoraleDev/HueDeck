// SPDX-License-Identifier: GPL-3.0-or-later
//
// Cooler Master MasterKeys (V1) plugin: for every product and every key table the firmware
// string can pick, the firmware query, two frames and the shutdown are byte for byte the
// packets worked out from OpenRGB's C++ (fixtures/expected.json); unchanged frames are only
// resent as a keepalive; Validate takes interface 1 / 0xFF00 / 1 only; the profile save (50 55)
// and profile / effect writes (51 ...) are never sent.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex, hexToBytes } from "../../_testkit/mock-device.mjs";

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

function canvas(variant, colours) {
  const byCell = new Map(variant.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(pid, variant, colours) {
  const reply = variant ? [0x00, ...hexToBytes(variant.firmware_reply)] : [];
  const device = createMockDevice({
    productId: Number(pid),
    color: variant ? canvas(variant, colours) : [0, 0, 0],
    inputReports: variant ? { 0: reply } : {},
  });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const product of expected.products) {
  for (const region of ["ANSI", "ISO", "JIS"]) {
    const variant = expected.variants[`${product.table}_${region}`];
    test(`${product.pid} ${product.name}, firmware ${variant.firmware}: query, frames, shutdown`, async () => {
      const { device, plugin, restore } = await setup(product.pid, variant, variant.colours_a);
      try {
        plugin.Initialize();
        assert.deepEqual(io(device), variant.initialize, "Initialize: firmware query");
        const call = (m) => device.calls.find((c) => c.method === m)?.args;
        assert.deepEqual(call("setName"), [product.name]);
        assert.deepEqual(call("setSize"), [variant.size]);
        assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
        for (const [x, y] of variant.leds.map((l) => l.pos)) {
          assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
        }
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
        assert.deepEqual(io(device), variant.shutdown, "Shutdown: back to firmware control");
      } finally {
        restore();
      }
    });
  }
}

test("no answer to the firmware query: ANSI table (OpenRGB's default)", async () => {
  const { device, plugin, restore } = await setup("0x003B", null, null);
  try {
    plugin.Initialize();
    const leds = device.calls.find((c) => c.method === "setControllableLeds").args[0];
    assert.equal(leds.length, expected.variants.PRO_L_ANSI.leds.length);
  } finally {
    restore();
  }
});

test("hand-checked key values against CMKeyboardDevices.cpp", () => {
  const value = (v, name) => expected.variants[v].leds.find((l) => l.name === name)?.value;
  // mk_pro_s_keymap: ESC 96, F10 67, ISO\ 100, # 89; mk_pro_l_keymap: ESC 11, NM0 107; P1..P4 101/109/117/119.
  assert.equal(value("PRO_S_ANSI", "Esc"), 96);
  assert.equal(value("PRO_S_ANSI", "F10"), 67);
  assert.equal(value("PRO_S_ANSI", "ISO_<"), undefined, "ANSI has no ISO backslash");
  assert.equal(value("PRO_S_ISO", "ISO_<"), 100);
  assert.equal(value("PRO_S_ISO", "ISO_#"), 89);
  assert.equal(value("PRO_S_ISO", "\\"), undefined, "ISO has no ANSI backslash");
  assert.equal(value("PRO_L_ANSI", "Esc"), 11);
  assert.equal(value("PRO_L_ANSI", "Num 0"), 107);
  assert.deepEqual(["P1", "P2", "P3", "P4"].map((n) => value("PRO_L_ANSI", n)), [101, 109, 117, 119]);
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const variant = expected.variants.PRO_S_ANSI;
  const { device, plugin, restore } = await setup("0x003C", variant, variant.colours_a);
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

test("metadata: product ids equal the manifest, Validate takes interface 1 / 0xFF00 / 1 only", async () => {
  const { plugin, restore } = await setup("0x003C", null, null);
  try {
    assert.deepEqual(plugin.ProductId(), expected.products.map((p) => Number(p.pid)));
    assert.deepEqual(plugin.ProductId(), manifest.devices[0].pids.map(Number));
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF00, usage: 0x0002, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x0001, usage: 0x0006, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("only 41 / 01 / C0 commands are ever written (no save 50, no profile or effect write 51)", async () => {
  for (const product of expected.products) {
    const variant = expected.variants[`${product.table}_ISO`];
    const { device, plugin, restore } = await setup(product.pid, variant, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device).filter((o) => o.op === "write")) {
        assert.ok(["0041", "0001", "00C0"].includes(op.hex.slice(0, 4)), op.hex.slice(0, 8));
      }
    } finally {
      restore();
    }
  }
});

test("an unknown product id drives nothing", async () => {
  const device = createMockDevice({ productId: 0x0089 });
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
