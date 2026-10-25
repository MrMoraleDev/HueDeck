// SPDX-License-Identifier: GPL-3.0-or-later
//
// Corsair Peripheral V2 plugin: for every protocol variant (software / hardware controller, light
// endpoint 1 or 2, K100 pinned endpoint, K57 without reads, wireless receivers, unknown and silent
// devices) the start, two frames and the shutdown are byte for byte the I/O of OpenRGB's own code
// against the same scripted device (fixtures/expected.json); unchanged frames are only resent as a
// keepalive; Validate takes interface 1 / usage page 0xFF42 only; ProductId() equals the manifest;
// every LED grid is inside its size; nothing but the reviewed commands is ever written.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "../../_testkit/mock-device.mjs";
import { TABLES, DEVICES } from "../devices.js";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
const IO = new Set(["write", "send_report", "read", "pause", "get_report"]);

/** The I/O calls a plugin made, in order, in the fixture's shape. */
function io(device) {
  return device.calls.filter((c) => IO.has(c.method)).map(({ method, args }) => {
    if (method === "pause") return { op: "pause", ms: args[0] };
    if (method === "read") return { op: "read", len: args[1], timeout: args[2] };
    const [data, length] = args;
    const bytes = Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0));
    return { op: method, len: length, hex: bytesToHex(bytes).replace(/ /g, "") };
  });
}

/**
 * A scripted Corsair V2 device, the same model as the fixture generator's: every request but a
 * block-write continuation (0x07) is answered with [write_cmd, command, status, value LE], behind
 * a 0x00 report id.
 */
function scripted(device, script) {
  let pending = null;
  let wirelessZero = script.wireless_zero_answers;
  const write = device.write.bind(device);
  device.write = (data, length) => {
    const n = write(data, length);
    const wc = data[1] & 0xFF;
    const cmd = data[2] & 0xFF;
    const addr = data[3] & 0xFF;
    pending = null;
    if (!script.answers || cmd === 0x07) {
      return n;
    }
    let value = 0;
    let status = 0;
    if (cmd === 0x02) {
      if (addr === 0x12) {
        if (wc === 0x09) {
          if (wirelessZero > 0) {
            wirelessZero--;
          } else {
            value = script.pid_wireless;
          }
        } else {
          value = script.pid_wired;
        }
      } else if (addr === 0x11) {
        value = 0x1B1C;
      } else if (addr === 0x41) {
        value = script.layout_code;
      }
    } else if (cmd === 0x0D) {
      status = script.start_tx_status;
    }
    const answer = new Array(64).fill(0);
    answer[0] = wc;
    answer[1] = cmd;
    answer[2] = status;
    answer[3] = value & 0xFF;
    answer[4] = (value >> 8) & 0xFF;
    answer[5] = (value >> 16) & 0xFF;
    answer[6] = (value >>> 24) & 0xFF;
    pending = [0x00, ...answer];
    return n;
  };
  let lastSize = 0;
  device.read = (id, length, timeout) => {
    device.calls.push({ method: "read", args: [id, length, timeout] });
    if (!pending) {
      lastSize = 0;
      return [];
    }
    const out = pending.slice(0, length);
    pending = null;
    lastSize = out.length;
    return out;
  };
  device.getLastReadSize = () => lastSize;
}

function canvas(variant, colours) {
  const byCell = new Map(variant.leds.map((led, i) => [led.pos.join(","), colours[i]]));
  return (x, y) => byCell.get(`${x},${y}`) ?? [0xEE, 0xEE, 0xEE];
}

async function setup(variant, colours) {
  const device = createMockDevice({ productId: Number(variant.hid_pid), color: variant.leds ? canvas(variant, colours) : [0, 0, 0] });
  scripted(device, variant.device);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

for (const [label, variant] of Object.entries(expected.variants)) {
  test(`${label} (${variant.hid_pid}): start, frames and shutdown are OpenRGB's I/O`, async () => {
    const { device, plugin, restore } = await setup(variant, variant.colours_a);
    try {
      plugin.Initialize();
      assert.deepEqual(io(device), variant.initialize, "Initialize");
      device.calls.length = 0;
      plugin.Render();
      if (!variant.supported) {
        plugin.Shutdown(false);
        assert.deepEqual(io(device), [], "an unknown device is never driven");
        return;
      }
      assert.deepEqual(io(device), variant.frame_a, "frame with colour set A");
      device.calls.length = 0;
      plugin.Render();
      assert.deepEqual(io(device), [], "an unchanged frame within a second is not resent");
      device.setColor(canvas(variant, variant.colours_b));
      plugin.Render();
      assert.deepEqual(io(device), variant.frame_b, "frame with colour set B");
      device.calls.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(io(device), variant.shutdown, "Shutdown: hardware render mode");
    } finally {
      restore();
    }
  });

  if (variant.supported) {
    test(`${label}: name and LED grid`, async () => {
      const { device, plugin, restore } = await setup(variant, variant.colours_a);
      try {
        plugin.Initialize();
        const call = (m) => device.calls.find((c) => c.method === m)?.args;
        assert.deepEqual(call("setSize"), [variant.size]);
        assert.deepEqual(call("setControllableLeds"), [variant.leds.map((l) => l.name), variant.leds.map((l) => l.pos)]);
        assert.ok(typeof call("setName")[0] === "string");
      } finally {
        restore();
      }
    });
  }
}

test("an unchanged frame is resent after a second as a keepalive", async () => {
  mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const variant = expected.variants.M55_SW;
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

test("ProductId() lists every detector id once and equals the manifest; Validate takes interface 1 / 0xFF42", async () => {
  const device = createMockDevice({ productId: 0x1BA4 });
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    const ids = plugin.ProductId();
    assert.equal(new Set(ids).size, ids.length);
    assert.deepEqual(ids, expected.detectors.map((d) => Number(d.pid)));
    assert.deepEqual(ids, manifest.devices[0].pids.map(Number));
    assert.equal(ids.length, 38);
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    for (const d of expected.detectors) {
      assert.equal(d.interface, 1);
      assert.equal(d.usage_page, "0xFF42");
    }
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF42, usage: 0x0001, collection: 1 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF42, usage: 0x0002, collection: 2 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0xFF43, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF42, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 2, usage_page: 0x0001, usage: 0x0006, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("every LED table is inside its size, with distinct cells and buffer indices in range", () => {
  for (const t of TABLES) {
    const cells = new Set();
    for (const [name, value, x, y] of t.leds) {
      assert.ok(typeof name === "string" && name.length > 0);
      assert.ok(x >= 0 && y >= 0 && x < t.size[0] && y < t.size[1], `${name} inside ${t.size}`);
      assert.ok(value >= 0 && value < t.bufferSize, `${name} value ${value} < ${t.bufferSize}`);
      cells.add(`${x},${y}`);
    }
    assert.equal(cells.size, t.leds.length);
  }
  for (const entry of Object.values(DEVICES)) {
    for (const code of [1, 2, 4]) {
      assert.ok(TABLES[entry.layouts[code]]);
    }
  }
});

test("only lighting commands are written: render mode, transactions, block writes and GETs", async () => {
  for (const [label, variant] of Object.entries(expected.variants)) {
    const { device, plugin, restore } = await setup(variant, variant.colours_a ?? [[1, 2, 3]]);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      for (const op of io(device)) {
        if (op.op === "read" || op.op === "pause") continue;
        assert.equal(op.op, "write", `${label}: only output reports`);
        assert.equal(op.len, 65);
        const b = op.hex.match(/../g).map((h) => parseInt(h, 16));
        assert.equal(b[0], 0x00, "report id 0");
        assert.ok([0x08, 0x09, 0x80].includes(b[1]), `${label}: write_cmd ${b[1]}`);
        const cmd = b[2];
        assert.ok([0x01, 0x02, 0x05, 0x06, 0x07, 0x0D].includes(cmd), `${label}: command ${cmd}`);
        if (cmd === 0x01) {
          // SET is used for one property only: render mode (0x03) = 1 hardware / 2 software.
          assert.equal(b[3], 0x03, `${label}: SET property`);
          assert.ok(b[5] === 0x01 || b[5] === 0x02);
          assert.ok(b.slice(6).every((x) => x === 0) && b[4] === 0);
        }
        if (cmd === 0x06 || cmd === 0x0D || cmd === 0x05) {
          // block writes only through the lighting endpoint opened by START_TX 0 (0x01 / 0x22)
          if (cmd === 0x0D) assert.ok(b[3] === 0 && (b[4] === 0x01 || b[4] === 0x22));
          if (cmd === 0x06) assert.equal(b[3], 0);
        }
      }
    } finally {
      restore();
    }
  }
});
