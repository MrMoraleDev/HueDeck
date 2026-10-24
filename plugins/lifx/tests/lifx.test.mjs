// SPDX-License-Identifier: GPL-3.0-or-later
//
// LIFX plugin: GetService discovery, StateService parsing, the label/version questions and the
// announced controller; device packets (Get, SetLightPower, SetColor HSBK) byte for byte against
// the hand-worked goldens in fixtures/expected.json; the 20 messages/s limit; Shutdown restore
// from the LightState reply; robustness.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockNet, createMockService, installNetGlobals, startService } from "../../_testkit/mock-net.mjs";
import { bytesToHex, createMockDevice, hexToBytes, loadPlugin } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const raw = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const golden = Object.fromEntries(Object.entries(raw).filter(([k]) => k !== "$comment").map(([k, v]) => [k, hexToBytes(v)]));

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

const CONTROLLER = { id: "lifx-d073d5010203", mac: "d073d5010203", name: "Kitchen", ip: "192.168.1.30", port: 56700 };

async function deviceSetup({ controller = CONTROLLER, color = [0, 0, 0] } = {}) {
  const net = createMockNet();
  const device = createMockDevice({ color });
  const restore = installNetGlobals({ net, device, controller });
  const plugin = await loadPlugin(pluginPath);
  const unclock = useClock();
  return { net, device, plugin, restore: () => { unclock(); restore(); } };
}

const hex = (bytes) => bytesToHex(bytes);
const sentHex = (net, ip = "192.168.1.30") => net.sentTo(ip, 56700).map((s) => hex(s.bytes));

test("fixtures have the sizes their headers say", () => {
  for (const [name, bytes] of Object.entries(golden)) {
    if (name.startsWith("payload_")) {
      assert.equal(bytes.length, 13, name);
      continue;
    }
    assert.equal(bytes[0] | (bytes[1] << 8), bytes.length, name);
  }
});

test("metadata: broadcast on 56700; the constructor opens nothing", async () => {
  const net = createMockNet();
  const restore = installNetGlobals({ net });
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.equal(plugin.Type(), "network");
    const d = new plugin.DiscoveryService();
    assert.equal(d.UdpBroadcastAddress, "255.255.255.255");
    assert.equal(d.UdpBroadcastPort, 56700);
    assert.equal(net.sockets.length, 0);
  } finally {
    restore();
  }
});

test("GetService broadcast, StateService -> GetLabel/GetVersion -> announce with the label", async () => {
  const { net, service, discovery, restore } = await serviceSetup();
  try {
    assert.equal(service.broadcasts.length, 1);
    assert.equal(hex(service.broadcasts[0]), hex(golden.get_service_broadcast));
    // Our own broadcast looping back is not a bulb.
    discovery.Discovered({ ip: "192.168.1.2", port: 56700, buffer: golden.get_service_broadcast, response: "" });
    assert.equal(service.controllers.length, 0);
    discovery.Discovered({ id: "192.168.1.30", ip: "192.168.1.30", port: 56700, buffer: golden.state_service_reply, response: "" });
    assert.equal(service.controllers.length, 1);
    assert.equal(service.announced.length, 0, "announced once the name is known");
    assert.deepEqual(sentHex(net), [hex(golden.get_label), hex(golden.get_version)]);
    const sock = net.sentTo("192.168.1.30", 56700)[0].socket;
    net.deliver(sock, { buffer: golden.state_version_reply, address: "192.168.1.30", port: 56700 });
    net.deliver(sock, { buffer: golden.state_label_reply_kitchen, address: "192.168.1.30", port: 56700 });
    assert.equal(service.announced.length, 1);
    const a = service.announced[0];
    assert.equal(a.id, "lifx-d073d5010203");
    assert.equal(a.mac, "d073d5010203");
    assert.equal(a.name, "Kitchen");
    assert.equal(a.ip, "192.168.1.30");
    assert.equal(a.port, 56700);
    assert.equal(a.product, 27);
    assert.equal(a.status, "online");
    // A second StateService (next round) does not announce again.
    discovery.Discovered({ ip: "192.168.1.30", buffer: golden.state_service_reply });
    net.deliver(sock, { buffer: golden.state_label_reply_kitchen, address: "192.168.1.30" });
    assert.equal(service.announced.length, 1);
  } finally {
    restore();
  }
});

test("without a label reply the bulb is announced after 3 s with a default name", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.Discovered({ ip: "192.168.1.30", buffer: golden.state_service_reply });
    clock.t += 1000;
    discovery.Update();
    assert.equal(service.announced.length, 0);
    clock.t += 2000;
    discovery.Update();
    assert.equal(service.announced.length, 1);
    assert.equal(service.announced[0].name, "LIFX 010203");
  } finally {
    restore();
  }
});

test("discovery repeats every 30 s; silent bulbs are suppressed after three rounds", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.Discovered({ ip: "192.168.1.30", buffer: golden.state_service_reply });
    clock.t += 30_000;
    discovery.Update();
    assert.equal(service.broadcasts.length, 2);
    clock.t += 60_000;
    discovery.Update();
    assert.deepEqual(service.suppressed, []);
    clock.t += 6_000;
    discovery.Update();
    assert.deepEqual(service.suppressed, ["lifx-d073d5010203"]);
    assert.equal(service.controllers.length, 0);
  } finally {
    restore();
  }
});

test("forceDiscover unicasts GetService, persists, and the reply on the socket adds the bulb", async () => {
  const { net, service, discovery, restore } = await serviceSetup();
  try {
    discovery.forceDiscover("192.168.1.30");
    const sent = net.sentTo("192.168.1.30", 56700);
    assert.equal(sent.length, 1);
    const unicast = sent[0].bytes;
    assert.equal(hex(unicast.slice(0, 23)), hex(golden.get_service_broadcast.slice(0, 23)));
    assert.equal(unicast[32], 2);
    assert.equal(service.getSetting("forced", "ips"), '["192.168.1.30"]');
    net.deliver(sent[0].socket, { buffer: golden.state_service_reply, address: "192.168.1.30", port: 56700 });
    assert.equal(service.controllers.length, 1, "the placeholder is replaced by the bulb");
    assert.equal(service.controllers[0].obj.forced, true);
    discovery.forgetController("lifx-d073d5010203");
    assert.equal(service.controllers.length, 0);
    assert.equal(service.getSetting("forced", "ips"), "[]");
  } finally {
    restore();
  }
});

test("added IPs come back after a restart", async () => {
  const { net, service, restore } = await serviceSetup({ forced: { ips: '["192.168.1.31"]' } });
  try {
    assert.equal(net.sentTo("192.168.1.31", 56700).length, 1);
    assert.equal(service.controllers[0].obj.pending, true);
  } finally {
    restore();
  }
});

test("Initialize sends Get and SetLightPower; Render sends SetColor HSBK", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    assert.deepEqual(sentHex(net), [hex(golden.light_get), hex(golden.set_power_on)]);
    plugin.Render();
    assert.equal(sentHex(net).at(-1), hex(golden.set_color_red_seq2));
    const payloadOf = (color) => {
      device.setColor(color);
      clock.t += 50;
      plugin.Render();
      return hex(net.sentTo("192.168.1.30", 56700).at(-1).bytes.slice(36));
    };
    assert.equal(payloadOf([0, 0, 255]), hex(golden.payload_blue));
    assert.equal(payloadOf([0, 128, 0]), hex(golden.payload_dim_green_008000));
    assert.equal(payloadOf([0, 0, 0]), hex(golden.payload_black));
  } finally {
    restore();
  }
});

test("at most 20 messages a second and only on change", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    const base = net.sent.length;
    for (let i = 0; i < 30; i++) {
      device.setColor([i * 8, 0, 0]);
      plugin.Render();
      clock.t += 33;
    }
    const sent = net.sent.length - base;
    assert.ok(sent <= 20 && sent >= 15, `${sent} SetColor in ~1 s`);
    const before = net.sent.length;
    for (let i = 0; i < 10; i++) {
      clock.t += 100;
      plugin.Render();
    }
    assert.equal(net.sent.length, before + 1, "the last colour once, then nothing while it stays");
  } finally {
    restore();
  }
});

test("Shutdown restores colour and power from the LightState reply", async () => {
  const { net, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    net.deliver(net.sockets[0], { buffer: golden.light_state_reply, address: "192.168.1.30", port: 56700 });
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(sentHex(net).slice(-2), [hex(golden.restore_color_seq3), hex(golden.restore_power_off_seq4)]);
    assert.equal(net.sockets[0].closed, true);
  } finally {
    restore();
  }
});

test("without a LightState reply Shutdown sends nothing and logs", async () => {
  const { net, device, plugin, restore } = await deviceSetup();
  try {
    plugin.Initialize();
    const before = net.sent.length;
    plugin.Shutdown(true);
    assert.equal(net.sent.length, before);
    assert.ok(device.logs.some((l) => /keeps the last colour/.test(l.message)));
  } finally {
    restore();
  }
});

test("robustness: empty controller, dry-run stub, garbage replies", async () => {
  for (const controller of [{}, undefined, { id: "x", ip: "127.0.0.1", port: 0, name: "stub", hostname: "", mac: "" }]) {
    const { net, plugin, restore } = await deviceSetup({ controller });
    try {
      plugin.Initialize();
      if (net.sockets[0]) {
        net.deliver(net.sockets[0], { buffer: [1, 2, 3] });
      }
      plugin.Render();
      plugin.Shutdown(false);
    } finally {
      restore();
    }
  }
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.Discovered({ ip: "192.168.1.9", buffer: [0x24, 0x00], response: "" });
    discovery.Discovered({ ip: "192.168.1.9" });
    discovery.Discovered(undefined);
    assert.equal(service.controllers.length, 0);
  } finally {
    restore();
  }
});
