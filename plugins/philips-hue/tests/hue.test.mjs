// SPDX-License-Identifier: GPL-3.0-or-later
//
// Philips Hue plugin against the network mocks: discovery (mDNS, Add by IP), link-button pairing
// (error 101, then success), settings across a restart, an unauthorized username, Forget, the
// lamps as subdevices, light commands for known colours (fixtures/xy.json, worked out by hand),
// the 100 ms rate limit with one request in flight per lamp, restore on Shutdown, and robustness
// with the host's dry-run stub controller.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockNet, createMockService, installNetGlobals, startService } from "../../_testkit/mock-net.mjs";
import { createMockDevice, loadPlugin } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const fixture = JSON.parse(readFileSync(join(here, "..", "fixtures", "xy.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));

const IP = "192.168.1.20";
const ID = "001788fffe1a2b3c";
const USER = "k9JfTeStUsErNaMe";
const CONFIG = { name: "Hue Bridge", bridgeid: "001788FFFE1A2B3C", apiversion: "1.65.0", modelid: "BSB002", swversion: "1965111030" };
const LIGHTS = {
  1: {
    type: "Extended color light", name: "Desk", modelid: "LCT015",
    state: { on: true, bri: 200, hue: 8000, sat: 120, xy: [0.4, 0.4], ct: 300, colormode: "xy", reachable: true },
    capabilities: { control: { colorgamuttype: "C", colorgamut: [[0.6915, 0.3083], [0.17, 0.7], [0.1532, 0.0475]] } },
  },
  2: { type: "Color temperature light", name: "Hall", state: { on: true, bri: 100, ct: 366, colormode: "ct" } },
  3: {
    type: "Color light", name: "Strip", modelid: "LST001",
    state: { on: false, bri: 50, hue: 100, sat: 200, xy: [0.3, 0.3], colormode: "hs" },
    capabilities: { control: { colorgamuttype: "A" } },
  },
  4: { type: "Dimmable light", name: "Lamp", state: { on: true, bri: 254 } },
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

function bridgeRoutes(net, { linkFailures = 2, lights = LIGHTS } = {}) {
  let posts = 0;
  net.route("GET", `http://${IP}/api/config`, { status: 200, body: CONFIG });
  net.route("POST", `http://${IP}/api`, () => {
    posts += 1;
    return posts <= linkFailures
      ? { status: 200, body: [{ error: { type: 101, address: "", description: "link button not pressed" } }] }
      : { status: 200, body: [{ success: { username: USER } }] };
  });
  net.route("GET", `http://${IP}/api/${USER}/lights`, { status: 200, body: lights });
  return net;
}

const ctl = (service) => service.controllers[0]?.obj;

test("metadata needs no globals and the manifest repeats the mDNS type", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.Type(), "network");
  assert.equal(plugin.DeviceType(), "lightingcontroller");
  assert.equal(plugin.SubdeviceController(), true);
  assert.equal(plugin.Documentation(), "hardware/philips-hue");
  const discovery = new plugin.DiscoveryService();
  assert.deepEqual(discovery.MDns, ["_hue._tcp.local."]);
  assert.deepEqual(manifest.devices[0].network.mdns, discovery.MDns.map((t) => t.replace(/\.local\.$/, "")));
});

test("Add by IP reads /api/config and lists an unpaired bridge", async () => {
  const s = await serviceSetup({ routes: (net) => bridgeRoutes(net) });
  try {
    s.discovery.forceDiscover(IP);
    assert.equal(s.service.controllers.length, 0, "nothing before the reply");
    s.net.flush();
    const bridge = ctl(s.service);
    assert.equal(bridge.id, ID);
    assert.equal(bridge.ip, IP);
    assert.equal(bridge.name, "Hue Bridge");
    assert.equal(bridge.paired, false);
    assert.equal(typeof bridge.startLink, "function");
    assert.equal(s.service.getSetting(ID, "ip"), IP);
    assert.deepEqual(JSON.parse(s.service.getSetting("bridges", "list")), [ID]);
    assert.equal(s.service.announced.length, 0);
  } finally {
    s.restore();
  }
});

test("Add by IP of something that is not a bridge adds nothing", async () => {
  const s = await serviceSetup({ routes: (net) => net.route("GET", `http://${IP}/api/config`, { status: 200, body: { name: "router" } }) });
  try {
    s.discovery.forceDiscover(IP);
    s.discovery.forceDiscover("not-an-ip");
    s.net.flush();
    assert.equal(s.service.controllers.length, 0);
  } finally {
    s.restore();
  }
});

test("mDNS with a bridgeid TXT adds the bridge without probing", async () => {
  const s = await serviceSetup({ routes: (net) => bridgeRoutes(net) });
  try {
    s.discovery.Discovered({ id: "Philips Hue - 1A2B3C", name: "Philips Hue - 1A2B3C", type: "_hue._tcp.local.", hostname: "hue.local.", port: 443, ip: IP, bridgeid: "001788fffe1a2b3c", modelid: "BSB002" });
    assert.equal(s.net.http.length, 0);
    assert.equal(ctl(s.service).id, ID);
    s.discovery.Discovered({ id: "x", ip: IP, bridgeid: "001788fffe1a2b3c" });
    assert.equal(s.service.controllers.length, 1, "no duplicate");
  } finally {
    s.restore();
  }
});

test("pairing: 101 (press the button) twice, then a username that is saved; lamps announced", async () => {
  const s = await serviceSetup({ routes: (net) => bridgeRoutes(net) });
  try {
    s.discovery.forceDiscover(IP);
    s.net.flush();
    const bridge = ctl(s.service);
    bridge.startLink();
    s.net.flush();
    assert.match(bridge.status, /link button/);
    assert.equal(bridge.paired, false);
    const post = s.net.http.find((r) => r.method === "POST");
    assert.deepEqual(JSON.parse(post.body), { devicetype: "lightloom#pc" });
    clock += 1000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(bridge.paired, false);
    clock += 1000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(s.net.http.filter((r) => r.method === "POST").length, 3);
    assert.equal(bridge.paired, true);
    assert.equal(s.service.getSetting(ID, "username"), USER);
    assert.equal(s.service.announced.length, 1);
    const data = s.service.announced[0];
    assert.equal(data.ip, IP);
    assert.equal(data.username, USER);
    assert.deepEqual(data.lights.map((l) => [l.id, l.name]), [["1", "Desk"], ["3", "Strip"]], "colour lamps only");
    assert.deepEqual(data.lights[0].gamut, LIGHTS[1].capabilities.control.colorgamut);
    assert.deepEqual(data.lights[1].gamut, fixture.gamuts.A, "gamut from colorgamuttype");
    clock += 1000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(s.net.http.filter((r) => r.method === "POST").length, 3, "no more pairing requests");
  } finally {
    s.restore();
  }
});

test("pairing gives up after 30 s", async () => {
  const s = await serviceSetup({ routes: (net) => bridgeRoutes(net, { linkFailures: Infinity }) });
  try {
    s.discovery.forceDiscover(IP);
    s.net.flush();
    const bridge = ctl(s.service);
    bridge.startLink();
    s.net.flush();
    for (let i = 0; i < 29; i++) {
      clock += 1000;
      s.discovery.Update();
      s.net.flush();
    }
    const posts = s.net.http.filter((r) => r.method === "POST").length;
    assert.equal(posts, 30);
    clock += 2000;
    s.discovery.Update();
    s.net.flush();
    assert.match(bridge.status, /timed out/);
    clock += 1000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(s.net.http.filter((r) => r.method === "POST").length, posts);
    assert.equal(bridge.paired, false);
    assert.equal(s.service.getSetting(ID, "username"), undefined);
  } finally {
    s.restore();
  }
});

test("after a restart a stored bridge is announced once its lamps are read", async () => {
  const settings = { bridges: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, username: USER } };
  const s = await serviceSetup({ settings, routes: (net) => bridgeRoutes(net) });
  try {
    assert.equal(ctl(s.service).paired, true);
    assert.equal(s.service.announced.length, 0);
    s.net.flush();
    assert.equal(s.service.announced.length, 1);
    assert.equal(s.service.announced[0].lights.length, 2);
    assert.equal(s.net.http.filter((r) => r.method === "POST").length, 0, "no pairing");
  } finally {
    s.restore();
  }
});

test("a bridge that answers 'unauthorized user' clears the username", async () => {
  const settings = { bridges: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, username: USER } };
  const s = await serviceSetup({
    settings,
    routes: (net) => net.route("GET", `http://${IP}/api/${USER}/lights`, { status: 200, body: [{ error: { type: 1, address: "/lights", description: "unauthorized user" } }] }),
  });
  try {
    s.net.flush();
    const bridge = ctl(s.service);
    assert.equal(bridge.paired, false);
    assert.equal(bridge.username, "");
    assert.equal(s.service.getSetting(ID, "username"), undefined);
    assert.equal(s.service.getSetting(ID, "ip"), IP, "the bridge stays known");
    assert.equal(s.service.announced.length, 0);
  } finally {
    s.restore();
  }
});

test("an unreachable paired bridge is retried 30 s later", async () => {
  const settings = { bridges: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, username: USER } };
  const s = await serviceSetup({ settings });
  try {
    s.net.flush();
    assert.equal(ctl(s.service).paired, true);
    const before = s.net.http.length;
    clock += 10_000;
    s.discovery.Update();
    assert.equal(s.net.http.length, before);
    bridgeRoutes(s.net);
    clock += 21_000;
    s.discovery.Update();
    s.net.flush();
    assert.equal(s.service.announced.length, 1);
  } finally {
    s.restore();
  }
});

test("Forget removes the username, suppresses the bridge and keeps mDNS from re-adding it", async () => {
  const settings = { bridges: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, username: USER } };
  const s = await serviceSetup({ settings, routes: (net) => bridgeRoutes(net) });
  try {
    s.net.flush();
    s.discovery.forgetController(ID);
    assert.deepEqual(s.service.suppressed, [ID]);
    assert.equal(s.service.controllers.length, 0);
    assert.equal(s.service.getSetting(ID, "username"), undefined);
    assert.equal(s.service.getSetting(ID, "ip"), undefined);
    assert.equal(s.service.getSetting("bridges", "list"), undefined);
    s.discovery.Discovered({ id: "Philips Hue", ip: IP, bridgeid: ID });
    assert.equal(s.service.controllers.length, 0);
    s.discovery.forceDiscover(IP);
    s.net.flush();
    assert.equal(ctl(s.service).paired, false, "Add by IP brings it back, unpaired");
  } finally {
    s.restore();
  }
});

// ---------------------------------------------------------------------------------------------
// Device context

async function deviceSetup({ data, colors = {}, routes = (net) => net } = {}) {
  const net = createMockNet();
  const device = createMockDevice({ subdeviceColors: colors });
  const restoreGlobals = installNetGlobals({ net, device, controller: data });
  const restoreClock = useClock();
  routes(net);
  const plugin = await loadPlugin(pluginPath);
  return { net, device, plugin, restore: () => { restoreGlobals(); restoreClock(); } };
}

const bridgeData = (lights) => ({ id: ID, ip: IP, name: "Hue Bridge", username: USER, paired: true, lights });
const puts = (net) => net.http.filter((r) => r.method === "PUT");

test("Initialize makes one 1x1 subdevice per lamp and reads their state", async () => {
  const lights = [{ id: "1", name: "Desk", gamut: fixture.gamuts.C }, { id: "3", name: "Strip", gamut: fixture.gamuts.A }];
  const s = await deviceSetup({ data: bridgeData(lights), routes: (net) => bridgeRoutes(net) });
  try {
    s.plugin.Initialize();
    assert.deepEqual(s.device.subdevices, [
      { id: "light-1", name: "Desk", size: [1, 1], leds: { names: ["Desk"], positions: [[0, 0]] } },
      { id: "light-3", name: "Strip", size: [1, 1], leds: { names: ["Strip"], positions: [[0, 0]] } },
    ]);
    assert.ok(s.device.calls.some((c) => c.method === "setName" && c.args[0] === "Hue Bridge"));
    assert.ok(s.device.calls.some((c) => c.method === "setSize" && c.args[0][0] === 1 && c.args[0][1] === 1));
    const gets = s.net.http.filter((r) => r.method === "GET");
    assert.deepEqual(gets.map((r) => [r.url, r.async]), [[`http://${IP}/api/${USER}/lights`, false]]);
  } finally {
    s.restore();
  }
});

test("light commands for known colours match the hand-worked fixture", async () => {
  const lights = fixture.cases.map((c, i) => ({ id: String(i + 1), name: `L${i + 1}`, gamut: c.gamut ? fixture.gamuts[c.gamut] : null }));
  const colors = Object.fromEntries(fixture.cases.map((c, i) => [`light-${i + 1}`, c.rgb]));
  const s = await deviceSetup({ data: bridgeData(lights), colors, routes: (net) => {
    net.route("PUT", /\/lights\/\d+\/state$/, { status: 200, body: [{ success: {} }] });
  } });
  try {
    s.plugin.Initialize();
    for (let i = 0; i < fixture.cases.length; i++) {
      s.plugin.Render();
      clock += 100;
    }
    const sent = puts(s.net);
    assert.equal(sent.length, fixture.cases.length);
    fixture.cases.forEach((c, i) => {
      assert.equal(sent[i].url, `http://${IP}/api/${USER}/lights/${i + 1}/state`);
      assert.equal(sent[i].async, true);
      assert.equal(sent[i].body, c.body, `rgb ${c.rgb} gamut ${c.gamut}`);
    });
  } finally {
    s.restore();
  }
});

test("rate limit: one command per 100 ms, round robin, one in flight per lamp, only on change", async () => {
  const lights = ["1", "2", "3"].map((id) => ({ id, name: id, gamut: fixture.gamuts.C }));
  const colors = { "light-1": [255, 0, 0], "light-2": [0, 255, 0], "light-3": [0, 0, 255] };
  const s = await deviceSetup({ data: bridgeData(lights), colors, routes: (net) => {
    net.route("PUT", /\/lights\/\d+\/state$/, { status: 200, body: [{ success: {} }] });
  } });
  try {
    s.plugin.Initialize();
    // Renders every 33 ms for 330 ms: sends at 0, 132 and 264 ms only.
    for (let i = 0; i < 10; i++) {
      s.plugin.Render();
      clock += 33;
    }
    assert.deepEqual(puts(s.net).map((r) => r.url.match(/lights\/(\d+)/)[1]), ["1", "2", "3"]);
    // Every lamp has a request in flight: a new colour waits for the reply.
    colors["light-1"][0] = 0; colors["light-1"][1] = 0; colors["light-1"][2] = 200;
    clock += 200;
    s.plugin.Render();
    assert.equal(puts(s.net).length, 3);
    s.net.flush();
    clock += 100;
    s.plugin.Render();
    assert.equal(puts(s.net).length, 4);
    assert.match(puts(s.net)[3].url, /lights\/1\//);
    s.net.flush();
    // Unchanged and nearly unchanged colours are not resent.
    colors["light-2"][1] = 253;
    for (let i = 0; i < 5; i++) {
      clock += 100;
      s.plugin.Render();
    }
    assert.equal(puts(s.net).length, 4);
    colors["light-2"][1] = 252;
    clock += 100;
    s.plugin.Render();
    assert.equal(puts(s.net).length, 5);
    assert.match(puts(s.net)[4].url, /lights\/2\//);
  } finally {
    s.restore();
  }
});

test("a failed command is logged once and retried", async () => {
  const lights = [{ id: "1", name: "Desk", gamut: null }];
  const s = await deviceSetup({ data: bridgeData(lights), colors: { "light-1": [10, 20, 30] } });
  try {
    s.plugin.Initialize();
    for (let i = 0; i < 3; i++) {
      s.plugin.Render();
      s.net.flush();
      clock += 100;
    }
    assert.equal(puts(s.net).length, 3);
    assert.equal(s.device.logs.filter((l) => /command failed/.test(l.message)).length, 1);
  } finally {
    s.restore();
  }
});

test("Shutdown puts every lamp back as Initialize found it", async () => {
  const lights = [{ id: "1", name: "Desk", gamut: null }, { id: "3", name: "Strip", gamut: null }, { id: "5", name: "Ceiling", gamut: null }];
  const states = {
    ...LIGHTS,
    5: { type: "Extended color light", name: "Ceiling", state: { on: true, bri: 180, ct: 366, xy: [0.45, 0.41], colormode: "ct" } },
  };
  const s = await deviceSetup({ data: bridgeData(lights), colors: { "light-1": [255, 0, 0] }, routes: (net) => {
    bridgeRoutes(net, { lights: states });
    net.route("PUT", /\/lights\/\d+\/state$/, { status: 200, body: [{ success: {} }] });
  } });
  try {
    s.plugin.Initialize();
    s.plugin.Render();
    s.net.flush();
    const before = puts(s.net).length;
    s.plugin.Shutdown(false);
    const restores = puts(s.net).slice(before);
    assert.deepEqual(restores.map((r) => [r.url.match(/lights\/(\d+)/)[1], r.async, r.body]), [
      ["1", false, "{\"on\":true,\"bri\":200,\"xy\":[0.4,0.4]}"],
      ["3", false, "{\"on\":false}"],
      ["5", false, "{\"on\":true,\"bri\":180,\"ct\":366}"],
    ]);
    s.plugin.Render();
    s.plugin.Shutdown(false);
    assert.equal(puts(s.net).length, before + 3, "nothing after Shutdown");
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
    assert.equal(s.net.http.length, 0, "no username: no request");
  } finally {
    s.restore();
  }
  const failing = await deviceSetup({ data: bridgeData([{ id: "1" }, null, { name: "no id" }]), colors: { "light-1": [1, 2, 3] } });
  try {
    failing.plugin.Initialize();
    failing.plugin.Render();
    failing.net.flush();
    failing.plugin.Shutdown(false);
    assert.equal(failing.device.subdevices.length, 1);
    assert.ok(failing.device.logs.some((l) => /could not read/.test(l.message)));
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
