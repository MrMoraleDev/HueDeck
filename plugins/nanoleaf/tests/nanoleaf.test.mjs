// SPDX-License-Identifier: GPL-3.0-or-later
//
// Nanoleaf plugin against the network mocks: discovery (mDNS, Add by IP), power-button pairing
// (403 until the device is in pairing mode, then a token that is saved), settings across a
// restart, a revoked token, Forget, the layout grid and extControl datagrams for known colours
// (fixtures/packets.json, worked out by hand), send-on-change with a 1 s keepalive, restore on
// Shutdown, and robustness with the host's dry-run stub controller.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockNet, createMockService, installNetGlobals, startService } from "../../_testkit/mock-net.mjs";
import { createMockDevice, hexToBytes, loadPlugin } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const fixture = JSON.parse(readFileSync(join(here, "..", "fixtures", "packets.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));

const IP = "192.168.1.30";
const ID = "5C:E7:53:12:34:56";
const TOKEN = "dTeStToKeN0123456789abcdefABCDEF";
const BASE = `http://${IP}:16021/api/v1/${TOKEN}`;
const INFO = {
  name: "Shapes 1A2B", serialNo: "S123", manufacturer: "Nanoleaf", firmwareVersion: "9.2.4", model: "NL42",
  state: { on: { value: true }, brightness: { value: 80 } },
  effects: { select: "Northern Lights", effectsList: ["Northern Lights", "Flames"] },
  panelLayout: { globalOrientation: { value: 0 }, layout: { numPanels: 5, sideLength: 100, positionData: fixture.layout.positionData } },
};

let clock = 1_000_000;
const realNow = Date.now;
function useClock() {
  clock = 1_000_000;
  Date.now = () => clock;
  return () => { Date.now = realNow; };
}

async function serviceSetup({ settings, routes = (net) => net } = {}) {
  const net = createMockNet();
  const service = createMockService({ settings });
  const restoreGlobals = installNetGlobals({ net, service });
  const restoreClock = useClock();
  routes(net);
  const plugin = await loadPlugin(pluginPath);
  const discovery = startService(plugin);
  return { net, service, plugin, discovery, restore: () => { restoreGlobals(); restoreClock(); } };
}

function deviceRoutes(net, { refusals = 2, info = INFO } = {}) {
  let posts = 0;
  net.route("GET", `http://${IP}:16021/api/v1/`, { status: 401, body: "" });
  net.route("POST", `http://${IP}:16021/api/v1/new`, () => {
    posts += 1;
    return posts <= refusals ? { status: 403, body: "" } : { status: 200, body: { auth_token: TOKEN } };
  });
  net.route("GET", `${BASE}/`, { status: 200, body: info });
  return net;
}

const ctl = (service) => service.controllers[0]?.obj;
const mdns = { id: ID, name: "Shapes 1A2B", type: "_nanoleafapi._tcp.local.", hostname: "Shapes-1A2B.local.", port: 16021, ip: IP, md: "NL42", srcvers: "9.2.4" };

test("metadata needs no globals and the manifest repeats the mDNS type", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.Type(), "network");
  assert.equal(plugin.Documentation(), "hardware/nanoleaf");
  const discovery = new plugin.DiscoveryService();
  assert.deepEqual(discovery.MDns, ["_nanoleafapi._tcp.local."]);
  assert.deepEqual(manifest.devices[0].network.mdns, discovery.MDns.map((t) => t.replace(/\.local\.$/, "")));
});

test("mDNS adds an unpaired device under its TXT id; Add by IP of the same address adds nothing new", async () => {
  const s = await serviceSetup({ routes: (net) => deviceRoutes(net) });
  try {
    s.discovery.Discovered(mdns);
    const dev = ctl(s.service);
    assert.equal(dev.id, ID);
    assert.equal(dev.ip, IP);
    assert.equal(dev.paired, false);
    assert.equal(dev.model, "NL42");
    assert.equal(s.service.getSetting(ID, "ip"), IP);
    s.discovery.forceDiscover(IP);
    s.net.flush();
    assert.equal(s.service.controllers.length, 1);
  } finally {
    s.restore();
  }
});

test("Add by IP: any answer on 16021 adds a device named by its address; silence adds nothing", async () => {
  const s = await serviceSetup({ routes: (net) => deviceRoutes(net) });
  try {
    s.discovery.forceDiscover("192.168.1.99");
    s.discovery.forceDiscover(IP);
    s.net.flush();
    assert.equal(s.service.controllers.length, 1);
    assert.equal(ctl(s.service).id, "ip-192-168-1-30");
    assert.deepEqual(JSON.parse(s.service.getSetting("devices", "list")), ["ip-192-168-1-30"]);
  } finally {
    s.restore();
  }
});

test("pairing: 403 (hold the power button) twice, then a token that is saved; panels announced", async () => {
  const s = await serviceSetup({ routes: (net) => deviceRoutes(net) });
  try {
    s.discovery.Discovered(mdns);
    const dev = ctl(s.service);
    dev.startLink();
    s.net.flush();
    assert.match(dev.status, /power button/);
    clock += 1000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(dev.paired, false);
    assert.match(dev.status, /power button/);
    clock += 1000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(dev.paired, true);
    assert.equal(s.service.getSetting(ID, "token"), TOKEN);
    assert.equal(s.net.http.filter((r) => r.method === "POST").length, 3);
    assert.equal(s.service.announced.length, 1);
    const data = s.service.announced[0];
    assert.equal(data.token, TOKEN);
    assert.equal(data.name, "Shapes 1A2B");
    assert.equal(data.extControl, "v2");
    assert.deepEqual(data.panels.map((p) => p.id), [101, 300, 4660], "controller and power supply left out");
    clock += 1000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(s.net.http.filter((r) => r.method === "POST").length, 3, "no more pairing requests");
  } finally {
    s.restore();
  }
});

test("pairing gives up after 30 s", async () => {
  const s = await serviceSetup({ routes: (net) => deviceRoutes(net, { refusals: Infinity }) });
  try {
    s.discovery.Discovered(mdns);
    const dev = ctl(s.service);
    dev.startLink();
    s.net.flush();
    for (let i = 0; i < 31; i++) {
      clock += 1000;
      s.discovery.Update();
      s.net.flush();
    }
    assert.match(dev.status, /timed out/);
    const posts = s.net.http.filter((r) => r.method === "POST").length;
    assert.equal(posts, 31);
    clock += 1000;
    s.discovery.Update();
    assert.equal(s.net.http.filter((r) => r.method === "POST").length, posts);
  } finally {
    s.restore();
  }
});

test("Light Panels (NL22) stream with extControl v1", async () => {
  const s = await serviceSetup({ routes: (net) => deviceRoutes(net, { refusals: 0, info: { ...INFO, model: "NL22" } }) });
  try {
    s.discovery.Discovered(mdns);
    ctl(s.service).startLink();
    s.net.flush();
    assert.equal(s.service.announced[0].extControl, "v1");
  } finally {
    s.restore();
  }
});

test("after a restart a stored device is announced; a revoked token unpairs it", async () => {
  const settings = { devices: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, token: TOKEN } };
  const s = await serviceSetup({ settings, routes: (net) => deviceRoutes(net) });
  try {
    assert.equal(ctl(s.service).paired, true);
    s.net.flush();
    assert.equal(s.service.announced.length, 1);
  } finally {
    s.restore();
  }
  const revoked = await serviceSetup({ settings, routes: (net) => net.route("GET", `${BASE}/`, { status: 401, body: "" }) });
  try {
    revoked.net.flush();
    const dev = ctl(revoked.service);
    assert.equal(dev.paired, false);
    assert.equal(revoked.service.getSetting(ID, "token"), undefined);
    assert.equal(revoked.service.getSetting(ID, "ip"), IP);
    assert.equal(revoked.service.announced.length, 0);
  } finally {
    revoked.restore();
  }
});

test("Forget removes the token, suppresses the device and keeps mDNS from re-adding it", async () => {
  const settings = { devices: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, token: TOKEN } };
  const s = await serviceSetup({ settings, routes: (net) => deviceRoutes(net) });
  try {
    s.net.flush();
    s.discovery.forgetController(ID);
    assert.deepEqual(s.service.suppressed, [ID]);
    assert.equal(s.service.controllers.length, 0);
    assert.equal(s.service.getSetting(ID, "token"), undefined);
    assert.equal(s.service.getSetting("devices", "list"), undefined);
    s.discovery.Discovered(mdns);
    assert.equal(s.service.controllers.length, 0);
  } finally {
    s.restore();
  }
});

// ---------------------------------------------------------------------------------------------
// Device context

async function deviceSetup({ data, color = [0, 0, 0], routes = (net) => net } = {}) {
  const net = createMockNet();
  const device = createMockDevice({ color });
  const restoreGlobals = installNetGlobals({ net, device, controller: data });
  const restoreClock = useClock();
  routes(net);
  const plugin = await loadPlugin(pluginPath);
  return { net, device, plugin, restore: () => { restoreGlobals(); restoreClock(); } };
}

const panelsOf = (positionData) => positionData.filter((p) => ![5, 12].includes(p.shapeType)).map((p) => ({ id: p.panelId, x: p.x, y: p.y, o: p.o, shapeType: p.shapeType }));
const deviceData = (extra = {}) => ({ id: ID, ip: IP, port: 16021, name: "Shapes 1A2B", model: "NL42", token: TOKEN, paired: true, extControl: "v2", panels: panelsOf(fixture.layout.positionData), ...extra });

function streamingRoutes(net, { effect = "Northern Lights", state = { on: { value: true }, brightness: { value: 80 }, hue: { value: 10 }, sat: { value: 20 }, ct: { value: 4000 }, colorMode: "effect" }, extReply = "" } = {}) {
  net.route("GET", `${BASE}/effects/select`, { status: 200, body: JSON.stringify(effect) });
  net.route("GET", `${BASE}/state`, { status: 200, body: state });
  net.route("PUT", `${BASE}/effects`, (req) => (JSON.parse(req.body).write ? { status: 200, body: extReply } : { status: 204, body: "" }));
  net.route("PUT", `${BASE}/state`, { status: 204, body: "" });
  return net;
}

const sampler = (map) => (x, y) => map[`${x},${y}`] ?? [0, 0, 0];

test("Initialize lays out one LED per panel, saves the effect and enters extControl v2", async () => {
  const s = await deviceSetup({ data: deviceData(), routes: (net) => streamingRoutes(net) });
  try {
    s.plugin.Initialize();
    const call = (m) => s.device.calls.find((c) => c.method === m)?.args;
    assert.deepEqual(call("setSize")[0], fixture.layout.size);
    assert.deepEqual(call("setControllableLeds"), [fixture.layout.names, fixture.layout.cells]);
    assert.deepEqual(call("setName"), ["Shapes 1A2B"]);
    assert.ok(s.device.features.includes("udp"));
    assert.deepEqual(s.net.http.map((r) => [r.method, r.url, r.async, r.body]), [
      ["GET", `${BASE}/effects/select`, false, null],
      ["GET", `${BASE}/state`, false, null],
      ["PUT", `${BASE}/effects`, false, "{\"write\":{\"command\":\"display\",\"animType\":\"extControl\",\"extControlVersion\":\"v2\"}}"],
    ]);
  } finally {
    s.restore();
  }
});

test("v2 datagram for known colours matches the hand-worked fixture", async () => {
  const [a, b, c] = fixture.v2.colors;
  const s = await deviceSetup({ data: deviceData(), color: sampler({ "0,2": a, "2,2": b, "2,0": c }), routes: (net) => streamingRoutes(net) });
  try {
    s.plugin.Initialize();
    s.plugin.Render();
    const sent = s.net.sentTo(IP, 60222);
    assert.equal(sent.length, 1);
    assert.deepEqual(sent[0].bytes, hexToBytes(fixture.v2.bytes));
  } finally {
    s.restore();
  }
});

test("v1 datagram goes to the address the extControl reply names", async () => {
  const [a, b] = fixture.v1.colors;
  const data = deviceData({ model: "NL22", extControl: "v1", panels: [{ id: 12, x: 0, y: 0, o: 0, shapeType: 0 }, { id: 200, x: 150, y: 0, o: 60, shapeType: 0 }] });
  const s = await deviceSetup({ data, color: sampler({ "0,0": a, "2,0": b }), routes: (net) => streamingRoutes(net, { extReply: { streamControlIpAddr: IP, streamControlPort: 60221, streamControlProtocol: "udp" } }) });
  try {
    s.plugin.Initialize();
    assert.match(s.net.http[2].body, /"extControlVersion":"v1"/);
    s.plugin.Render();
    const sent = s.net.sentTo(IP, 60221);
    assert.equal(sent.length, 1);
    assert.deepEqual(sent[0].bytes, hexToBytes(fixture.v1.bytes));
  } finally {
    s.restore();
  }
});

test("frames are sent on change, and an unchanged frame once a second", async () => {
  const colors = { "0,2": [10, 20, 30] };
  const s = await deviceSetup({ data: deviceData(), color: (x, y) => colors[`${x},${y}`] ?? [0, 0, 0], routes: (net) => streamingRoutes(net) });
  try {
    s.plugin.Initialize();
    for (let i = 0; i < 30; i++) {
      s.plugin.Render();
      clock += 33;
    }
    assert.equal(s.net.sentTo(IP, 60222).length, 1, "990 ms of the same frame: one datagram");
    clock += 33;
    s.plugin.Render();
    assert.equal(s.net.sentTo(IP, 60222).length, 2, "keepalive after 1 s");
    colors["0,2"] = [11, 20, 30];
    clock += 33;
    s.plugin.Render();
    assert.equal(s.net.sentTo(IP, 60222).length, 3, "a change is sent at once");
  } finally {
    s.restore();
  }
});

test("a refused extControl request is retried every 5 s from Render", async () => {
  const s = await deviceSetup({ data: deviceData(), routes: (net) => {
    streamingRoutes(net);
    net.route("PUT", `${BASE}/effects`, { status: 500, body: "" });
  } });
  try {
    s.plugin.Initialize();
    s.plugin.Render();
    assert.equal(s.net.sent.length, 0);
    const puts = () => s.net.http.filter((r) => r.method === "PUT" && r.url.endsWith("/effects")).length;
    assert.equal(puts(), 1);
    clock += 5000;
    streamingRoutes(s.net);
    s.plugin.Render();
    assert.equal(puts(), 2);
    s.net.flush();
    s.plugin.Render();
    assert.equal(s.net.sentTo(IP, 60222).length, 1);
  } finally {
    s.restore();
  }
});

test("Shutdown selects the saved effect again", async () => {
  const s = await deviceSetup({ data: deviceData(), routes: (net) => streamingRoutes(net) });
  try {
    s.plugin.Initialize();
    s.plugin.Render();
    const before = s.net.http.length;
    s.plugin.Shutdown(false);
    assert.deepEqual(s.net.http.slice(before).map((r) => [r.method, r.url, r.async, r.body]), [
      ["PUT", `${BASE}/effects`, false, "{\"select\":\"Northern Lights\"}"],
    ]);
  } finally {
    s.restore();
  }
});

test("Shutdown restores a solid colour through the state, and switches back off", async () => {
  const state = { on: { value: false }, brightness: { value: 40 }, hue: { value: 120 }, sat: { value: 90 }, ct: { value: 2700 }, colorMode: "hs" };
  const s = await deviceSetup({ data: deviceData(), routes: (net) => streamingRoutes(net, { effect: "*Solid*", state }) });
  try {
    s.plugin.Initialize();
    assert.deepEqual(s.net.http.slice(2, 3).map((r) => [r.url, r.body]), [[`${BASE}/state`, "{\"on\":{\"value\":true}}"]], "turned on to stream");
    const before = s.net.http.length;
    s.plugin.Shutdown(false);
    assert.deepEqual(s.net.http.slice(before).map((r) => [r.url, r.body]), [
      [`${BASE}/state`, "{\"hue\":{\"value\":120},\"sat\":{\"value\":90},\"brightness\":{\"value\":40}}"],
      [`${BASE}/state`, "{\"on\":{\"value\":false}}"],
    ]);
  } finally {
    s.restore();
  }
});

test("dry-run stub controller and failing HTTP never throw", async () => {
  const stub = { id: "stub", ip: "127.0.0.1", port: 0, name: "stub", hostname: "stub", mac: "00:00:00:00:00:00" };
  const s = await deviceSetup({ data: stub });
  try {
    s.plugin.Initialize();
    s.plugin.Render();
    s.plugin.Shutdown(true);
    assert.equal(s.net.http.length, 0);
    assert.equal(s.net.sent.length, 0);
  } finally {
    s.restore();
  }
  const failing = await deviceSetup({ data: deviceData({ panels: [{ id: 1, x: 0, y: 0 }, null, { x: 3 }] }) });
  try {
    failing.plugin.Initialize();
    failing.plugin.Render();
    failing.net.flush();
    failing.plugin.Render();
    failing.plugin.Shutdown(false);
    assert.equal(failing.net.sent.length, 0);
    assert.ok(failing.device.logs.some((l) => /external control/.test(l.message)));
  } finally {
    failing.restore();
  }
  const bare = await deviceSetup({ data: undefined });
  try {
    bare.plugin.Initialize();
    bare.plugin.Render();
    bare.plugin.Shutdown(false);
  } finally {
    bare.restore();
  }
});
