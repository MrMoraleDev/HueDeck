// SPDX-License-Identifier: GPL-3.0-or-later
//
// Corsair Peripheral plugin: for every product id OpenRGB's detector lists, the start, two frames
// and the shutdown are byte for byte the packets worked out from OpenRGB's C++
// (fixtures/expected.json), also with the ISO layout and the feature-report fallback of the
// firmware-info request; a device that never says what it is gets nothing after ten requests;
// unchanged frames are only resent as a keepalive; Validate follows the detector macros; the
// product ids equal the manifest's; no hardware-effect (stored) command is ever sent.

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
const IO = new Set(["write", "send_report", "get_report", "read", "pause"]);

/** The I/O calls a plugin made, in order, in the fixture's shape. */
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

/** A canvas giving each LED cell of `variant` its colour from `colours`. */
function canvas(variant, colours) {
  const byCell = new Map(variant.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

/** A mock device answering the firmware-info request as the variant says. */
async function setup(pid, variant, colours) {
  const options = { productId: Number(pid), color: canvas(variant, colours) };
  if (variant.reply) {
    const reply = [0x00, ...hexToBytes(variant.reply.payload)];
    if (variant.reply.path === "read") options.inputReports = { 0: reply };
    else options.featureReports = { 0: reply };
  }
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

async function runVariant(pid, variant) {
  const { device, plugin, restore } = await setup(pid, variant, variant.colours_a);
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
}

for (const product of [...expected.products, ...expected.extra]) {
  const variant = expected.variants[product.variant];
  test(`${product.pid} ${product.name} (${product.variant}): start, frames and shutdown are the C++ packets`, async () => {
    await runVariant(product.pid, variant);
  });
}

for (const product of expected.products) {
  test(`${product.pid} ${product.name}: name and LED grid`, async () => {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product.pid, variant, variant.colours_a);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [product.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
      const cells = new Set();
      for (const [x, y] of variant.leds.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
        cells.add(`${x},${y}`);
      }
      assert.equal(cells.size, variant.leds.length, "one cell per LED");
      assert.equal(new Set(variant.leds.map((l) => l.name)).size, variant.leds.length, "LED names are unique");
    } finally {
      restore();
    }
  });
}

test("a device that never says what it is: ten firmware-info requests, then nothing", async () => {
  const u = expected.unknown_type;
  const device = createMockDevice({ productId: Number(u.pid), inputReports: { 0: [0x00, ...hexToBytes(u.reply_payload)] } });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    assert.deepEqual(io(device), u.initialize);
    assert.equal(io(device).filter((o) => o.op === "read").length, 10);
    device.calls.length = 0;
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  for (const key of ["K65MINI", "K55XT", "mouse-read-ansi"]) {
    const variant = expected.variants[key];
    const product = expected.products.find((p) => p.variant === key);
    const { device, plugin, restore } = await setup(product.pid, variant, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      device.calls.length = 0;
      mock.timers.tick(999);
      plugin.Render();
      assert.deepEqual(io(device), [], key);
      mock.timers.tick(1);
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, key);
    } finally {
      restore();
    }
  }
  mock.timers.reset();
});

test("ProductId() lists every detector id once and equals the manifest", async () => {
  const device = createMockDevice({ productId: 0 });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    const ids = plugin.ProductId();
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, expected.products.map((p) => Number(p.pid)));
    const claimed = manifest.devices.flatMap((d) => d.pids.map(Number));
    assert.deepEqual([...claimed].sort((a, b) => a - b), [...ids].sort((a, b) => a - b));
    for (const d of manifest.devices) {
      assert.equal(Number(d.vid), plugin.VendorId());
      assert.deepEqual(d.tested, []);
    }
    assert.equal(plugin.VendorId(), 0x1B1C);
    assert.deepEqual(manifest.conflictingProcesses, plugin.ConflictingProcesses());
  } finally {
    restore();
  }
});

test("Validate follows the detector macros", async () => {
  const cases = [
    // [pid, endpoint, expected]
    [0x1B13, { interface: 1, usage_page: 0xFFC2, usage: 0x0004, collection: 2 }, true],
    [0x1B13, { interface: 1, usage_page: 0x000C, usage: 0x0001, collection: 1 }, false],
    [0x1B13, { interface: 0, usage_page: 0xFFC2, usage: 0x0004, collection: 0 }, false],
    [0x1B5D, { interface: 1, usage_page: 0xFFC2, usage: 0x0004, collection: 0 }, true],
    [0x1B5D, { interface: 2, usage_page: 0xFFC2, usage: 0x0004, collection: 0 }, false],
    [0x1B3B, { interface: 0, usage_page: 0xFFC2, usage: 0x0004, collection: 0 }, true],
    [0x0A34, { interface: 2, usage_page: 0xFFC2, usage: 0x0004, collection: 0 }, true],
    [0x0A34, { interface: 0, usage_page: 0x0001, usage: 0x0006, collection: 0 }, false],
    [0x1BAF, { interface: 1, usage_page: 0xFF42, usage: 0x0001, collection: 0 }, true],
    [0x1BAF, { interface: 1, usage_page: 0x0001, usage: 0x0006, collection: 1 }, true],
    [0x1BAF, { interface: 0, usage_page: 0xFF42, usage: 0x0001, collection: 0 }, false],
    [0x1BA1, { interface: 1, usage_page: 0xFF42, usage: 0x0001, collection: 0 }, true],
    [0x1BA1, { interface: 1, usage_page: 0xFFC2, usage: 0x0001, collection: 0 }, false],
    [0x1BA1, { interface: 0, usage_page: 0xFF42, usage: 0x0001, collection: 0 }, false],
    [0x1234, { interface: 1, usage_page: 0xFFC2, usage: 0x0004, collection: 0 }, false],
  ];
  for (const [pid, endpoint, want] of cases) {
    const device = createMockDevice({ productId: pid });
    const restore = installGlobals(device);
    try {
      const plugin = await loadPlugin(pluginPath);
      assert.equal(plugin.Validate(endpoint), want, `0x${pid.toString(16)} ${JSON.stringify(endpoint)}`);
    } finally {
      restore();
    }
  }
  // Every product's rule, as recorded from the detector.
  for (const p of expected.products) {
    const device = createMockDevice({ productId: Number(p.pid) });
    const restore = installGlobals(device);
    try {
      const plugin = await loadPlugin(pluginPath);
      const iface = p.validate.interface ?? 3;
      const page = Number(p.validate.usage_page ?? 0xFF00);
      assert.equal(plugin.Validate({ interface: iface, usage_page: page, usage: 1, collection: 0 }), true, p.pid);
      if ("interface" in p.validate) {
        assert.equal(plugin.Validate({ interface: iface + 1, usage_page: page, usage: 1, collection: 0 }), false, p.pid);
      }
      if ("usage_page" in p.validate) {
        assert.equal(plugin.Validate({ interface: iface, usage_page: 0x0001, usage: 1, collection: 0 }), false, p.pid);
      }
    } finally {
      restore();
    }
  }
});

test("no hardware-effect, store or commit command is ever sent", async () => {
  for (const product of [...expected.products, ...expected.extra]) {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product.pid, variant, variant.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        if (op.op !== "write" && op.op !== "send_report") continue;
        const head = op.hex.slice(0, 8);
        // CUE: 07 17 (file / commit), 07 04 01 (hardware), 07 05 01 (hardware lighting),
        // 07 13 (hardware profile), 07 02 (reset).
        for (const bad of ["0007170", "00070401", "00070501", "000713", "000702"]) {
          assert.ok(!op.hex.startsWith(bad), `${product.pid}: ${head}`);
        }
        // 0x08 protocol: 08 05 (close/commit), 08 09, 08 0D 01 61 (effect buffer), 00 00 01 03.
        for (const bad of ["000805", "000809", "00080D0161", "00000103"]) {
          assert.ok(!op.hex.startsWith(bad), `${product.pid}: ${head}`);
        }
        // Report id 0x00, then only the commands this plugin is meant to use.
        assert.ok(op.hex.startsWith("00") && ["07", "7F", "08", "0E"].includes(op.hex.slice(2, 4)), `${product.pid}: unexpected ${head}`);
        if (op.hex.slice(2, 4) === "07") {
          // CUE writes: special function 04, lighting control 05, submit 22 / 25 / 28, key packets 40.
          assert.ok(["04", "05", "22", "25", "28", "40"].includes(op.hex.slice(4, 6)), `${product.pid}: unexpected ${head}`);
        }
        if (op.hex.slice(2, 4) === "08") {
          // 0x08 protocol: 01 (mode), 02, 0D 00 / 0D 01 22 (open), 06 / 07 (colour data).
          assert.ok(["01", "02", "0D", "06", "07"].includes(op.hex.slice(4, 6)), `${product.pid}: unexpected ${head}`);
        }
      }
    } finally {
      restore();
    }
  }
});

test("an unknown product id drives nothing", async () => {
  const device = createMockDevice({ productId: 0x1B99 });
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
