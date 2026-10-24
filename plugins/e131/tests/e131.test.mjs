// SPDX-License-Identifier: GPL-3.0-or-later
//
// E1.31 plugin: Add by IP / Forget with a stable CID in the settings, the parameters (universe,
// start channel, LED count, colour order, priority), the universe split, the data packets
// (goldens in fixtures/expected.json, worked out by hand from ANSI E1.31-2018), per-universe
// sequence numbers, keep-alive and the Stream_Terminated shutdown. Only the mock net is used.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockNet, createMockService, installNetGlobals, startService } from "../../_testkit/mock-net.mjs";
import { createMockDevice, hexToBytes, loadPlugin } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const GOLDEN = hexToBytes(expected.two_leds_universe_1.parts.join(" "));

const CONTROLLER = { id: "e131-192.168.1.30", name: "E1.31 192.168.1.30", ip: "192.168.1.30", port: 5568, cid: expected.cid_hex };
const PARAMS = { universe: "1", startChannel: "1", ledCount: "2", colorOrder: "RGB", priority: "100" };
const redGreen = (x) => (x === 0 ? [255, 0, 0] : [0, 255, 0]);

async function serviceSetup(settings) {
  const net = createMockNet();
  const service = createMockService({ settings });
  const restore = installNetGlobals({ net, service });
  const plugin = await loadPlugin(pluginPath);
  const discovery = startService(plugin);
  return { net, service, plugin, discovery, restore };
}

async function deviceSetup({ controller = CONTROLLER, color = [0, 0, 0], params = PARAMS } = {}) {
  const net = createMockNet();
  const device = createMockDevice({ color });
  const restore = installNetGlobals({ net, device, controller, params });
  const plugin = await loadPlugin(pluginPath);
  return { net, device, plugin, restore };
}

function withClock(start = 1_000_000) {
  const real = Date.now;
  let now = start;
  Date.now = () => now;
  return { advance(ms) { now += ms; }, restore() { Date.now = real; } };
}

const u16 = (b, at) => (b[at] << 8) | b[at + 1];

test("the hand-made golden is self-consistent (126-byte header + 6 slots)", () => {
  assert.equal(GOLDEN.length, 132);
  assert.equal(u16(GOLDEN, 16) & 0x0FFF, 132 - 16);
  assert.equal(u16(GOLDEN, 38) & 0x0FFF, 132 - 38);
  assert.equal(u16(GOLDEN, 115) & 0x0FFF, 132 - 115);
});

test("metadata, parameters and no discovery sockets", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.Type(), "network");
  assert.equal(plugin.Documentation(), "hardware/e131");
  const params = Object.fromEntries(plugin.ControllableParameters().map((p) => [p.property, p]));
  assert.deepEqual(Object.keys(params), ["universe", "startChannel", "ledCount", "colorOrder", "priority"]);
  assert.equal(params.universe.max, "63999");
  assert.equal(params.startChannel.max, "512");
  assert.equal(params.ledCount.default, "170");
  assert.equal(params.priority.default, "100");
  const d = new plugin.DiscoveryService();
  assert.equal(d.MDns, undefined);
  const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
  assert.deepEqual(manifest.devices[0].network.udpPorts, [expected.port]);
});

test("Add by IP generates a version-4 CID once, persists it, and restores it after a restart", async () => {
  const first = await serviceSetup();
  let saved;
  try {
    first.discovery.forceDiscover("192.168.1.30");
    first.discovery.forceDiscover("192.168.1.30");
    first.discovery.forceDiscover("300.1.1.1");
    assert.equal(first.service.controllers.length, 1);
    const a = first.service.announced[0];
    assert.equal(a.id, "e131-192.168.1.30");
    assert.equal(a.ip, "192.168.1.30");
    assert.equal(a.port, 5568);
    assert.match(a.cid, /^[0-9a-f]{32}$/);
    assert.equal(a.cid[12], "4", "UUID version 4");
    assert.ok("89ab".includes(a.cid[16]), "RFC 4122 variant");
    saved = first.service.getSetting("e131", "controllers");
    assert.deepEqual(JSON.parse(saved), [{ ip: "192.168.1.30", cid: a.cid }]);
    assert.equal(first.net.sent.length + first.net.http.length, 0, "no traffic from the service");
  } finally {
    first.restore();
  }
  const second = await serviceSetup({ e131: { controllers: saved } });
  try {
    assert.equal(second.service.announced[0].cid, JSON.parse(saved)[0].cid, "same CID after restart");
    second.discovery.forgetController("e131-192.168.1.30");
    assert.deepEqual(second.service.suppressed, ["e131-192.168.1.30"]);
    assert.equal(second.service.controllers.length, 0);
    assert.equal(second.service.getSetting("e131", "controllers"), undefined);
  } finally {
    second.restore();
  }
});

test("universe layout: 170 LEDs per universe, no LED split, first universe from the start channel", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.deepEqual(plugin.universeLayout(1, 1, 2), [{ universe: 1, firstLed: 0, leds: 2, firstSlot: 1 }]);
  assert.deepEqual(plugin.universeLayout(1, 1, 171), [
    { universe: 1, firstLed: 0, leds: 170, firstSlot: 1 },
    { universe: 2, firstLed: 170, leds: 1, firstSlot: 1 },
  ]);
  assert.deepEqual(plugin.universeLayout(1, 4, 172), [
    { universe: 1, firstLed: 0, leds: 169, firstSlot: 4 },
    { universe: 2, firstLed: 169, leds: 3, firstSlot: 1 },
  ]);
  assert.deepEqual(plugin.universeLayout(5, 511, 1), [{ universe: 6, firstLed: 0, leds: 1, firstSlot: 1 }], "no room in universe 5");
  assert.deepEqual(plugin.universeLayout(63999, 1, 200), [{ universe: 63999, firstLed: 0, leds: 170, firstSlot: 1 }]);
});

test("Render sends the two-LED golden on universe 1; Shutdown sends three Stream_Terminated packets", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: redGreen });
  try {
    plugin.Initialize();
    assert.deepEqual(device.calls.find((c) => c.method === "setSize").args[0], [2, 1]);
    plugin.Render();
    assert.equal(net.sent.length, 1);
    assert.equal(net.sent[0].ip, "192.168.1.30");
    assert.equal(net.sent[0].port, expected.port);
    assert.deepEqual(net.sent[0].bytes, GOLDEN);
    plugin.Shutdown(false);
    const term = net.sent.slice(1);
    assert.equal(term.length, 3);
    term.forEach((s, i) => {
      const want = GOLDEN.slice();
      want[111] = expected.two_leds_terminated.sequences[i];
      want[112] = expected.two_leds_terminated.options;
      assert.deepEqual(s.bytes, want);
    });
  } finally {
    restore();
  }
});

test("172 LEDs from channel 4 split over universes 1 and 2 as worked out by hand", async () => {
  const pixel = hexToBytes(expected.split_172_from_channel_4.pixel_hex);
  const { net, plugin, restore } = await deviceSetup({ color: pixel, params: { ...PARAMS, startChannel: 4, ledCount: 172 } });
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(net.sent.length, 2);
    expected.split_172_from_channel_4.packets.forEach((p, i) => {
      const b = net.sent[i].bytes;
      assert.equal(b.length, p.length);
      assert.deepEqual(b.slice(16, 18), hexToBytes(p.root_fl));
      assert.deepEqual(b.slice(38, 40), hexToBytes(p.framing_fl));
      assert.deepEqual(b.slice(115, 117), hexToBytes(p.dmp_fl));
      assert.deepEqual(b.slice(123, 125), hexToBytes(p.count));
      assert.equal(u16(b, 113), p.universe);
      assert.equal(b[111], 0, "each universe has its own sequence, starting at 0");
      const data = b.slice(126);
      assert.deepEqual(data.slice(0, p.leading_zero_slots), new Array(p.leading_zero_slots).fill(0));
      for (let k = 0; k < p.leds; k++) {
        const at = p.leading_zero_slots + k * 3;
        assert.deepEqual(data.slice(at, at + 3), pixel);
      }
    });
  } finally {
    restore();
  }
});

test("sequence numbers count per universe and wrap at 255; unchanged frames wait for the 1 s keep-alive", async () => {
  const clock = withClock();
  let n = 0;
  const { net, plugin, restore } = await deviceSetup({ color: () => [n & 0xFF, 0, 0] });
  try {
    plugin.Initialize();
    for (let i = 0; i < 257; i++) {
      n = i;
      plugin.Render();
      clock.advance(33);
    }
    assert.equal(net.sent.length, 257);
    assert.deepEqual(net.sent.slice(254).map((s) => s.bytes[111]), [254, 255, 0]);
    plugin.Render();
    assert.equal(net.sent.length, 257, "unchanged frame is not resent");
    clock.advance(1000);
    plugin.Render();
    assert.equal(net.sent.length, 258, "keep-alive");
  } finally {
    restore();
    clock.restore();
  }
});

test("parameter changes: colour order, priority, and a universe move terminates the old universe", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: redGreen });
  try {
    plugin.Initialize();
    plugin.Render();
    globalThis.colorOrder = "GRB";
    plugin.oncolorOrderChanged();
    globalThis.priority = "150";
    plugin.onpriorityChanged();
    plugin.Render();
    assert.equal(net.sent.length, 2);
    assert.deepEqual(net.sent[1].bytes.slice(126), [0, 255, 0, 255, 0, 0]);
    assert.equal(net.sent[1].bytes[108], 150);
    globalThis.universe = "7";
    plugin.onuniverseChanged();
    const term = net.sent.slice(2);
    assert.equal(term.length, 3, "three Stream_Terminated packets for universe 1");
    for (const s of term) {
      assert.equal(u16(s.bytes, 113), 1);
      assert.equal(s.bytes[112], 0x40);
    }
    plugin.Render();
    assert.equal(u16(net.sent.at(-1).bytes, 113), 7);
    globalThis.ledCount = "400";
    plugin.onledCountChanged();
    assert.deepEqual(device.calls.filter((c) => c.method === "setSize").at(-1).args[0], [400, 1]);
    plugin.Render();
    assert.deepEqual(net.sent.slice(-3).map((s) => u16(s.bytes, 113)), [7, 8, 9]);
    plugin.Shutdown(true);
    const last = net.sent.slice(-9);
    assert.deepEqual(last.map((s) => u16(s.bytes, 113)), [7, 7, 7, 8, 8, 8, 9, 9, 9]);
    assert.ok(last.every((s) => s.bytes[112] === 0x40));
  } finally {
    restore();
  }
});

test("robust with the dry-run stub controller and bad parameters", async () => {
  const { net, device, plugin, restore } = await deviceSetup({
    controller: { id: "x", ip: "127.0.0.1", port: 0, name: "stub", hostname: "stub", mac: "" },
    params: {},
  });
  try {
    plugin.Initialize();
    assert.deepEqual(device.calls.find((c) => c.method === "setSize").args[0], [170, 1], "default 170 LEDs");
    plugin.Render();
    assert.equal(net.sent.length, 1);
    assert.equal(net.sent[0].port, 5568);
    assert.ok(device.logs.some((l) => l.message.includes("CID")));
    plugin.Shutdown(false);
  } finally {
    restore();
  }
  const bare = await deviceSetup({ controller: {}, params: { universe: "0", startChannel: "999", ledCount: "x", priority: "-5", colorOrder: 3 } });
  try {
    bare.plugin.Initialize();
    bare.plugin.Render();
    bare.plugin.Shutdown(false);
    assert.equal(bare.net.sent.length, 0);
  } finally {
    bare.restore();
  }
});
