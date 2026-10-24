// SPDX-License-Identifier: GPL-3.0-or-later
//
// Yeelight plugin: M-SEARCH discovery, search replies and NOTIFY adverts -> announced controller,
// added IPs, stale lights; device session: get_prop, music mode (listen + set_music + the light
// connecting with net.incoming), the per-frame commands against the hand-worked goldens in
// fixtures/expected.json, rate limits, the control-connection fallback, white-only lights,
// Shutdown restore and robustness.

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

const IP = "192.168.1.239";
const COLOR_SUPPORT = "get_prop set_default set_power toggle set_bright start_cf stop_cf set_scene cron_add cron_get cron_del set_ct_abx set_rgb set_music".split(" ");
const CONTROLLER = { id: "0x000000000015243f", name: "desk", ip: IP, port: 55443, model: "color", support: COLOR_SUPPORT };

async function deviceSetup({ controller = CONTROLLER, color = [0, 0, 0], netOptions = {}, listen } = {}) {
  const net = createMockNet(netOptions);
  if (listen !== undefined) {
    const create = net.tcp.createSocket;
    net.tcp.createSocket = () => {
      const s = create();
      s.listen = listen;
      return s;
    };
  }
  const device = createMockDevice({ color });
  const restore = installNetGlobals({ net, device, controller });
  const plugin = await loadPlugin(pluginPath);
  const unclock = useClock();
  return { net, device, plugin, restore: () => { unclock(); restore(); } };
}

const control = (net) => net.sentTo(IP, 55443).map((s) => s.text);
const music = (net) => net.sentTo(IP, 40000).map((s) => s.text);
const noId = (lines) => lines.map((l) => l.replace(/"id":\d+,/, ""));
const listener = (net) => net.sockets.find((s) => s.listening);

test("metadata: SSDP-like discovery on 239.255.255.250:1982; the constructor opens nothing", async () => {
  const net = createMockNet();
  const restore = installNetGlobals({ net });
  try {
    const plugin = await loadPlugin(pluginPath);
    assert.equal(plugin.Type(), "network");
    const d = new plugin.DiscoveryService();
    assert.equal(d.UdpBroadcastAddress, "239.255.255.250");
    assert.equal(d.UdpBroadcastPort, 1982);
    assert.equal(net.sockets.length, 0);
  } finally {
    restore();
  }
});

test("M-SEARCH at start; a search reply becomes an announced controller", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    assert.deepEqual(service.broadcasts, [golden.search]);
    discovery.Discovered({ ip: IP, port: 1982, response: golden.search });
    assert.equal(service.controllers.length, 0, "our own M-SEARCH is ignored");
    discovery.Discovered({ id: IP, ip: IP, port: 1982, response: golden.search_reply });
    assert.equal(service.announced.length, 1);
    const a = service.announced[0];
    assert.equal(a.id, "0x000000000015243f");
    assert.equal(a.name, "desk");
    assert.equal(a.ip, IP);
    assert.equal(a.port, 55443);
    assert.equal(a.model, "color");
    assert.deepEqual(a.support, COLOR_SUPPORT);
    assert.equal(a.status, "power on");
    // The same advert again: no new announce.
    discovery.Discovered({ ip: IP, response: golden.search_reply });
    assert.equal(service.announced.length, 1);
  } finally {
    restore();
  }
});

test("NOTIFY adverts are parsed the same; a nameless light is named after its model", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.Discovered({ ip: "192.168.1.240", response: golden.notify_mono });
    const a = service.announced[0];
    assert.equal(a.id, "0x0000000000abcdef");
    assert.equal(a.name, "Yeelight mono");
    assert.equal(a.support.includes("set_rgb"), false);
    assert.equal(a.status, "power off");
  } finally {
    restore();
  }
});

test("searches every 60 s; silent lights are suppressed after three rounds", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.Discovered({ ip: IP, response: golden.search_reply });
    clock.t += 60_000;
    discovery.Update();
    assert.equal(service.broadcasts.length, 2);
    clock.t += 120_000;
    discovery.Update();
    assert.deepEqual(service.suppressed, []);
    clock.t += 6_000;
    discovery.Update();
    assert.deepEqual(service.suppressed, ["0x000000000015243f"]);
  } finally {
    restore();
  }
});

test("forceDiscover announces a generated id, persists it, and discovery replaces it", async () => {
  const { service, discovery, restore } = await serviceSetup();
  try {
    discovery.forceDiscover(IP);
    assert.equal(service.getSetting("forced", "ips"), JSON.stringify([IP]));
    assert.equal(service.announced.length, 1);
    assert.equal(service.announced[0].id, "forced-192-168-1-239");
    assert.equal(service.announced[0].port, 55443);
    assert.deepEqual(service.announced[0].support, []);
    // Forced lights are not removed for being silent.
    clock.t += 600_000;
    discovery.Update();
    assert.deepEqual(service.suppressed, []);
    discovery.Discovered({ ip: IP, response: golden.search_reply });
    assert.deepEqual(service.suppressed, ["forced-192-168-1-239"]);
    assert.deepEqual(service.controllers.map((c) => c.id), ["0x000000000015243f"]);
    discovery.forgetController("0x000000000015243f");
    assert.equal(service.getSetting("forced", "ips"), "[]");
  } finally {
    restore();
  }
});

test("added IPs come back after a restart", async () => {
  const { service, restore } = await serviceSetup({ forced: { ips: JSON.stringify(["192.168.1.77"]) } });
  try {
    assert.deepEqual(service.announced.map((a) => a.ip), ["192.168.1.77"]);
  } finally {
    restore();
  }
});

test("music mode: get_prop, listen, set_music, the light connects, then the stream", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    const server = listener(net);
    assert.ok(server, "a listening socket");
    assert.equal(server.listening.peer, IP, "listen(0, controller.ip)");
    assert.deepEqual(control(net), [golden.get_prop, golden.set_music_on.replace("{PORT}", String(server.listening.port))]);
    // Before the light connects nothing is streamed.
    plugin.Render();
    assert.deepEqual(music(net), []);
    net.incoming(server);
    plugin.Render();
    assert.deepEqual(music(net), [golden.music_power_on, golden.music_red_rgb, golden.music_red_bright]);
    // Within 33 ms of the last send: nothing, even for a new colour.
    device.setColor([0, 0, 0x80]);
    clock.t += 10;
    plugin.Render();
    assert.equal(music(net).length, 3);
    clock.t += 30;
    plugin.Render();
    assert.deepEqual(noId(music(net).slice(3)), [golden.rgb_000080_noid, golden.bright_50_noid]);
    // Same colour: nothing.
    clock.t += 40;
    plugin.Render();
    assert.equal(music(net).length, 5);
    device.setColor([0x12, 0x34, 0x56]);
    clock.t += 40;
    plugin.Render();
    assert.deepEqual(noId(music(net).slice(5)), [golden.rgb_123456_noid, golden.bright_34_noid]);
    device.setColor([0, 0, 0]);
    clock.t += 40;
    plugin.Render();
    assert.deepEqual(noId(music(net).slice(7)), [golden.power_off_noid]);
    device.setColor([0x12, 0x34, 0x56]);
    clock.t += 40;
    plugin.Render();
    assert.deepEqual(noId(music(net).slice(8)), [golden.power_on_noid], "only power: colour and brightness are unchanged");
    assert.equal(control(net).length, 2, "nothing else on the control connection");
  } finally {
    restore();
  }
});

test("music mode is capped near 30 commands a second", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    net.incoming(listener(net));
    plugin.Render();
    const base = music(net).length;
    let frames = 0;
    for (let i = 0; i < 60; i++) {
      device.setColor([255, 0, i % 2 ? 255 : 0]);
      clock.t += 16;
      plugin.Render();
    }
    frames = (music(net).length - base);
    // One set_rgb per sent frame (brightness stays 100): at most one frame per 33 ms in ~1 s.
    assert.ok(frames <= 30, `${frames} commands in ~1 s`);
  } finally {
    restore();
  }
});

test("fallback: the light never connects within 5 s -> control connection, 1 command a second", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    clock.t += 4_999;
    plugin.Render();
    assert.equal(control(net).length, 2);
    clock.t += 1;
    plugin.Render();
    assert.ok(listener(net).closed, "the listener is closed");
    assert.deepEqual(noId(control(net).slice(2)), [golden.power_on_noid]);
    clock.t += 500;
    plugin.Render();
    assert.equal(control(net).length, 3);
    clock.t += 500;
    plugin.Render();
    clock.t += 1000;
    plugin.Render();
    assert.deepEqual(noId(control(net).slice(2)), [golden.power_on_noid, golden.rgb_red_noid, golden.bright_100_noid]);
    clock.t += 1000;
    plugin.Render();
    assert.equal(control(net).length, 5, "unchanged: nothing");
    // In 60 s of frames with a change every frame: at most 60 commands.
    const before = control(net).length;
    for (let i = 0; i < 1800; i++) {
      device.setColor([255, i % 255, 0]);
      clock.t += 33;
      plugin.Render();
    }
    assert.ok(control(net).length - before <= 60, `${control(net).length - before} commands in ~60 s`);
    assert.deepEqual(music(net), []);
  } finally {
    restore();
  }
});

test("fallback: listen() returns null -> no set_music, control connection at once", async () => {
  const { net, plugin, device, restore } = await deviceSetup({ color: [255, 0, 0], listen: () => null });
  try {
    plugin.Initialize();
    assert.deepEqual(control(net), [golden.get_prop]);
    plugin.Render();
    assert.deepEqual(noId(control(net).slice(1)), [golden.power_on_noid]);
    assert.ok(device.logs.some((l) => /no local address/.test(l.message)));
    plugin.Shutdown(false);
    assert.equal(noId(control(net)).includes(golden.music_off_noid), false, "set_music 0 only after set_music 1");
  } finally {
    restore();
  }
});

test("fallback: the light drops the music connection", async () => {
  const { net, device, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    const client = net.incoming(listener(net));
    plugin.Render();
    assert.equal(music(net).length, 3);
    client.close();
    device.setColor([0, 0, 0x80]);
    clock.t += 1000;
    plugin.Render();
    assert.deepEqual(noId(control(net).slice(2)), [golden.rgb_000080_noid]);
  } finally {
    restore();
  }
});

test("white-only light: brightness and power only, no set_rgb", async () => {
  const controller = { ...CONTROLLER, model: "mono", support: ["get_prop", "set_power", "set_bright", "set_music"] };
  const { net, device, plugin, restore } = await deviceSetup({ controller, color: [0, 0, 0x80] });
  try {
    plugin.Initialize();
    net.incoming(listener(net));
    plugin.Render();
    assert.deepEqual(noId(music(net)), [golden.power_on_noid, golden.bright_50_noid]);
    device.setColor([0, 0x80, 0]);
    clock.t += 40;
    plugin.Render();
    assert.equal(music(net).length, 2, "same brightness: nothing");
  } finally {
    restore();
  }
});

test("a light without set_music goes straight to the control connection", async () => {
  const controller = { ...CONTROLLER, support: ["get_prop", "set_power", "set_bright", "set_rgb"] };
  const { net, plugin, restore } = await deviceSetup({ controller, color: [255, 0, 0] });
  try {
    plugin.Initialize();
    assert.equal(listener(net), undefined);
    assert.deepEqual(control(net), [golden.get_prop]);
    plugin.Render();
    assert.deepEqual(noId(control(net).slice(1)), [golden.power_on_noid]);
  } finally {
    restore();
  }
});

test("Shutdown leaves music mode and restores rgb and brightness", async () => {
  const { net, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    net.deliver(net.sockets[0], { data: golden.props_reply_rgb });
    const client = net.incoming(listener(net));
    plugin.Render();
    const before = control(net).length;
    plugin.Shutdown(false);
    assert.deepEqual(noId(control(net).slice(before)), [golden.music_off_noid, golden.restore_rgb_noid, golden.restore_bright_80_noid]);
    assert.ok(client.closed && net.sockets[0].closed && listener(net).closed, "every socket is closed");
  } finally {
    restore();
  }
});

test("Shutdown restores a white temperature and turns an off light off again", async () => {
  const { net, plugin, restore } = await deviceSetup({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    // Replies may arrive split across reads.
    const reply = golden.props_reply_ct_off;
    net.deliver(net.sockets[0], { data: reply.slice(0, 10) });
    net.deliver(net.sockets[0], { data: reply.slice(10) });
    net.incoming(listener(net));
    plugin.Render();
    const before = control(net).length;
    plugin.Shutdown(true);
    assert.deepEqual(noId(control(net).slice(before)), [golden.music_off_noid, golden.restore_ct_noid, golden.restore_bright_30_noid, golden.power_off_noid]);
  } finally {
    restore();
  }
});

test("Shutdown after a black frame turns an on light back on before the colour", async () => {
  const { net, plugin, restore } = await deviceSetup({ color: [0, 0, 0] });
  try {
    plugin.Initialize();
    net.deliver(net.sockets[0], { data: golden.props_reply_rgb });
    net.incoming(listener(net));
    plugin.Render();
    const before = control(net).length;
    plugin.Shutdown(false);
    assert.deepEqual(noId(control(net).slice(before)), [golden.music_off_noid, golden.power_on_noid, golden.restore_rgb_noid, golden.restore_bright_80_noid]);
  } finally {
    restore();
  }
});

test("robustness: unreachable light, empty controller, dry-run stub, no get_prop reply", async () => {
  {
    const { net, device, plugin, restore } = await deviceSetup({ netOptions: { unreachableTcp: new Set([`${IP}:55443`]) } });
    try {
      plugin.Initialize();
      plugin.Render();
      clock.t += 30_000;
      plugin.Render();
      plugin.Shutdown(false);
      assert.equal(net.sent.length, 0);
      assert.ok(device.logs.some((l) => /could not connect/.test(l.message)));
    } finally {
      restore();
    }
  }
  for (const controller of [{}, undefined, { id: "x", ip: "127.0.0.1", port: 0, name: "stub", hostname: "", mac: "" }]) {
    const { plugin, restore } = await deviceSetup({ controller });
    try {
      plugin.Initialize();
      plugin.Render();
      clock.t += 6000;
      plugin.Render();
      plugin.Shutdown(false);
    } finally {
      restore();
    }
  }
  const { net, device, plugin, restore } = await deviceSetup();
  try {
    plugin.Initialize();
    net.deliver(net.sockets[0], { data: "garbage\r\n{\"id\":9,\"error\":{\"code\":-1,\"message\":\"quota exceeded\"}}\r\n" });
    plugin.Shutdown(false);
    assert.ok(device.logs.some((l) => /keeps the last colour/.test(l.message)));
    assert.ok(device.logs.some((l) => /quota exceeded/.test(l.message)));
  } finally {
    restore();
  }
});
