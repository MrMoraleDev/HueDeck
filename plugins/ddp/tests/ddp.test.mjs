// SPDX-License-Identifier: GPL-3.0-or-later
//
// DDP plugin: Add by IP / Forget and their settings, the announced data, the parameters (LED
// count re-layout, colour order) and the packets (goldens in fixtures/expected.json, worked out
// by hand from the 3waylabs DDP specification). Only the mock net is used.

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

const CONTROLLER = { id: "ddp-192.168.1.40", name: "DDP 192.168.1.40", ip: "192.168.1.40", port: 4048 };
const redGreen = (x) => (x === 0 ? [255, 0, 0] : [0, 255, 0]);

async function serviceSetup(settings) {
  const net = createMockNet();
  const service = createMockService({ settings });
  const restore = installNetGlobals({ net, service });
  const plugin = await loadPlugin(pluginPath);
  const discovery = startService(plugin);
  return { net, service, plugin, discovery, restore };
}

async function deviceSetup({ controller = CONTROLLER, color = [0, 0, 0], params = { ledCount: "2", colorOrder: "RGB" } } = {}) {
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

test("metadata, parameters and no discovery sockets", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.Type(), "network");
  assert.equal(plugin.Documentation(), "hardware/ddp");
  const params = Object.fromEntries(plugin.ControllableParameters().map((p) => [p.property, p]));
  assert.equal(params.ledCount.type, "number");
  assert.equal(params.ledCount.default, "60");
  assert.equal(params.ledCount.min, "1");
  assert.equal(params.ledCount.max, "1500");
  assert.equal(params.colorOrder.type, "combobox");
  assert.ok(params.colorOrder.values.includes("GRB"));
  const d = new plugin.DiscoveryService();
  assert.equal(d.MDns, undefined);
  assert.equal(d.UdpBroadcastPort, undefined);
  const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
  assert.equal(manifest.devices[0].network.mdns, undefined);
  assert.deepEqual(manifest.devices[0].network.udpPorts, [expected.port]);
});

test("Add by IP adds, announces and persists; restart restores; Forget removes", async () => {
  const first = await serviceSetup();
  let saved;
  try {
    first.discovery.forceDiscover("192.168.1.40");
    first.discovery.forceDiscover("192.168.1.40");
    first.discovery.forceDiscover("bogus");
    assert.equal(first.service.controllers.length, 1);
    assert.equal(first.service.announced.length, 1);
    assert.deepEqual(first.service.announced[0], { ...CONTROLLER, status: "UDP 4048" });
    saved = first.service.getSetting("ddp", "controllers");
    assert.deepEqual(JSON.parse(saved), ["192.168.1.40"]);
    assert.equal(first.net.sent.length + first.net.http.length, 0, "no traffic from the service");
  } finally {
    first.restore();
  }
  const second = await serviceSetup({ ddp: { controllers: saved } });
  try {
    assert.equal(second.service.announced.length, 1);
    assert.equal(second.service.announced[0].id, "ddp-192.168.1.40");
    second.discovery.forgetController("ddp-192.168.1.40");
    assert.deepEqual(second.service.suppressed, ["ddp-192.168.1.40"]);
    assert.equal(second.service.controllers.length, 0);
    assert.equal(second.service.getSetting("ddp", "controllers"), undefined);
  } finally {
    second.restore();
  }
});

test("Initialize lays out ledCount LEDs; onledCountChanged re-lays out", async () => {
  const { device, plugin, restore } = await deviceSetup({ params: { ledCount: "3", colorOrder: "RGB" } });
  try {
    plugin.Initialize();
    const layouts = () => device.calls.filter((c) => c.method === "setSize").map((c) => c.args[0]);
    assert.deepEqual(layouts(), [[3, 1]]);
    assert.deepEqual(device.calls.find((c) => c.method === "setControllableLeds").args[1], [[0, 0], [1, 0], [2, 0]]);
    globalThis.ledCount = 5;
    plugin.onledCountChanged();
    assert.deepEqual(layouts(), [[3, 1], [5, 1]]);
    globalThis.ledCount = 99999;
    plugin.onledCountChanged();
    assert.deepEqual(layouts().at(-1), [1500, 1]);
    assert.ok(device.logs.some((l) => l.message.includes("1500")));
  } finally {
    restore();
  }
});

test("Render sends the two-LED golden (RGB), then GRB after the order changes", async () => {
  const { net, plugin, restore } = await deviceSetup({ color: redGreen });
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(net.sent.length, 1);
    assert.equal(net.sent[0].ip, "192.168.1.40");
    assert.equal(net.sent[0].port, expected.port);
    assert.deepEqual(net.sent[0].bytes, hexToBytes(expected.two_leds_rgb.hex));
  } finally {
    restore();
  }
  const grb = await deviceSetup({ color: redGreen, params: { ledCount: 2, colorOrder: "GRB" } });
  try {
    grb.plugin.Initialize();
    grb.plugin.Render();
    assert.deepEqual(grb.net.sent[0].bytes, hexToBytes(expected.two_leds_grb.hex));
    globalThis.colorOrder = "RGB";
    grb.plugin.oncolorOrderChanged();
    grb.plugin.Render();
    assert.equal(grb.net.sent.length, 2, "an order change resends at once");
    assert.deepEqual(grb.net.sent[1].bytes.slice(10), [255, 0, 0, 0, 255, 0]);
  } finally {
    grb.restore();
  }
});

test("500 LEDs: two packets, 1440 data bytes then 60, push only on the last", async () => {
  const pixel = hexToBytes(expected.five_hundred_leds.pixel_hex);
  const { net, plugin, restore } = await deviceSetup({ color: pixel, params: { ledCount: 500, colorOrder: "RGB" } });
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(net.sent.length, 2);
    expected.five_hundred_leds.packets.forEach((p, i) => {
      const b = net.sent[i].bytes;
      assert.equal(b.length, p.length);
      assert.deepEqual(b.slice(0, 10), hexToBytes(p.header_hex));
      for (let k = 10; k < b.length; k += 3) assert.deepEqual(b.slice(k, k + 3), pixel);
    });
  } finally {
    restore();
  }
});

test("sequence numbers run 1..15 and wrap to 1; unchanged frames wait for the 1 s keep-alive", async () => {
  const clock = withClock();
  let n = 0;
  const { net, plugin, restore } = await deviceSetup({ color: () => [n & 0xFF, 0, 0] });
  try {
    plugin.Initialize();
    for (let i = 0; i < 16; i++) {
      n = i + 1;
      plugin.Render();
      clock.advance(33);
    }
    assert.deepEqual(net.sent.map((s) => s.bytes[1]), expected.sequence_wrap.sequence);
    plugin.Render();
    assert.equal(net.sent.length, 16, "unchanged frame is not resent");
    clock.advance(1000);
    plugin.Render();
    assert.equal(net.sent.length, 17, "keep-alive");
    plugin.Shutdown(false);
    assert.equal(net.sent.length, 17, "Shutdown sends nothing");
  } finally {
    restore();
    clock.restore();
  }
});

test("robust with the dry-run stub controller and missing parameters", async () => {
  const { net, device, plugin, restore } = await deviceSetup({
    controller: { id: "x", ip: "127.0.0.1", port: 0, name: "stub", hostname: "stub", mac: "" },
    params: {},
  });
  try {
    plugin.Initialize();
    assert.deepEqual(device.calls.find((c) => c.method === "setSize").args[0], [60, 1], "default 60 LEDs");
    plugin.Render();
    assert.equal(net.sent[0].port, 4048, "port 0 falls back to 4048");
    plugin.Shutdown(true);
  } finally {
    restore();
  }
  const bare = await deviceSetup({ controller: {}, params: { ledCount: "abc", colorOrder: "XYZ" } });
  try {
    bare.plugin.Initialize();
    bare.plugin.Render();
    bare.plugin.Shutdown(false);
    assert.equal(bare.net.sent.length, 0);
    assert.ok(bare.device.logs.length > 0);
  } finally {
    bare.restore();
  }
});
