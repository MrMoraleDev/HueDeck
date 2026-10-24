// SPDX-License-Identifier: GPL-3.0-or-later
//
// Govee plugin: discovery (multicast scan, replies, added IPs, stale lights, forget), the
// announced controller data, and the device session's JSON commands against the hand-written
// goldens in fixtures/expected.json (single colour and OpenRGB's per-segment frames), rate
// limiting, keep-alive, Shutdown restore and robustness.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockNet, createMockService, installNetGlobals, startService } from "../../_testkit/mock-net.mjs";
import { createMockDevice, loadPlugin } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const golden = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));

const realNow = Date.now;
const clock = { t: 1_000_000 };
function useClock() {
  clock.t = 1_000_000;
  Date.now = () => clock.t;
  return () => { Date.now = realNow; };
}

async function serviceSetup(settings) {
  const net = createMockNet();
  const service = createMockService({ settings });
  const restore = installNetGlobals({ net, service });
  const plugin = await loadPlugin(pluginPath);
  const unclock = useClock();
  const discovery = startService(plugin);
  return { net, service, plugin, discovery, restore: () => { unclock(); restore(); } };
}

const CONTROLLER = { id: "1F:80:C5:32:32:36:72:4E", name: "Govee H619A", ip: "192.168.1.23", port: 4003, sku: "H619A" };

async function deviceSetup({ controller = CONTROLLER, params = {}, color = [0, 0, 0] } = {}) {
  const net = createMockNet();
  const device = createMockDevice({ color });
  const restore = installNetGlobals({ net, device, controller, params });
  const plugin = await loadPlugin(pluginPath);
  const unclock = useClock();
  return { net, device, plugin, restore: () => { unclock(); restore(); } };
}

const texts = (net, ip = "192.168.1.23", port = 4003) => net.sentTo(ip, port).map((s) => s.text);

test("metadata: a network lighting controller with the Govee multicast ports", async () => {
  const net = createMockNet();
  const restore = installNetGlobals({ net });
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.equal(plugin.Type(), "network");
    assert.equal(plugin.DeviceType(), "lightingcontroller");
    assert.equal(plugin.Documentation(), "hardware/govee");
    const d = new plugin.DiscoveryService();
    assert.equal(d.UdpBroadcastAddress, "239.255.255.250");
    assert.equal(d.UdpBroadcastPort, 4001);
    assert.equal(d.UdpListenPort, 4002);
    assert.equal(net.sockets.length, 0, "the constructor opens no socket");
    const params = plugin.ControllableParameters();
    assert.equal(params.find((p) => p.property === "segmentMode").default, "false");
  } finally {
    restore();
  }
});

test("Initialize scans; a scan reply becomes an announced controller", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    assert.deepEqual(service.broadcasts, [golden.scan]);
    discovery.Discovered({ id: "192.168.1.23", ip: "192.168.1.23", port: 4002, response: golden.scan_reply });
    assert.equal(service.announced.length, 1);
    const a = service.announced[0];
    assert.equal(a.id, "1F:80:C5:32:32:36:72:4E");
    assert.equal(a.ip, "192.168.1.23");
    assert.equal(a.port, 4003);
    assert.equal(a.sku, "H619A");
    assert.equal(a.name, "Govee H619A");
    assert.equal(a.status, "online");
    // Seen again with the same data: no new announce (that would restart the session).
    discovery.Discovered({ ip: "192.168.1.23", response: golden.scan_reply });
    assert.equal(service.announced.length, 1);
    // Our own scan looped back and devStatus replies are ignored.
    discovery.Discovered({ ip: "192.168.1.2", response: golden.scan });
    discovery.Discovered({ ip: "192.168.1.23", response: golden.status_reply });
    discovery.Discovered({ ip: "192.168.1.9", response: "not json" });
    assert.equal(service.controllers.length, 1);
  } finally {
    restore();
  }
});

test("a new IP for the same light is announced again", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.Discovered({ ip: "192.168.1.23", response: golden.scan_reply });
    discovery.Discovered({ ip: "192.168.1.24", response: golden.scan_reply.replace("192.168.1.23", "192.168.1.24") });
    assert.equal(service.announced.length, 2);
    assert.equal(service.announced[1].ip, "192.168.1.24");
  } finally {
    restore();
  }
});

test("scans repeat every 60 s; a light silent for three rounds is suppressed", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.Discovered({ ip: "192.168.1.23", response: golden.scan_reply });
    clock.t += 30_000;
    discovery.Update();
    assert.equal(service.broadcasts.length, 1);
    clock.t += 30_000;
    discovery.Update();
    assert.equal(service.broadcasts.length, 2);
    for (let i = 0; i < 2; i++) {
      clock.t += 60_000;
      discovery.Update();
    }
    assert.deepEqual(service.suppressed, []);
    clock.t += 10_000;
    discovery.Update();
    assert.deepEqual(service.suppressed, ["1F:80:C5:32:32:36:72:4E"]);
    assert.equal(service.controllers.length, 0);
  } finally {
    restore();
  }
});

test("forceDiscover unicasts the scan, persists the IP, and the reply on the socket announces", async () => {
  const { net, service, discovery, restore } = await serviceSetup();
  try {
    discovery.forceDiscover("192.168.1.23");
    const sent = net.sentTo("192.168.1.23", 4001);
    assert.equal(sent.length, 1);
    assert.equal(sent[0].text, golden.scan);
    assert.equal(service.getSetting("forced", "ips"), JSON.stringify(["192.168.1.23"]));
    assert.equal(service.controllers[0].obj.status.startsWith("waiting"), true);
    assert.equal(service.announced.length, 0, "a pending IP is not announced");
    net.deliver(sent[0].socket, { data: golden.scan_reply, address: "192.168.1.23", port: 4002 });
    assert.equal(service.announced.length, 1);
    assert.equal(service.announced[0].forced, true);
    assert.equal(service.controllers.length, 1, "the placeholder is replaced");
    // Every round scans the added IP again.
    clock.t += 60_000;
    discovery.Update();
    assert.equal(net.sentTo("192.168.1.23", 4001).length, 2);
  } finally {
    restore();
  }
});

test("added IPs come back after a restart; forget removes them", async () => {
  const { net, service, discovery, restore } = await serviceSetup({ forced: { ips: JSON.stringify(["192.168.1.40"]) } });
  try {
    assert.equal(net.sentTo("192.168.1.40", 4001).length, 1);
    assert.equal(service.controllers.length, 1);
    discovery.forgetController(service.controllers[0].id);
    assert.equal(service.controllers.length, 0);
    assert.equal(service.getSetting("forced", "ips"), "[]");
    discovery.Discovered({ ip: "192.168.1.23", response: golden.scan_reply });
    discovery.forgetController("1F:80:C5:32:32:36:72:4E");
    assert.deepEqual(service.suppressed, ["1F:80:C5:32:32:36:72:4E"]);
  } finally {
    restore();
  }
});

test("Initialize asks the status, turns on, full brightness; a frame is one colorwc", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    assert.deepEqual(texts(net), [golden.dev_status, golden.turn_on, golden.brightness_100]);
    assert.ok(device.calls.some((c) => c.method === "setSize" && c.args[0][0] === 1 && c.args[0][1] === 1));
    plugin.Render();
    assert.equal(texts(net).at(-1), golden.color_red);
    device.setColor([0x12, 0x34, 0x56]);
    clock.t += 100;
    plugin.Render();
    assert.equal(texts(net).at(-1), golden.color_123456);
  } finally {
    restore();
  }
});

test("rate limit: at most one command per 100 ms, only on change, keep-alive every 30 s", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    const base = texts(net).length;
    plugin.Render();
    assert.equal(texts(net).length, base + 1);
    device.setColor([0, 255, 0]);
    clock.t += 33;
    plugin.Render();
    clock.t += 33;
    plugin.Render();
    assert.equal(texts(net).length, base + 1, "within 100 ms nothing more is sent");
    clock.t += 34;
    plugin.Render();
    assert.equal(texts(net).length, base + 2, "the newest colour goes out once 100 ms have passed");
    for (let i = 0; i < 20; i++) {
      clock.t += 100;
      plugin.Render();
    }
    assert.equal(texts(net).length, base + 2, "an unchanged colour is not re-sent");
    clock.t += 30_000;
    plugin.Render();
    assert.equal(texts(net).length, base + 3, "keep-alive");
    // In one simulated second of 33 ms frames with a colour change each frame: at most 10 sends.
    const before = texts(net).length;
    for (let i = 0; i < 30; i++) {
      device.setColor([i, 0, 0]);
      clock.t += 33;
      plugin.Render();
    }
    assert.ok(texts(net).length - before <= 10, `${texts(net).length - before} sends in ~1 s`);
  } finally {
    restore();
  }
});

test("Shutdown restores colour, brightness and power from the devStatus reply", async () => {
  const { net, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    net.deliver(net.sockets[0], { data: golden.status_reply, address: "192.168.1.23", port: 4003 });
    plugin.Render();
    const before = texts(net).length;
    plugin.Shutdown(false);
    assert.deepEqual(texts(net).slice(before), [golden.color_restore_0a141e, golden.brightness_40, golden.turn_off]);
    assert.equal(net.sockets[0].closed, true);
  } finally {
    restore();
  }
});

test("Shutdown restores a white temperature; without a reply the last colour stays", async () => {
  {
    const { net, plugin, restore } = await deviceSetup();
    try {
      plugin.Initialize();
      const reply = '{"msg":{"cmd":"devStatus","data":{"onOff":1,"brightness":100,"color":{"r":255,"g":255,"b":255},"colorTemInKelvin":2700}}}';
      net.deliver(net.sockets[0], { data: reply });
      const before = texts(net).length;
      plugin.Shutdown(true);
      assert.deepEqual(texts(net).slice(before), [golden.white_restore_2700, golden.brightness_100, golden.turn_on]);
    } finally {
      restore();
    }
  }
  {
    const { net, device, plugin, restore } = await deviceSetup();
    try {
      plugin.Initialize();
      const before = texts(net).length;
      plugin.Shutdown(false);
      assert.equal(texts(net).length, before);
      assert.ok(device.logs.some((l) => /keeps the last colour/.test(l.message)));
    } finally {
      restore();
    }
  }
});

test("segment mode: OpenRGB razer enable, frames and disable", async () => {
  const { net, device, plugin, restore } = await deviceSetup({
    params: { segmentMode: true, segmentCount: 2 },
    color: (x) => (x === 0 ? [255, 0, 0] : [0, 0, 255]),
  });
  try {
    plugin.Initialize();
    assert.deepEqual(texts(net), [golden.dev_status, golden.turn_on, golden.brightness_100, golden.razer_enable]);
    const leds = device.calls.find((c) => c.method === "setControllableLeds").args;
    assert.deepEqual(leds, [["Segment 1", "Segment 2"], [[0, 0], [1, 0]]]);
    plugin.Render();
    assert.equal(texts(net).at(-1), golden.razer_red_blue);
    // Keep-alive re-asserts the session first.
    clock.t += 30_000;
    plugin.Render();
    assert.deepEqual(texts(net).slice(-2), [golden.razer_enable, golden.razer_red_blue]);
    plugin.Shutdown(false);
    assert.equal(texts(net).at(-1), golden.razer_disable);
  } finally {
    restore();
  }
});

test("turning segment mode off at runtime sends the razer disable and relayouts", async () => {
  const params = { segmentMode: "true", segmentCount: "3" };
  const { net, device, plugin, restore } = await deviceSetup({ params });
  try {
    plugin.Initialize();
    globalThis.segmentMode = false;
    plugin.onsegmentModeChanged();
    assert.equal(texts(net).at(-1), golden.razer_disable);
    const sizes = device.calls.filter((c) => c.method === "setSize").map((c) => c.args[0]);
    assert.deepEqual(sizes, [[3, 1], [1, 1]]);
  } finally {
    restore();
  }
});

test("robustness: empty controller, dry-run stub and failing sockets never throw", async () => {
  for (const controller of [{}, undefined, { id: "x", ip: "127.0.0.1", port: 0, name: "stub", hostname: "", mac: "" }]) {
    const { plugin, restore } = await deviceSetup({ controller });
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(false);
    } finally {
      restore();
    }
  }
  const { net, device, plugin, restore } = await deviceSetup();
  try {
    plugin.Initialize();
    net.sockets[0].close();
    clock.t += 1000;
    device.setColor([1, 2, 3]);
    plugin.Render();
    plugin.Shutdown(false);
    assert.ok(device.logs.some((l) => /failed/.test(l.message)));
  } finally {
    restore();
  }
});
