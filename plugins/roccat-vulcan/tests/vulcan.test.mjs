// SPDX-License-Identifier: GPL-3.0-or-later
//
// Roccat Vulcan family plugin (the same test runs in plugins/roccat-vulcan/ and
// plugins/roccat-vulcan-turtle-beach/): for every product id OpenRGB registers for this vendor id,
// Initialize (info read, direct mode on, ready poll), two frames and Shutdown (direct mode off) are
// byte for byte the I/O worked out from OpenRGB's C++ (fixtures/expected.json), on the right
// collection; Validate takes the control and LED collections only; the mode report OpenRGB stores
// (SendMode, 0x0D / 0x11) is never sent; ProductId() equals the manifest; vulcan.js and layouts.js
// are the same files in both plugin directories.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { basename, dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex, hexToBytes } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginDir = join(here, "..");
const pluginPath = join(pluginDir, "plugin.js");
const expected = JSON.parse(readFileSync(join(pluginDir, "fixtures", "expected.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(pluginDir, "manifest.json"), "utf8"));
const IO = new Set(["write", "send_report", "get_report", "read", "pause", "set_endpoint"]);

function endpointsOf(c) {
  const ctrl = { interface: c.endpoints.ctrl.interface, usage: 0x0001, usage_page: c.endpoints.ctrl.usage_page, collection: 0 };
  const led = { interface: c.endpoints.led.interface, usage: 0x0001, usage_page: c.endpoints.led.usage_page, collection: 0 };
  // Other collections of the keyboard, which the plugin must leave alone.
  const others = [
    { interface: 0, usage: 0x0006, usage_page: 0x0001, collection: 0 },
    { interface: 2, usage: 0x0001, usage_page: 0x000C, collection: 0 },
  ];
  return { ctrl, led, list: [others[0], ctrl, others[1], led] };
}

/** The I/O calls a plugin made, in order, in the fixture's shape. */
function io(device, eps) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    if (method === "get_report") return { op: "get_report", id: args[0][0], len: args[1] };
    if (method === "set_endpoint") {
      const [iface, usage, page] = args;
      const which = Object.entries({ ctrl: eps.ctrl, led: eps.led })
        .find(([, e]) => e.interface === iface && e.usage === usage && e.usage_page === page);
      return { op: "set_endpoint", ep: which ? which[0] : `?${iface}/${page}` };
    }
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

function canvas(c, colours) {
  const byCell = new Map();
  c.leds.forEach((led, i) => { if (led.pos) byCell.set(led.pos.join(","), colours[i]); });
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(c, colours, { endpoints } = {}) {
  const eps = endpointsOf(c);
  const info = hexToBytes(c.info_reply);
  const ready = hexToBytes(c.ready_reply);
  const device = createMockDevice({
    productId: Number(c.pid),
    color: canvas(c, colours),
    featureReports: { [info[0]]: info, 4: ready },
  });
  device.getHidEndpoints = () => endpoints ?? eps.list;
  device.set_endpoint = (...args) => { device.calls.push({ method: "set_endpoint", args }); };
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore, eps };
}

for (const c of expected.cases) {
  const label = `${c.pid} ${c.name} (${c.layout_reported} reported, ${c.layout_used} used)`;

  test(`${label}: start, frames and shutdown are the C++ I/O`, async () => {
    const { device, plugin, restore, eps } = await setup(c, c.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device, eps), c.initialize, "Initialize");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device, eps), c.frame_a, "frame with colour set A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device, eps), [], "an unchanged frame within a second is not resent");
      device.setColor(canvas(c, c.colours_b));
      plugin.Render();
      assert.deepEqual(io(device, eps), c.frame_b, "frame with colour set B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device, eps), c.shutdown, "Shutdown: direct mode off");
    } finally {
      restore();
    }
  });

  test(`${label}: name and key grid`, async () => {
    const { device, plugin, restore } = await setup(c, c.colours_a);
    try {
      plugin.Initialize();
      const call = (m) => device.calls.find((x) => x.method === m)?.args;
      const placed = c.leds.filter((l) => l.pos);
      assert.deepEqual(call("setName"), [`${c.name} (${c.layout_used})`]);
      assert.deepEqual(call("setSize"), [c.size]);
      assert.deepEqual(call("setControllableLeds"), [placed.map((l) => l.name), placed.map((l) => l.pos)]);
      assert.equal(new Set(placed.map((l) => l.name)).size, placed.length, "unique key names");
      const cells = new Set(placed.map((l) => l.pos.join(",")));
      assert.equal(cells.size, placed.length, "one key per cell");
      for (const [x, y] of placed.map((l) => l.pos)) {
        assert.ok(x >= 0 && y >= 0 && x < c.size[0] && y < c.size[1], "key inside the grid");
      }
    } finally {
      restore();
    }
  });
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const c = expected.cases[0];
  const { device, plugin, restore, eps } = await setup(c, c.colours_a);
  try {
    plugin.Initialize();
    plugin.Render();
    device.calls.length = 0;
    mock.timers.tick(999);
    plugin.Render();
    assert.deepEqual(io(device, eps), []);
    mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual(io(device, eps), c.frame_a);
  } finally {
    restore();
    mock.timers.reset();
  }
});

test("ProductId() lists every product id of this vendor once and equals the manifest", async () => {
  const { plugin, restore } = await setup(expected.cases[0], expected.cases[0].colours_a);
  try {
    const ids = plugin.ProductId();
    const fixtureIds = [...new Set(expected.cases.map((c) => Number(c.pid)))];
    assert.deepEqual([...ids].sort(), [...fixtureIds].sort());
    assert.equal(new Set(ids).size, ids.length);
    assert.equal(manifest.devices.length, 1);
    assert.deepEqual(manifest.devices[0].pids.map(Number).sort(), [...ids].sort());
    assert.equal(Number(manifest.devices[0].vid), plugin.VendorId());
    assert.deepEqual(manifest.devices[0].tested, []);
    for (const c of expected.cases) assert.equal(Number(c.vid), plugin.VendorId());
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
  } finally {
    restore();
  }
});

test("Validate takes the control and the LED collection only", async () => {
  for (const c of expected.cases) {
    const { plugin, restore, eps } = await setup(c, c.colours_a);
    try {
      assert.equal(plugin.Validate(eps.ctrl), true, `${c.pid} control`);
      assert.equal(plugin.Validate({ ...eps.ctrl, usage: 0x0080, collection: 2 }), true, `${c.pid} control, any usage`);
      assert.equal(plugin.Validate(eps.led), true, `${c.pid} LED`);
      for (const other of eps.list.filter((e) => e !== eps.ctrl && e !== eps.led)) {
        assert.equal(plugin.Validate(other), false, `${c.pid} ${JSON.stringify(other)}`);
      }
      assert.equal(plugin.Validate({ ...eps.ctrl, usage_page: 0xFFC0 }), false, `${c.pid} wrong page on interface 1`);
      assert.equal(plugin.Validate({ ...eps.led, interface: 5 }), false, `${c.pid} LED page on another interface`);
    } finally {
      restore();
    }
  }
});

test("only the direct-mode switch, the info and ready reads and colour frames are ever sent", async () => {
  for (const c of expected.cases) {
    const { device, plugin, restore, eps } = await setup(c, c.colours_a);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      let active = null;
      const ops = io(device, eps);
      assert.equal(ops.find((op) => op.op === "write").hex.slice(0, 6), "00A101", `${c.pid}: frame header`);
      for (const op of ops) {
        if (op.op === "set_endpoint") active = op.ep;
        if (op.op === "send_report") {
          assert.equal(active, "ctrl", `${c.pid}: feature reports go to the control collection`);
          assert.ok(["15", "0E"].includes(op.hex.slice(0, 2)), `${c.pid}: ${op.hex}`);
          assert.ok(op.len <= 5, `${c.pid}: no mode (SendMode 0x0D / 0x11) or keepalive (0x13) report`);
        }
        if (op.op === "write") {
          assert.equal(active, "led", `${c.pid}: colour frames go to the LED collection`);
          assert.equal(op.len, 65);
          assert.equal(op.hex.slice(0, 2), "00", "report id 0x00");
        }
      }
    } finally {
      restore();
    }
  }
});

test("without both collections nothing is sent", async () => {
  const c = expected.cases[0];
  const eps = endpointsOf(c);
  const { device, plugin, restore } = await setup(c, c.colours_a, { endpoints: [eps.ctrl] });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(io(device, eps), []);
    assert.equal(device.logs.length, 1);
  } finally {
    restore();
  }
});

test("of several LED-page collections, the one declaring the 65-byte output report is used", async () => {
  const c = expected.cases[0];
  const eps = endpointsOf(c);
  const decoy = { ...eps.led, usage: 0x0006, collection: 1 };
  const led = { ...eps.led, collection: 2 };
  const { device, plugin, restore } = await setup(c, c.colours_a, { endpoints: [eps.ctrl, decoy, led] });
  let active = null;
  device.set_endpoint = (...args) => { active = args; device.calls.push({ method: "set_endpoint", args }); };
  device.getHidReportFields = () => ({
    reports: active && active[3] === 2 ? [{ kind: "output", reportId: 0, bytes: 65 }] : [{ kind: "input", reportId: 1, bytes: 9 }],
  });
  try {
    plugin.Initialize();
    assert.deepEqual(active, [led.interface, led.usage, led.usage_page, led.collection]);
  } finally {
    restore();
  }
});

test("an unknown product id drives nothing", async () => {
  const device = createMockDevice({ productId: 0x1234 });
  device.getHidEndpoints = () => [];
  device.set_endpoint = () => { device.calls.push({ method: "set_endpoint", args: [] }); };
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(device.calls.filter((x) => IO.has(x.method)), []);
    assert.equal(device.logs.length, 1);
  } finally {
    restore();
  }
});

test("vulcan.js and layouts.js are the same in both Vulcan plugin directories", () => {
  const other = basename(pluginDir) === "roccat-vulcan" ? "roccat-vulcan-turtle-beach" : "roccat-vulcan";
  for (const f of ["vulcan.js", "layouts.js"]) {
    assert.equal(readFileSync(join(pluginDir, f), "utf8"), readFileSync(join(pluginDir, "..", other, f), "utf8"), f);
  }
});
