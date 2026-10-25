// SPDX-License-Identifier: GPL-3.0-or-later
//
// Philips Hue Entertainment API against a fake bridge (the network mocks): pairing with
// generateclientkey, the CLIP v2 area / service / light chain over https with the bridge id as the
// expected certificate name, the announced area, the device context starting the area, the DTLS
// handshake with the application id then the username as PSK identity, HueStream v2 frames
// byte for byte, REST for the lamps outside the area, the fallback to REST, and Shutdown.

import { test } from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockNet, createMockService, installNetGlobals, startService } from "../../_testkit/mock-net.mjs";
import { createMockDevice, loadPlugin } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");

const IP = "192.168.1.20";
const ID = "001788fffe1a2b3c";
const USER = "k9JfTeStUsErNaMe";
const CLIENTKEY = "0123456789ABCDEF0123456789ABCDEF";
const APP_ID = "8f1c2a6e-1111-4abc-9def-0123456789ab";
const AREA = "1a8d99cc-967b-44f2-9202-43f976c0fa6b";
const OTHER_AREA = "9b8d99cc-967b-44f2-9202-43f976c0fa6c";
const V2 = `https://${IP}/clip/v2/resource`;
const LIGHTS = {
  1: { type: "Extended color light", name: "Desk", state: { on: true, bri: 200, xy: [0.4, 0.4], colormode: "xy" } },
  2: { type: "Extended color light", name: "Shelf", state: { on: true, bri: 120, xy: [0.3, 0.3], colormode: "xy" } },
  3: { type: "Extended color light", name: "Hall", state: { on: false, bri: 50, xy: [0.3, 0.3], colormode: "xy" } },
};
// Lamps 1 and 2 are in the area (2 is a gradient lamp with two channels); 3 is not.
const CONFIGS = [{
  id: AREA,
  type: "entertainment_configuration",
  metadata: { name: "TV area" },
  channels: [
    { channel_id: 0, position: { x: -1, y: 0, z: 0 }, members: [{ service: { rid: "svc-1", rtype: "entertainment" }, index: 0 }] },
    { channel_id: 1, position: { x: 1, y: 0, z: 0 }, members: [{ service: { rid: "svc-2", rtype: "entertainment" }, index: 0 }] },
    { channel_id: 2, position: { x: 1, y: 1, z: 0 }, members: [{ service: { rid: "svc-2", rtype: "entertainment" }, index: 1 }] },
  ],
}, {
  id: OTHER_AREA,
  type: "entertainment_configuration",
  metadata: { name: "Desk area" },
  channels: [{ channel_id: 0, members: [{ service: { rid: "svc-1", rtype: "entertainment" }, index: 0 }] }],
}];
const SERVICES = [
  { id: "svc-1", type: "entertainment", renderer: true, renderer_reference: { rid: "light-a", rtype: "light" } },
  { id: "svc-2", type: "entertainment", renderer: true, renderer_reference: { rid: "light-b", rtype: "light" } },
  { id: "svc-bridge", type: "entertainment", renderer: false },
];
const V2_LIGHTS = [
  { id: "light-a", id_v1: "/lights/1", type: "light" },
  { id: "light-b", id_v1: "/lights/2", type: "light" },
  { id: "light-c", id_v1: "/lights/3", type: "light" },
];

let clock = 1_000_000;
const realNow = Date.now;
function useClock() {
  clock = 1_000_000;
  Date.now = () => clock;
  return () => { Date.now = realNow; };
}

function v2Routes(net, { appId = APP_ID } = {}) {
  net.route("GET", `${V2}/entertainment_configuration`, { status: 200, body: { errors: [], data: CONFIGS } });
  net.route("GET", `${V2}/entertainment`, { status: 200, body: { errors: [], data: SERVICES } });
  net.route("GET", `${V2}/light`, { status: 200, body: { errors: [], data: V2_LIGHTS } });
  net.route("GET", `https://${IP}/auth/v1`, { status: 200, body: "", headers: appId ? { "hue-application-id": appId } : {} });
  return net;
}

function bridgeRoutes(net, { clientkey = CLIENTKEY } = {}) {
  net.route("GET", `http://${IP}/api/config`, { status: 200, body: { name: "Hue Bridge", bridgeid: ID.toUpperCase(), apiversion: "1.65.0" } });
  net.route("POST", `http://${IP}/api`, { status: 200, body: [{ success: clientkey ? { username: USER, clientkey } : { username: USER } }] });
  net.route("GET", `http://${IP}/api/${USER}/lights`, { status: 200, body: LIGHTS });
  return net;
}

async function serviceSetup({ settings, routes }) {
  const net = createMockNet();
  const service = createMockService({ settings });
  const restoreGlobals = installNetGlobals({ net, service });
  const restoreClock = useClock();
  routes(net);
  const plugin = await loadPlugin(pluginPath);
  const discovery = startService(plugin);
  return { net, service, discovery, restore: () => { restoreGlobals(); restoreClock(); } };
}

const https = (net) => net.http.filter((r) => r.url.startsWith("https://"));

test("pairing keeps the clientkey and announces the entertainment area read over CLIP v2", async () => {
  const s = await serviceSetup({ routes: (net) => v2Routes(bridgeRoutes(net)) });
  try {
    s.discovery.forceDiscover(IP);
    s.net.flush();
    s.service.controllers[0].obj.startLink();
    s.net.flush();
    assert.equal(s.service.getSetting(ID, "clientkey"), CLIENTKEY);
    assert.equal(s.service.announced.length, 1);
    const data = s.service.announced[0];
    assert.equal(data.clientkey, CLIENTKEY);
    assert.equal(data.appId, APP_ID);
    assert.deepEqual(data.entertainment, {
      id: AREA,
      name: "TV area",
      channels: [{ id: 0, light: "1" }, { id: 1, light: "2" }, { id: 2, light: "2" }],
    });
    assert.match(s.service.controllers[0].obj.status, /streams to "TV area"/);
    // Every https request expects the bridge id as the certificate name and carries the key.
    const secure = https(s.net);
    assert.deepEqual(secure.map((r) => r.url), [
      `${V2}/entertainment_configuration`, `${V2}/entertainment`, `${V2}/light`, `https://${IP}/auth/v1`,
    ]);
    for (const r of secure) {
      assert.equal(r.tlsName, ID);
      assert.equal(r.headers["hue-application-key"], USER);
    }
  } finally {
    s.restore();
  }
});

test("the 'area' setting picks the entertainment area; a bridge without CLIP v2 gets none", async () => {
  const settings = { bridges: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, username: USER, clientkey: CLIENTKEY, area: OTHER_AREA } };
  const s = await serviceSetup({ settings, routes: (net) => v2Routes(bridgeRoutes(net)) });
  try {
    s.net.flush();
    assert.deepEqual(s.service.announced[0].entertainment, { id: OTHER_AREA, name: "Desk area", channels: [{ id: 0, light: "1" }] });
  } finally {
    s.restore();
  }
  const old = await serviceSetup({ settings, routes: (net) => bridgeRoutes(net) });
  try {
    old.net.flush();
    assert.equal(old.service.announced.length, 1, "announced anyway");
    assert.equal(old.service.announced[0].entertainment, null);
    assert.ok(old.service.logs.some((l) => /CLIP v1 only/.test(l)));
  } finally {
    old.restore();
  }
});

test("without a clientkey (paired before, or an old bridge) nothing goes over https", async () => {
  const s = await serviceSetup({ routes: (net) => v2Routes(bridgeRoutes(net, { clientkey: null })) });
  try {
    s.discovery.forceDiscover(IP);
    s.net.flush();
    s.service.controllers[0].obj.startLink();
    s.net.flush();
    assert.equal(s.service.getSetting(ID, "clientkey"), undefined);
    assert.equal(s.service.announced[0].entertainment, null);
    assert.equal(https(s.net).length, 0);
  } finally {
    s.restore();
  }
});

test("Forget removes the clientkey and the area too", async () => {
  const settings = { bridges: { list: JSON.stringify([ID]) }, [ID]: { ip: IP, username: USER, clientkey: CLIENTKEY, area: AREA } };
  const s = await serviceSetup({ settings, routes: (net) => v2Routes(bridgeRoutes(net)) });
  try {
    s.net.flush();
    s.discovery.forgetController(ID);
    for (const key of ["username", "clientkey", "area", "ip"]) assert.equal(s.service.getSetting(ID, key), undefined, key);
  } finally {
    s.restore();
  }
});

// ---------------------------------------------------------------------------------------------
// Device context

const AREA_DATA = { id: AREA, name: "TV area", channels: [{ id: 0, light: "1" }, { id: 1, light: "2" }, { id: 2, light: "2" }] };
const LAMPS = [{ id: "1", name: "Desk", gamut: null }, { id: "2", name: "Shelf", gamut: null }, { id: "3", name: "Hall", gamut: null }];
const bridgeData = (extra = {}) => ({ id: ID, ip: IP, name: "Hue Bridge", username: USER, clientkey: CLIENTKEY, appId: APP_ID, paired: true, lights: LAMPS, entertainment: AREA_DATA, ...extra });

async function deviceSetup({ data = bridgeData(), colors, startStatus = 200 } = {}) {
  const net = createMockNet();
  const device = createMockDevice({ subdeviceColors: colors ?? { "light-1": [255, 0, 0], "light-2": [0, 128, 255], "light-3": [10, 20, 30] } });
  const restoreGlobals = installNetGlobals({ net, device, controller: data });
  const restoreClock = useClock();
  net.route("GET", `http://${IP}/api/${USER}/lights`, { status: 200, body: LIGHTS });
  net.route("PUT", /\/lights\/\d+\/state$/, { status: 200, body: [{ success: {} }] });
  net.route("PUT", `${V2}/entertainment_configuration/${AREA}`, { status: startStatus, body: { errors: [], data: [{ rid: AREA }] } });
  const plugin = await loadPlugin(pluginPath);
  return { net, device, plugin, restore: () => { restoreGlobals(); restoreClock(); } };
}

const areaPuts = (net) => net.http.filter((r) => r.method === "PUT" && r.url.includes("entertainment_configuration"));
const lampPuts = (net) => net.http.filter((r) => r.method === "PUT" && /\/lights\/\d+\/state$/.test(r.url));

/** The HueStream v2 frame the plugin should send for the default colours. */
function expectedFrame(seq) {
  const ascii = (t) => [...t].map((c) => c.charCodeAt(0));
  return [
    ...ascii("HueStream"), 0x02, 0x00, seq, 0x00, 0x00, 0x00, 0x00, ...ascii(AREA),
    0, 255, 255, 0, 0, 0, 0, // channel 0: lamp 1, red
    1, 0, 0, 128, 128, 255, 255, // channel 1: lamp 2
    2, 0, 0, 128, 128, 255, 255, // channel 2: lamp 2 (second gradient segment)
  ];
}

test("Initialize starts the area over https and connects DTLS with the application id", async () => {
  const s = await deviceSetup();
  try {
    s.plugin.Initialize();
    const start = areaPuts(s.net);
    assert.equal(start.length, 1);
    assert.deepEqual([start[0].async, start[0].body, start[0].tlsName, start[0].headers["hue-application-key"]], [false, "{\"action\":\"start\"}", ID, USER]);
    assert.ok(s.device.features.includes("dtls"));
    assert.deepEqual(s.net.dtls.connections, [{ host: IP, port: 2100, identity: APP_ID, key: CLIENTKEY }]);
    // Handshaking: the area's lamps wait, lamp 3 (outside the area) gets REST.
    for (let i = 0; i < 3; i++) {
      s.plugin.Render();
      s.net.flush();
      clock += 100;
    }
    assert.deepEqual(lampPuts(s.net).map((r) => r.url.match(/lights\/(\d+)/)[1]), ["3"]);
    assert.equal(s.net.dtls.sent.length, 0);
    // Established: one HueStream frame per Render(), sequence counting up.
    s.net.dtls.establish();
    s.plugin.Render();
    clock += 33;
    s.plugin.Render();
    assert.deepEqual(s.net.dtls.sent, [expectedFrame(0), expectedFrame(1)]);
    assert.equal(s.net.dtls.sent[0].length, 52 + 3 * 7);
    assert.equal(lampPuts(s.net).length, 1, "no REST for streamed lamps");
    assert.ok(s.device.logs.some((l) => /streaming to entertainment area "TV area"/.test(l.message)));
    // Shutdown: DTLS closed, the area stopped (synchronously), then every lamp restored.
    s.plugin.Shutdown(false);
    assert.equal(s.net.dtls.closed, 1);
    const stop = areaPuts(s.net)[1];
    assert.deepEqual([stop.async, stop.body, stop.tlsName], [false, "{\"action\":\"stop\"}", ID]);
    assert.deepEqual(lampPuts(s.net).slice(1).map((r) => [r.url.match(/lights\/(\d+)/)[1], r.async]), [["1", false], ["2", false], ["3", false]]);
  } finally {
    s.restore();
  }
});

test("a failed handshake retries with the username, then every lamp falls back to REST", async () => {
  const s = await deviceSetup();
  try {
    s.plugin.Initialize();
    s.net.dtls.fail("DTLS handshake with 192.168.1.20:2100 timed out");
    assert.deepEqual(s.net.dtls.connections.map((c) => c.identity), [APP_ID, USER]);
    s.net.dtls.fail("unknown_psk_identity (115)");
    assert.equal(s.net.dtls.connections.length, 2, "no third try");
    assert.ok(s.device.logs.some((l) => /streaming stopped \(unknown_psk_identity \(115\)\); using REST/.test(l.message)));
    const stop = areaPuts(s.net)[1];
    assert.deepEqual([stop.async, stop.body], [true, "{\"action\":\"stop\"}"]);
    for (let i = 0; i < 3; i++) {
      s.plugin.Render();
      s.net.flush();
      clock += 100;
    }
    assert.deepEqual(lampPuts(s.net).map((r) => r.url.match(/lights\/(\d+)/)[1]).sort(), ["1", "2", "3"]);
    assert.equal(s.net.dtls.sent.length, 0);
  } finally {
    s.restore();
  }
});

test("a stream the bridge closes, a handshake that never ends and a refused start fall back to REST", async () => {
  const closed = await deviceSetup();
  try {
    closed.plugin.Initialize();
    closed.net.dtls.establish();
    closed.plugin.Render();
    closed.net.dtls.closeFromPeer("the peer closed the connection: close_notify (0)");
    assert.equal(closed.net.dtls.connections.length, 1, "an established stream is not retried");
    closed.net.flush();
    clock += 100;
    closed.plugin.Render();
    // Lamp 3 at the first Render(), then a streamed lamp over REST.
    assert.deepEqual(lampPuts(closed.net).map((r) => r.url.match(/lights\/(\d+)/)[1]), ["3", "1"]);
    assert.equal(closed.net.dtls.sent.length, 1);
  } finally {
    closed.restore();
  }
  const hung = await deviceSetup();
  try {
    hung.plugin.Initialize();
    clock += 10_001;
    hung.plugin.Render();
    assert.ok(hung.device.logs.some((l) => /did not complete/.test(l.message)));
    assert.equal(hung.net.dtls.connections.length, 1);
    assert.equal(areaPuts(hung.net).length, 2, "the area is stopped");
  } finally {
    hung.restore();
  }
  const refused = await deviceSetup({ startStatus: 403 });
  try {
    refused.plugin.Initialize();
    assert.ok(refused.device.logs.some((l) => /could not start entertainment area "TV area" \(HTTP 403\)/.test(l.message)));
    assert.equal(refused.net.dtls.connections.length, 0);
    refused.plugin.Render();
    assert.equal(lampPuts(refused.net).length, 1);
    refused.plugin.Shutdown(false);
    assert.equal(areaPuts(refused.net).length, 1, "nothing to stop");
  } finally {
    refused.restore();
  }
});

test("no appId: the username is the only identity; no clientkey or area: REST only", async () => {
  const s = await deviceSetup({ data: bridgeData({ appId: "" }) });
  try {
    s.plugin.Initialize();
    assert.deepEqual(s.net.dtls.connections.map((c) => c.identity), [USER]);
  } finally {
    s.restore();
  }
  for (const extra of [{ clientkey: "" }, { entertainment: null }, { entertainment: { id: AREA, name: "x", channels: [{ id: 0, light: "9" }] } }]) {
    const rest = await deviceSetup({ data: bridgeData(extra) });
    try {
      rest.plugin.Initialize();
      assert.equal(areaPuts(rest.net).length, 0, JSON.stringify(extra));
      assert.equal(rest.net.dtls.connections.length, 0);
      assert.ok(!rest.device.features.includes("dtls"));
    } finally {
      rest.restore();
    }
  }
});
