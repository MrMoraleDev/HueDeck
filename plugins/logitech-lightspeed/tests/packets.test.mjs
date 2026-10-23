// SPDX-License-Identifier: GPL-3.0-or-later
//
// Logitech Lightspeed (wired) plugin, against a simulated HID++ 2.0 device that answers each long
// request: for every product id the queries, the switch to software control, two frames and the
// shutdown are byte for byte the I/O worked out from OpenRGB's C++ (fixtures/expected.json); a
// device with only the 0x8071 feature and a silent device are not driven; Validate takes the
// collection OpenRGB's detector names; after the read-only queries only feature 0x8070 functions
// 0x80 (with 01 01) and 0x30 are written.

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

/** The fixture's simulated device: answers every long request once, on the next read. */
function attachSimulator(device, spec) {
  let pending = null;
  let lastSize = 0;
  const write = device.write.bind(device);
  device.write = (data, length) => {
    const n = write(data, length);
    if (!spec || data[0] !== 0x11) return n;
    const feats = Object.fromEntries(Object.entries(spec.features).map(([k, v]) => [Number(k), v]));
    const [idx, fn] = [data[2], data[3]];
    const p = Array.from({ length: 16 }, (_, i) => data[4 + i] ?? 0);
    let reply = new Array(16).fill(0);
    if (idx === 0 && fn === 0x01) {
      reply[0] = feats[(p[0] << 8) | p[1]] ?? 0;
    } else if (idx && idx === feats[0x0005]) {
      const name = Array.from(spec.name, (c) => c.charCodeAt(0));
      if (fn === 0x01) reply[0] = name.length;
      else if (fn === 0x11) name.slice(p[0], p[0] + 16).forEach((c, i) => { reply[i] = c; });
      else if (fn === 0x21) reply[0] = spec.type;
    } else if (idx && idx === feats[0x8070]) {
      if (fn === 0x00) reply[0] = spec.leds.length;
      else if (fn === 0x10) {
        const led = spec.leds[p[0]];
        reply.splice(0, 4, p[0], led.location >> 8, led.location & 0xFF, led.fx.length);
      } else if (fn === 0x20) {
        const mode = spec.leds[p[0]].fx[p[1]];
        reply.splice(0, 8, p[0], p[1], mode >> 8, mode & 0xFF, 0, 0, 0x01, 0xF4);
      } else reply = p;
    } else {
      reply = p;
    }
    pending = [0x11, 0xFF, idx, fn, ...reply];
    return n;
  };
  device.read = (id, length, timeout) => {
    device.calls.push({ method: "read", args: [id, length, timeout] });
    const r = pending;
    pending = null;
    lastSize = r ? r.length : 0;
    return r ? r.slice() : [];
  };
  device.getLastReadSize = () => lastSize;
}

function canvas(sc, colours) {
  const byCell = new Map(sc.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(pid, scenarioName, colours) {
  const sc = expected.scenarios[scenarioName];
  const device = createMockDevice({ productId: Number(pid), color: sc?.driven ? canvas(sc, colours) : [0, 0, 0] });
  attachSimulator(device, scenarioName ? expected.devices[scenarioName] : null);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore, sc };
}

for (const product of expected.products) {
  test(`${product.pid} ${product.name}: queries, start, frames and shutdown are the C++ I/O`, async () => {
    const sc = expected.scenarios[product.scenario];
    const { device, plugin, restore } = await setup(product.pid, product.scenario, sc.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), sc.initialize, "Initialize");
      const call = (m) => device.calls.find((c) => c.method === m)?.args;
      assert.deepEqual(call("setName"), [sc.name]);
      assert.deepEqual(call("setSize"), [sc.size]);
      assert.deepEqual(call("setControllableLeds"), [sc.leds.map((l) => l.name), sc.leds.map((l) => l.pos)]);
      for (const [x, y] of sc.leds.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < sc.size[0] && y < sc.size[1], "LED inside the grid");
      }
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), sc.frame_a, "frame with colour set A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      device.setColor(canvas(sc, sc.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), sc.frame_b, "frame with colour set B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), sc.shutdown, "Shutdown: black");
    } finally {
      restore();
    }
  });

  test(`${product.pid}: Validate takes ${product.validate.interface === null ? "any interface" : `interface ${product.validate.interface}`}, usage page ${product.validate.usage_page}, usage ${product.validate.usage}`, async () => {
    const { plugin, restore } = await setup(product.pid, null, []);
    try {
      const page = Number(product.validate.usage_page);
      const usage = Number(product.validate.usage);
      const iface = product.validate.interface;
      for (const i of [0, 1, 2, 3]) {
        const want = iface === null || i === iface;
        assert.equal(plugin.Validate({ interface: i, usage_page: page, usage, collection: 2 }), want, `interface ${i}`);
      }
      const i0 = iface ?? 1;
      assert.equal(plugin.Validate({ interface: i0, usage_page: page, usage: 0x0001, collection: 1 }), false, "usage 1");
      assert.equal(plugin.Validate({ interface: i0, usage_page: page === 0xFF00 ? 0xFF43 : 0xFF00, usage, collection: 0 }), false, "other page");
    } finally {
      restore();
    }
  });
}

test("a device with only the 0x8071 RGB feature is held back after the read-only queries", async () => {
  const sc = expected.scenarios.ONLY_8071;
  const { device, plugin, restore } = await setup("0xC088", "ONLY_8071", []);
  try {
    plugin.Initialize();
    assert.deepEqual(io(device), sc.initialize);
    device.calls.length = 0;
    plugin.Render();
    plugin.Shutdown(true);
    assert.deepEqual(io(device), []);
    assert.ok(device.logs.some((l) => l.message.includes("0x8071")));
  } finally {
    restore();
  }
});

test("a device that never answers is tried ten times, then left alone", async () => {
  const sc = expected.scenarios.SILENT;
  const { device, plugin, restore } = await setup("0xC332", "SILENT", []);
  try {
    plugin.Initialize();
    assert.deepEqual(io(device), sc.initialize);
    assert.equal(io(device).filter((o) => o.op === "pause").length, 10);
    device.calls.length = 0;
    plugin.Render();
    plugin.Shutdown(true);
    assert.deepEqual(io(device), []);
  } finally {
    restore();
  }
});

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const sc = expected.scenarios.MOUSE;
  const { device, plugin, restore } = await setup("0xC08B", "MOUSE", sc.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(io(device), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(io(device), sc.frame_a);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("ProductId() lists every claimed id once, equal to the manifest", async () => {
  const { plugin, restore } = await setup(expected.products[0].pid, null, []);
  try {
    const ids = plugin.ProductId();
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, expected.products.map((p) => Number(p.pid)));
    assert.equal(manifest.devices.length, 1);
    assert.equal(Number(manifest.devices[0].vid), plugin.VendorId());
    assert.deepEqual(manifest.devices[0].pids.map(Number), ids);
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
  } finally {
    restore();
  }
});

test("after the queries only 0x8070 software control (01 01) and set-zone-effect are written", async () => {
  for (const name of ["MOUSE", "HEADSET"]) {
    const sc = expected.scenarios[name];
    const idx = expected.devices[name].features["0x8070"].toString(16).toUpperCase().padStart(2, "0");
    const { device, plugin, restore } = await setup("0xC08B", name, sc.colours_a);
    try {
      plugin.Initialize();
      const queries = io(device).filter((o) => o.op === "write");
      for (const w of queries.slice(0, -1)) {
        assert.ok(/^11FF(00010|..(00|01|10|11|20|21))/.test(w.hex), `query ${w.hex}`);
        assert.ok(!w.hex.startsWith(`11FF${idx}30`) && !w.hex.startsWith(`11FF${idx}80`), `query ${w.hex}`);
      }
      assert.equal(queries.at(-1).hex, `11FF${idx}800101${"0".repeat(28)}`);
      device.calls.length = 0;
      plugin.Render();
      plugin.Shutdown(true);
      for (const w of io(device).filter((o) => o.op === "write")) {
        assert.ok(new RegExp(`^11FF${idx}30[0-9A-F]{10}0{22}$`).test(w.hex), w.hex);
      }
    } finally {
      restore();
    }
  }
});
