// SPDX-License-Identifier: GPL-3.0-or-later
//
// MSI GPU plugin, experimental per-LED mode (docs/research/msi-gpu-perled.md): off by default;
// when the "perLed" parameter is on, Initialize reads 0xEC/0xED, enters with 0x2E <- 1, Render
// sends one raw 0x49 frame of 34 [B, G, R, 0] records, Shutdown leaves with 0x2E <- 0 before the
// usual restore. Until Shutdown nothing but 0x2E and 0x49 is written; 0x3F never.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, loadPlugin } from "../../_testkit/mock-device.mjs";
import { createMockBus, installGlobals } from "../../_testkit/mock-bus.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const startup = Object.fromEntries(Object.entries(expected.startup_registers).map(([r, v]) => [Number(r), Number(v)]));
const RTX3060 = { vendor: 0x10DE, device: 0x2504, subVendor: 0x1462, subDevice: 0x3903 };
// A v1 controller: LED type 0x01, module type 0x0102, firmware 0x03; chip id 0x0405.
const V1 = { 0xEC: 0x01, 0xED: 0x01, 0xEE: 0x02, 0xEF: 0x03 };

async function setup({ perLed = true, ident = V1, color = (x) => [x, 0x10, 0x20] } = {}) {
  const device = createMockDevice({ color });
  const registers = { ...startup, ...ident };
  // 0xED is read as two bytes: chip id high, low.
  const mock = createMockBus({ kind: "nvidia", pci: RTX3060, name: "NVIDIA GeForce RTX 3060", address: 0x68, registers: { 0x68: registers } });
  const params = perLed === "unset" ? {} : { perLed };
  const restore = installGlobals(device, mock.fixed, params);
  // loadPlugin() imports a fresh module each time (the plugin keeps state at module level).
  const plugin = await loadPlugin(pluginPath);
  return { device, mock, plugin, restore };
}

const writeRegs = (mock) => mock.writes().map((w) => w.reg);

test("per-LED is a boolean parameter, off by default", async () => {
  const { plugin, restore } = await setup({ perLed: "unset" });
  try {
    const p = plugin.ControllableParameters().find((x) => x.property === "perLed");
    assert.equal(p.label, "Per-LED (experimental)");
    assert.equal(p.type, "boolean");
    assert.equal(p.default, "false");
  } finally {
    restore();
  }
});

test("off (no parameter, or false): the one-colour path, never 0x2E or 0x49", async () => {
  for (const perLed of ["unset", false, "false"]) {
    const { mock, plugin, restore } = await setup({ perLed });
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(false);
      assert.ok(!writeRegs(mock).some((r) => r === 0x2E || r === 0x49), `perLed=${perLed}`);
      assert.ok(!mock.ops.some((o) => o.reg === 0xEC || o.reg === 0xED), "no identification reads");
    } finally {
      restore();
    }
  }
});

test("on: identify, enter, 34-slot B,G,R,0 frames; only 0x2E and 0x49 written", async () => {
  const { mock, plugin, device, restore } = await setup({ perLed: "true" });
  try {
    plugin.Initialize();
    const reads = mock.ops.filter((o) => o.op === "ReadBlockBytes").map((o) => [o.reg, o.data[0]]);
    assert.deepEqual(reads, [[0xEC, 4], [0xED, 2]]);
    assert.ok(device.calls.some((c) => c.method === "setSize" && c.args[0][0] === 34 && c.args[0][1] === 1));
    const leds = device.calls.find((c) => c.method === "setControllableLeds");
    assert.equal(leds.args[0].length, 34);
    assert.deepEqual(leds.args[1][33], [33, 0]);
    plugin.Render();
    const w = mock.writes();
    assert.deepEqual(w.map((x) => [x.op, x.reg]), [["WriteByte", 0x2E], ["WriteBlockWithoutRegister", 0x49]]);
    assert.deepEqual(w[0].data, [0x01]);
    const frame = w[1].data;
    assert.equal(frame.length, 1 + 34 * 4);
    for (let i = 0; i < 34; i++) {
      assert.deepEqual(frame.slice(1 + 4 * i, 5 + 4 * i), [0x20, 0x10, i, 0x00], `slot ${i}`);
    }
    // Unchanged frames are not sent again (until the resend interval).
    plugin.Render();
    assert.equal(mock.writes().length, 2);
    assert.ok(mock.writes().every((x) => x.reg === 0x2E || x.reg === 0x49));
  } finally {
    restore();
  }
});

test("shutdown leaves the stream first, then restores; 0x3F never written", async () => {
  for (const mode of [undefined, "restore", "keep", "off"]) {
    const { mock, plugin, restore } = await setup();
    try {
      plugin.Initialize();
      plugin.Render();
      const before = mock.writes().length;
      plugin.Shutdown(false, mode);
      const after = mock.writes().slice(before);
      assert.deepEqual([after[0].reg, after[0].data], [0x2E, [0x00]], `mode ${mode}`);
      assert.ok(!mock.writes().some((x) => x.reg === 0x3F), `mode ${mode}`);
      assert.ok(!mock.writes().slice(0, before).some((x) => x.reg === 0x22), "no mode write before Shutdown");
    } finally {
      restore();
    }
  }
});

test("an NV5000 controller, or failed identification reads, stays in one-colour mode", async () => {
  for (const ident of [{ 0xEC: 0x01, 0xED: 0x50, 0xEE: 0x00, 0xEF: 0x01 }, { 0xEC: 0x81, 0xED: 0x00, 0xEE: 0x00, 0xEF: 0x01 }]) {
    const { mock, plugin, restore } = await setup({ ident });
    try {
      plugin.Initialize();
      plugin.Render();
      assert.ok(!writeRegs(mock).some((r) => r === 0x2E || r === 0x49));
      assert.ok(writeRegs(mock).includes(0x22), "Direct (static) as usual");
    } finally {
      restore();
    }
  }
  const { mock, plugin, restore } = await setup();
  try {
    mock.fixed.ReadBlockBytes = () => [-1, []];
    plugin.Initialize();
    plugin.Render();
    assert.ok(!writeRegs(mock).some((r) => r === 0x2E || r === 0x49));
  } finally {
    restore();
  }
});

test("hardware modes and switching the parameter off leave the stream first", async () => {
  const { mock, plugin, restore } = await setup();
  try {
    plugin.Initialize();
    plugin.Render();
    let n = mock.writes().length;
    plugin.SetHardwareMode({ id: "rainbow", brightness: 100, speed: 50 });
    assert.deepEqual(mock.writes()[n].reg, 0x2E);
    assert.deepEqual(mock.writes()[n].data, [0x00]);
    n = mock.writes().length;
    plugin.SetHardwareMode(null);
    assert.deepEqual(mock.writes().slice(n).map((x) => [x.reg, x.data[0]]), [[0x2E, 0x01]], "back to the stream, no 0x22");
    globalThis.perLed = false; // installGlobals() put it there; restore() puts it back
    n = mock.writes().length;
    plugin.onperLedChanged();
    assert.deepEqual(mock.writes().slice(n).map((x) => [x.reg, x.data[0]]), [[0x2E, 0x00], [0x36, 0x64], [0x22, 0x13]]);
    assert.ok(!mock.writes().some((x) => x.reg === 0x3F));
  } finally {
    restore();
  }
});
