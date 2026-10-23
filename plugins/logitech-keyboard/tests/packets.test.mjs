// SPDX-License-Identifier: GPL-3.0-or-later
//
// Logitech G815 / G915 plugin: for every product id, the start, two frames and the shutdown are
// byte for byte the reports worked out from OpenRGB's C++ (fixtures/expected.json); an unchanged
// frame sends nothing; the 0xC547 receiver drives only a G915 / G915 TKL its name probe reports;
// Validate takes the collection OpenRGB opens; nothing but per-key frames and commits (feature
// 0x8081) is ever written after the read-only name probe.

import { test } from "node:test";
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

async function setup(pid, colours, variant, probeReply) {
  const device = createMockDevice({
    productId: Number(pid),
    color: variant ? canvas(variant, colours) : [0, 0, 0],
    inputReports: probeReply ? { 0x11: hexToBytes(probeReply) } : {},
  });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const product of expected.products) {
  const variant = expected.variants[product.variant];

  test(`${product.pid} ${product.name}: start, frames and shutdown are the C++ reports`, async () => {
    const { device, plugin, restore } = await setup(product.pid, variant.colours_a, variant, product.probe_reply);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), [...(product.probe_ops ?? []), ...variant.initialize], "Initialize");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame sends nothing");
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

  test(`${product.pid} ${product.name}: name and key grid`, async () => {
    const { device, plugin, restore } = await setup(product.pid, variant.colours_a, variant, product.probe_reply);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [product.name]);
      assert.deepEqual(call("setSize"), [variant.size]);
      assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
      const cells = new Set();
      for (const [x, y] of variant.leds.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < variant.size[0] && y < variant.size[1], "LED inside the grid");
        assert.ok(!cells.has(`${x},${y}`), "one LED per cell");
        cells.add(`${x},${y}`);
      }
    } finally {
      restore();
    }
  });

  test(`${product.pid}: Validate takes interface ${product.validate.interface}, usage page ${product.validate.usage_page}, usage ${product.validate.usage} only`, async () => {
    const { plugin, restore } = await setup(product.pid, [], null, null);
    try {
      const want = {
        interface: product.validate.interface,
        usage_page: Number(product.validate.usage_page),
        usage: Number(product.validate.usage),
        collection: 0,
      };
      assert.equal(plugin.Validate(want), true);
      assert.equal(plugin.Validate({ ...want, collection: 3 }), true, "any collection number");
      assert.equal(plugin.Validate({ ...want, interface: want.interface + 1 }), false);
      assert.equal(plugin.Validate({ ...want, usage_page: 0xFF00 + 0xFF43 - want.usage_page }), false);
      assert.equal(plugin.Validate({ ...want, usage: want.usage === 0x0602 ? 0x0604 : 0x0001 }), false);
    } finally {
      restore();
    }
  });
}

for (const probe of expected.probe_cases) {
  test(`0xC547 receiver, name probe answer ${probe.reply ?? "(none)"}`, async () => {
    const variant = probe.variant ? expected.variants[probe.variant] : null;
    const { device, plugin, restore } = await setup("0xC547", variant?.colours_a ?? [], variant, probe.reply);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), [...probe.ops, ...(variant?.initialize ?? [])], "Initialize");
      device.calls.length = 0;
      plugin.Render();
      if (variant) {
        device.setColor(canvas(variant, variant.colours_b));
      }
      plugin.Render();
      plugin.Shutdown(false);
      if (variant) {
        assert.deepEqual(io(device), [...variant.frame_a, ...variant.frame_b, ...variant.shutdown]);
      } else {
        assert.deepEqual(io(device), [], "nothing is driven");
        assert.ok(device.logs.length > 0, "the reason is logged");
      }
    } finally {
      restore();
    }
  });
}

test("ProductId() lists every claimed id once, equal to the manifest; held-back ids are not claimed", async () => {
  const { plugin, restore } = await setup(expected.products[0].pid, [], null, null);
  try {
    const ids = plugin.ProductId();
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, expected.products.map((p) => Number(p.pid)));
    assert.equal(manifest.devices.length, 1);
    assert.equal(Number(manifest.devices[0].vid), plugin.VendorId());
    assert.deepEqual(manifest.devices[0].pids.map(Number), ids);
    assert.deepEqual(manifest.devices[0].tested, []);
    for (const held of expected.held_back) {
      assert.ok(!ids.includes(Number(held.pid)), held.pid);
    }
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
});

test("a product id the plugin does not know drives nothing", async () => {
  const { device, plugin, restore } = await setup("0xC336", [], null, null);
  try {
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF43, usage: 0x0602, collection: 0 }), false);
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});

test("after the name probe only per-key frames and commits (feature 0x8081 functions 1F, 6F, 7F) are written", async () => {
  const addresses = { G815: ["FF", "10"], G915_WIRED: ["FF", "0A"], G915_WIRELESS: ["01", "0B"], G915_TKL_WIRED: ["FF", "0A"], G915_TKL_WIRELESS: ["01", "0B"] };
  for (const product of expected.products) {
    const variant = expected.variants[product.variant];
    const { device, plugin, restore } = await setup(product.pid, variant.colours_a, variant, product.probe_reply);
    try {
      plugin.Initialize();
      plugin.Render();
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      plugin.Shutdown(true);
      const writes = io(device).filter((o) => o.op !== "read").slice((product.probe_ops ?? []).filter((o) => o.op === "write").length);
      const [dev, feature] = addresses[product.variant];
      for (const w of writes) {
        assert.equal(w.op, "write");
        assert.equal(w.len, 20);
        assert.equal(w.hex.slice(0, 6), `11${dev}${feature}`, `${product.pid}: ${w.hex.slice(0, 8)}`);
        assert.ok(["1F", "6F", "7F"].includes(w.hex.slice(6, 8)), `${product.pid}: function ${w.hex.slice(6, 8)}`);
      }
    } finally {
      restore();
    }
  }
});
