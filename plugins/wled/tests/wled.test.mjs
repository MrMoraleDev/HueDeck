// SPDX-License-Identifier: GPL-3.0-or-later
//
// WLED plugin: discovery (mDNS, Add by IP, /json/info check), settings, the announced data, and
// the UDP realtime datagrams (DRGB / DNRGB goldens from fixtures/expected.json, worked out by
// hand from kno.wled.ge), keep-alive and shutdown. Only the mock net is used.

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
const INFO = (() => { const { $comment, ...info } = expected.info; void $comment; return info; })();

async function serviceSetup({ settings, routes = {} } = {}) {
  const net = createMockNet();
  for (const [url, reply] of Object.entries(routes)) net.route("GET", url, reply);
  const service = createMockService({ settings });
  const restore = installNetGlobals({ net, service });
  const plugin = await loadPlugin(pluginPath);
  const discovery = startService(plugin);
  return { net, service, plugin, discovery, restore };
}

async function deviceSetup(controller, color = [0, 0, 0]) {
  const net = createMockNet();
  const device = createMockDevice({ color });
  const restore = installNetGlobals({ net, device, controller });
  const plugin = await loadPlugin(pluginPath);
  return { net, device, plugin, restore };
}

function withClock(start = 1_000_000) {
  const real = Date.now;
  let now = start;
  Date.now = () => now;
  return { advance(ms) { now += ms; }, restore() { Date.now = real; } };
}

const CONTROLLER = { id: "aabbccddeeff", ip: "192.168.1.50", name: "Desk strip", ledCount: 2, udpPort: 21324 };

test("metadata and module load without host globals", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.Name(), "WLED");
  assert.equal(plugin.Type(), "network");
  assert.equal(plugin.DeviceType(), "lightingcontroller");
  assert.equal(plugin.Documentation(), "hardware/wled");
  assert.deepEqual(plugin.ControllableParameters(), []);
  const d = new plugin.DiscoveryService();
  assert.deepEqual(d.MDns, ["_wled._tcp.local."]);
  assert.equal(d.IconUrl, "");
  const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
  assert.deepEqual(manifest.devices[0].network.mdns, d.MDns.map((t) => t.replace(/\.local\.$/, "")));
});

test("an mDNS answer is checked with /json/info, then added and announced", async () => {
  const { net, service, discovery, restore } = await serviceSetup({ routes: { "http://192.168.1.50/json/info": { body: INFO } } });
  try {
    discovery.Discovered({ id: "wled-desk", name: "wled-desk", type: "_wled._tcp", hostname: "wled-desk.local.", port: 80, ip: "192.168.1.50" });
    assert.equal(net.http.length, 1);
    assert.equal(net.http[0].async, true, "discovery uses asynchronous XHR");
    assert.equal(service.announced.length, 0, "nothing before the answer");
    net.flush();
    assert.equal(service.announced.length, 1);
    const a = service.announced[0];
    assert.equal(a.id, "aabbccddeeff");
    assert.equal(a.ip, "192.168.1.50");
    assert.equal(a.name, "Desk strip");
    assert.equal(a.ledCount, 2);
    assert.equal(a.udpPort, 21324);
    assert.equal(a.version, "0.14.4");
    assert.equal(service.controllers.length, 1);
    assert.match(service.controllers[0].obj.status, /2 LEDs/);
    // A repeated answer neither probes again nor restarts the device session.
    discovery.Discovered({ id: "wled-desk", ip: "192.168.1.50" });
    net.flush();
    assert.equal(net.http.length, 1);
    assert.equal(service.announced.length, 1);
  } finally {
    restore();
  }
});

test("a device that does not answer like WLED is not added", async () => {
  const { net, service, discovery, restore } = await serviceSetup({
    routes: { "http://192.168.1.60/json/info": { body: { hello: "world" } } },
  });
  try {
    discovery.Discovered({ id: "x", ip: "192.168.1.60" });
    discovery.Discovered({ id: "y", ip: "192.168.1.61" }); // unreachable
    net.flush();
    assert.equal(service.announced.length, 0);
    assert.equal(service.controllers.length, 0);
    assert.ok(service.logs.some((l) => l.includes("192.168.1.60")));
  } finally {
    restore();
  }
});

test("no MAC: the IP is the id; udpport defaults to 21324; a WLED-compatible brand is accepted", async () => {
  const info = { ver: "0.15.0", leds: { count: 30 }, brand: "Athom" };
  const { net, service, discovery, restore } = await serviceSetup({ routes: { "http://192.168.1.70/json/info": { body: info } } });
  try {
    discovery.Discovered({ id: "a", ip: "192.168.1.70" });
    net.flush();
    assert.equal(service.announced[0].id, "192.168.1.70");
    assert.equal(service.announced[0].udpPort, 21324);
    assert.equal(service.announced[0].name, "WLED 192.168.1.70");
    assert.ok(service.logs.some((l) => l.includes("Athom")));
  } finally {
    restore();
  }
});

test("the LED count is capped at 1500 with a log line", async () => {
  const info = { ...INFO, leds: { count: 4000 } };
  const { net, service, discovery, restore } = await serviceSetup({ routes: { "http://192.168.1.50/json/info": { body: info } } });
  try {
    discovery.Discovered({ ip: "192.168.1.50" });
    net.flush();
    assert.equal(service.announced[0].ledCount, 1500);
    assert.ok(service.logs.some((l) => l.includes("4000") && l.includes("1500")));
  } finally {
    restore();
  }
});

test("Add by IP is persisted, probed again on restart, and Forget removes it", async () => {
  const routes = { "http://10.0.0.5/json/info": { body: INFO } };
  const first = await serviceSetup({ routes });
  let saved;
  try {
    first.discovery.forceDiscover("10.0.0.5");
    first.discovery.forceDiscover("not-an-ip");
    saved = first.service.getSetting("wled", "forced");
    assert.deepEqual(JSON.parse(saved), ["10.0.0.5"]);
    first.net.flush();
    assert.equal(first.service.announced.length, 1);
    assert.equal(first.service.announced[0].ip, "10.0.0.5");
  } finally {
    first.restore();
  }
  const second = await serviceSetup({ routes, settings: { wled: { forced: saved } } });
  try {
    assert.equal(second.net.http[0].url, "http://10.0.0.5/json/info", "Initialize probes the saved IPs");
    second.net.flush();
    assert.equal(second.service.announced.length, 1);
    second.discovery.forgetController("aabbccddeeff");
    assert.equal(second.service.getSetting("wled", "forced"), undefined);
    assert.deepEqual(second.service.suppressed, ["aabbccddeeff"]);
    assert.equal(second.service.controllers.length, 0);
  } finally {
    second.restore();
  }
});

test("a forced IP that does not answer shows as not answering and is retried after 30 s", async () => {
  const clock = withClock();
  const { net, service, discovery, restore } = await serviceSetup({ settings: { wled: { forced: '["10.0.0.9"]' } } });
  try {
    net.flush();
    assert.equal(service.controllers.length, 1);
    assert.equal(service.controllers[0].obj.status, "not answering");
    discovery.Update();
    assert.equal(net.http.length, 1, "no retry before 30 s");
    clock.advance(30_000);
    net.route("GET", "http://10.0.0.9/json/info", { body: INFO });
    discovery.Update();
    net.flush();
    assert.equal(net.http.length, 2);
    assert.equal(service.announced.length, 1);
    assert.deepEqual(service.controllers.map((c) => c.id), ["aabbccddeeff"], "the placeholder row is replaced");
  } finally {
    restore();
    clock.restore();
  }
});

test("a new address for the same MAC re-announces; mDNS removal suppresses", async () => {
  const { net, service, discovery, restore } = await serviceSetup({
    routes: { "http://192.168.1.50/json/info": { body: INFO }, "http://192.168.1.51/json/info": { body: INFO } },
  });
  try {
    discovery.Discovered({ ip: "192.168.1.50" });
    net.flush();
    discovery.Discovered({ ip: "192.168.1.51" });
    net.flush();
    assert.equal(service.announced.length, 2);
    assert.equal(service.announced[1].ip, "192.168.1.51");
    assert.equal(service.controllers.length, 1);
    discovery.Removal({ ip: "192.168.1.51" });
    assert.deepEqual(service.suppressed, ["aabbccddeeff"]);
    assert.equal(service.controllers.length, 0);
  } finally {
    restore();
  }
});

test("Initialize lays out one LED per cell on a strip", async () => {
  const { device, plugin, restore } = await deviceSetup({ ...CONTROLLER, ledCount: 3 });
  try {
    plugin.Initialize();
    const call = (m) => device.calls.find((c) => c.method === m)?.args;
    assert.deepEqual(call("setName"), ["Desk strip"]);
    assert.deepEqual(call("setSize"), [[3, 1]]);
    assert.deepEqual(call("setControllableLeds"), [["LED 1", "LED 2", "LED 3"], [[0, 0], [1, 0], [2, 0]]]);
    assert.ok(device.features.includes("udp"));
  } finally {
    restore();
  }
});

test("Render sends the DRGB golden, skips unchanged frames, keeps alive every second, Shutdown sends timeout 1", async () => {
  const clock = withClock();
  const { net, device, plugin, restore } = await deviceSetup(CONTROLLER, (x) => (x === 0 ? [255, 0, 0] : [0, 255, 0]));
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(net.sent.length, 1);
    assert.equal(net.sent[0].ip, "192.168.1.50");
    assert.equal(net.sent[0].port, expected.port);
    assert.deepEqual(net.sent[0].bytes, hexToBytes(expected.drgb_red_green.hex));
    clock.advance(33);
    plugin.Render();
    assert.equal(net.sent.length, 1, "unchanged frame is not resent");
    clock.advance(1000);
    plugin.Render();
    assert.equal(net.sent.length, 2, "keep-alive after 1 s");
    device.setColor([1, 2, 3]);
    clock.advance(33);
    plugin.Render();
    assert.equal(net.sent.length, 3, "changed frame is sent at once");
    assert.deepEqual(net.sent[2].bytes, [2, 2, 1, 2, 3, 1, 2, 3]);
    device.setColor((x) => (x === 0 ? [255, 0, 0] : [0, 255, 0]));
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(net.sent.at(-1).bytes, hexToBytes(expected.drgb_red_green_shutdown.hex));
    assert.equal(net.sent.length, 5);
  } finally {
    restore();
    clock.restore();
  }
});

test("490 LEDs fit one DRGB datagram; 500 LEDs go out as two DNRGB datagrams", async () => {
  const pixel = hexToBytes(expected.drgb_490.pixel_hex);
  {
    const { net, plugin, restore } = await deviceSetup({ ...CONTROLLER, ledCount: 490 }, pixel);
    try {
      plugin.Initialize();
      plugin.Render();
      assert.equal(net.sent.length, 1);
      const b = net.sent[0].bytes;
      assert.equal(b.length, expected.drgb_490.length);
      assert.deepEqual(b.slice(0, 2), hexToBytes(expected.drgb_490.header_hex));
      assert.deepEqual(b.slice(-3), pixel);
    } finally {
      restore();
    }
  }
  const { net, plugin, restore } = await deviceSetup({ ...CONTROLLER, ledCount: 500 }, pixel);
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(net.sent.length, 2);
    expected.dnrgb_500.packets.forEach((p, i) => {
      const b = net.sent[i].bytes;
      assert.equal(b.length, p.length);
      assert.deepEqual(b.slice(0, 4), hexToBytes(p.header_hex));
      for (let led = 0; led < p.leds; led++) {
        assert.deepEqual(b.slice(4 + led * 3, 7 + led * 3), pixel);
      }
    });
  } finally {
    restore();
  }
});

test("a custom UDP port from /json/info is used", async () => {
  const { net, plugin, restore } = await deviceSetup({ ...CONTROLLER, udpPort: 4210 }, [9, 9, 9]);
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(net.sent[0].port, 4210);
  } finally {
    restore();
  }
});

test("robust with the dry-run stub controller: nothing thrown, nothing sent", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ id: "x", ip: "127.0.0.1", port: 0, name: "stub", hostname: "stub", mac: "" });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    assert.equal(net.sent.length, 0);
    assert.ok(device.logs.length > 0);
  } finally {
    restore();
  }
  const empty = await deviceSetup(undefined);
  try {
    delete globalThis.controller;
    empty.plugin.Initialize();
    empty.plugin.Render();
    empty.plugin.Shutdown(false);
    assert.equal(empty.net.sent.length, 0);
  } finally {
    empty.restore();
  }
});
