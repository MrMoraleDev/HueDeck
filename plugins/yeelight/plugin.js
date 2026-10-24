// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck first-party plugin: Yeelight Wi-Fi bulbs and strips over the "Yeelight WiFi Light
// Inter-Operation Specification" (the user turns on "LAN Control" for the light in the Yeelight
// app). JSON commands, one per line ("\r\n"), on TCP port 55443.
//
// Status: UNTESTED. Nobody working on HueDeck owns a Yeelight; everything is written from the
// public specification only.
//
// Discovery (service context): an SSDP-like M-SEARCH ("ST: wifi_bulb") to 239.255.255.250:1982
// at start and every 60 s; replies and the lights' own NOTIFY advertisements carry
// "Location: yeelight://ip:port", id, model, support, name, power, ... in HTTP-like header lines.
// Device: a control connection (get_prop at Initialize for Shutdown), then music mode: HueDeck
// listens on the LAN address facing the light (tcp listen(0, ip), a HueDeck extension), asks the
// light to connect with set_music, and streams set_rgb / set_bright on that connection (no
// command quota there; capped here at ~30/s and only on change). Without music mode (no listen
// address, the light never connects within 5 s, or it drops the connection) the control
// connection is used at 1 command a second: lights allow 60 commands a minute.
// White-only lights (no set_rgb in "support") get brightness and power only.
// Protocol notes and sources: docs/hardware/yeelight.md.

/* global device, service, controller, discovery */

import tcp from "@SignalRGB/tcp";

export function Name() { return "Yeelight"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/yeelight"; }
export function Type() { return "network"; }
// HueDeck extension (docs/host-api/save-shutdown.md): Shutdown() hands the light back to its
// own state ("restore"); for "keep" and "off" the host falls back to that too.
export function ShutdownModes() { return ["restore"]; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Light"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
export function ControllableParameters() { return []; }

const MULTICAST = "239.255.255.250";
const SSDP_PORT = 1982;
const CONTROL_PORT = 55443;
const SEARCH = `M-SEARCH * HTTP/1.1\r\nHOST: ${MULTICAST}:${SSDP_PORT}\r\nMAN: "ssdp:discover"\r\nST: wifi_bulb\r\n`;
const SEARCH_INTERVAL_MS = 60000;
const STALE_MS = 3 * SEARCH_INTERVAL_MS + 5000;

const MUSIC_WAIT_MS = 5000;
const MUSIC_INTERVAL_MS = 33;
const CONTROL_INTERVAL_MS = 1000;
const RECONNECT_MS = 30000;
const PROPS = ["power", "bright", "rgb", "color_mode", "ct"];

const FORCED_ID = "forced";
const FORCED_KEY = "ips";

function now() { return Date.now(); }

function safeId(text) {
  return String(text).replace(/[^0-9A-Za-z_-]/g, "-");
}

/** Header lines of a search reply / NOTIFY ("Key: value", keys lower-cased), or null. */
function parseAdvert(text) {
  const lines = String(text ?? "").split(/\r?\n/);
  const first = lines[0] ?? "";
  if (!/^HTTP\/1\.1 200/i.test(first) && !/^NOTIFY /i.test(first)) {
    return null;
  }
  const h = {};
  for (const line of lines.slice(1)) {
    const i = line.indexOf(":");
    if (i > 0) {
      h[line.slice(0, i).trim().toLowerCase()] = line.slice(i + 1).trim();
    }
  }
  const loc = /^yeelight:\/\/([^:/]+):(\d+)/i.exec(h.location ?? "");
  if (!loc || !h.id) {
    return null;
  }
  return { ...h, ip: loc[1], port: Number(loc[2]) };
}

// ---------------------------------------------------------------------------------------------
// Service context
// ---------------------------------------------------------------------------------------------

class YeelightController {
  constructor(a) {
    this.id = safeId(a.id);
    this.name = a.name ? a.name : `Yeelight ${a.model || ""}`.trim();
    this.ip = a.ip;
    this.port = a.port || CONTROL_PORT;
    this.model = a.model ?? "";
    this.fw_ver = a.fw_ver ?? "";
    this.support = String(a.support ?? "").split(/\s+/).filter((s) => s.length > 0);
    this.forced = false;
    this.status = a.power ? `power ${a.power}` : "online";
  }
}

class ForcedController {
  constructor(ip) {
    this.id = `forced-${safeId(ip)}`;
    this.name = `Yeelight ${ip}`;
    this.ip = ip;
    this.port = CONTROL_PORT;
    this.model = "";
    this.fw_ver = "";
    // Unknown: the device session assumes a colour light that supports music mode.
    this.support = [];
    this.forced = true;
    this.status = "added by IP";
  }
}

function sameData(a, b) {
  return a.ip === b.ip && a.port === b.port && a.model === b.model && a.name === b.name &&
    a.support.join(" ") === b.support.join(" ");
}

export class DiscoveryService {
  constructor() {
    this.IconUrl = "";
    this.UdpBroadcastAddress = MULTICAST;
    this.UdpBroadcastPort = SSDP_PORT;
    this.lastSearch = 0;
    this.forced = [];
    this.seen = {};
  }

  Initialize() {
    this.forced = this.loadForced();
    for (const ip of this.forced) {
      this.addForced(ip);
    }
    this.search();
  }

  search() {
    this.lastSearch = now();
    service.broadcast(SEARCH);
  }

  Update() {
    const t = now();
    if (t - this.lastSearch >= SEARCH_INTERVAL_MS) {
      this.search();
    }
    for (const wrapper of [...service.controllers]) {
      const obj = wrapper.obj ?? wrapper;
      if (obj.forced) {
        continue;
      }
      if (t - (this.seen[obj.id] ?? 0) > STALE_MS) {
        service.log(`Yeelight: ${obj.name} (${obj.ip}) stopped answering, removed`);
        service.suppressController(obj);
        service.removeController(obj);
        delete this.seen[obj.id];
        if (this.forced.includes(obj.ip)) {
          this.addForced(obj.ip);
        }
      }
    }
  }

  Discovered(value) {
    const a = parseAdvert(value?.response);
    if (!a) {
      // Our own M-SEARCH looped back, other SSDP traffic: ignored.
      return;
    }
    const fresh = new YeelightController(a);
    this.seen[fresh.id] = now();
    // A light found by discovery replaces the one added by hand at the same IP.
    for (const wrapper of [...service.controllers]) {
      const obj = wrapper.obj ?? wrapper;
      if (obj.forced && obj.ip === fresh.ip) {
        service.suppressController(obj);
        service.removeController(obj);
      }
    }
    fresh.forced = false;
    const existing = service.getController(fresh.id);
    if (existing) {
      existing.status = fresh.status;
      if (!sameData(existing, fresh)) {
        Object.assign(existing, { ip: fresh.ip, port: fresh.port, model: fresh.model, name: fresh.name, support: fresh.support, fw_ver: fresh.fw_ver });
        service.announceController(existing);
      }
      service.updateController(existing);
      return;
    }
    service.log(`Yeelight: found ${fresh.name} (${fresh.model}, ${fresh.id}) at ${fresh.ip}`);
    service.addController(fresh);
    service.announceController(fresh);
  }

  addForced(ip) {
    if (service.controllers.some((c) => (c.obj ?? c).ip === ip)) {
      return;
    }
    const obj = new ForcedController(ip);
    service.addController(obj);
    service.announceController(obj);
  }

  forceDiscover(ip) {
    const addr = String(ip ?? "").trim();
    if (!/^[0-9A-Za-z.:-]+$/.test(addr)) {
      service.log(`Yeelight: '${addr}' is not an IP address`);
      return;
    }
    if (!this.forced.includes(addr)) {
      this.forced.push(addr);
      this.saveForced();
    }
    this.addForced(addr);
  }

  forgetController(id) {
    const obj = service.getController(id);
    if (!obj) {
      return;
    }
    if (this.forced.includes(obj.ip)) {
      this.forced = this.forced.filter((ip) => ip !== obj.ip);
      this.saveForced();
    }
    service.suppressController(obj);
    service.removeController(obj);
    delete this.seen[obj.id];
  }

  loadForced() {
    try {
      const list = JSON.parse(service.getSetting(FORCED_ID, FORCED_KEY) ?? "[]");
      return Array.isArray(list) ? list.filter((ip) => typeof ip === "string" && ip.length > 0) : [];
    } catch {
      return [];
    }
  }

  saveForced() {
    service.saveSetting(FORCED_ID, FORCED_KEY, JSON.stringify(this.forced));
  }
}

// ---------------------------------------------------------------------------------------------
// Device context
// ---------------------------------------------------------------------------------------------

let target = null;
let support = null;
let ctrl = null;
let ctrlOk = false;
let lastConnectTry = 0;
let server = null;
let music = null;
let musicRequested = false;
let mode = "off"; // "off" | "waiting" | "music" | "fallback"
let musicDeadline = 0;
let nextId = 1;
let getPropId = 0;
let rx = "";
let saved = null;
let last = { power: null, rgb: null, bright: null };
let lastSendAt = -Infinity;
let failureLogged = false;

function log(text) {
  try {
    device.log(`Yeelight: ${text}`);
  } catch {
    // never throw from logging
  }
}

function supports(method) {
  return !support || support.includes(method);
}

function command(method, params) {
  const id = nextId++;
  return { id, text: `${JSON.stringify({ id, method, params })}\r\n` };
}

function sendOn(sock, method, params) {
  const c = command(method, params);
  let sent = -1;
  try {
    sent = sock ? sock.send(c.text) : -1;
  } catch {
    sent = -1;
  }
  if (sent < 0 && !failureLogged) {
    failureLogged = true;
    log(`${method} could not be sent (further failures are not logged)`);
  }
  return sent >= 0 ? c.id : 0;
}

function onControlData(msg) {
  rx += String(msg?.data ?? "");
  const lines = rx.split("\r\n");
  rx = lines.pop() ?? "";
  if (rx.length > 4096) {
    rx = "";
  }
  for (const line of lines) {
    let reply = null;
    try {
      reply = JSON.parse(line);
    } catch {
      continue;
    }
    if (reply && reply.id === getPropId && Array.isArray(reply.result) && !saved) {
      const r = reply.result;
      saved = {
        power: r[0] === "off" ? "off" : "on",
        bright: Number(r[1]) || 0,
        rgb: r[2] === "" || r[2] === undefined ? null : Number(r[2]),
        colorMode: Number(r[3]) || 0,
        ct: Number(r[4]) || 0,
      };
    } else if (reply && reply.error) {
      log(`error from the light: ${JSON.stringify(reply.error)}`);
    }
    // "props" notifications are ignored.
  }
}

function connectControl() {
  lastConnectTry = now();
  rx = "";
  try {
    ctrl = tcp.createSocket();
    ctrl.on("message", onControlData);
    ctrl.on("error", (code, text) => log(`control connection error ${code} ${text ?? ""}`));
    ctrl.on("close", () => { ctrlOk = false; });
    ctrlOk = ctrl.connect(target.ip, target.port) === true;
  } catch (e) {
    log(`control connection failed: ${e}`);
    ctrlOk = false;
  }
  if (!ctrlOk) {
    log(`could not connect to ${target.ip}:${target.port} (is LAN Control on?)`);
  }
  return ctrlOk;
}

function closeQuietly(sock) {
  try {
    sock?.close();
  } catch {
    // already closed
  }
}

function startFallback(reason) {
  log(`${reason}; using the control connection at 1 command a second`);
  closeQuietly(server);
  server = null;
  mode = "fallback";
}

function startMusic() {
  if (!supports("set_music")) {
    startFallback("no music mode on this light");
    return;
  }
  let at = null;
  try {
    server = tcp.createSocket();
    server.on("connection", (client) => {
      if (!client || music) {
        return;
      }
      music = client;
      client.on("close", () => {
        if (music === client) {
          music = null;
          if (mode === "music") {
            startFallback("the light closed the music connection");
          }
        }
      });
      mode = "music";
      last = { power: null, rgb: null, bright: null };
      lastSendAt = -Infinity;
    });
    at = server.listen(0, target.ip);
  } catch (e) {
    log(`listen failed: ${e}`);
    at = null;
  }
  if (!at || !at.address || !at.port) {
    startFallback("no local address to listen on for music mode");
    return;
  }
  if (!sendOn(ctrl, "set_music", [1, at.address, at.port])) {
    startFallback("set_music could not be sent");
    return;
  }
  musicRequested = true;
  mode = "waiting";
  musicDeadline = now() + MUSIC_WAIT_MS;
}

export function Initialize() {
  const c = typeof controller === "object" && controller ? controller : {};
  target = c.ip ? { ip: String(c.ip), port: Number(c.port) || CONTROL_PORT } : null;
  support = Array.isArray(c.support) && c.support.length > 0 ? c.support.map(String) : null;
  ctrl = null;
  ctrlOk = false;
  server = null;
  music = null;
  musicRequested = false;
  mode = "off";
  nextId = 1;
  getPropId = 0;
  saved = null;
  last = { power: null, rgb: null, bright: null };
  lastSendAt = -Infinity;
  failureLogged = false;
  try {
    device.setName(c.name || "Yeelight");
    device.setSize([1, 1]);
    device.setControllableLeds(["Light"], [[0, 0]]);
  } catch (e) {
    log(`layout failed: ${e}`);
  }
  if (!target) {
    log("the controller has no IP address; nothing will be sent");
    return;
  }
  if (!supports("set_rgb")) {
    log("white-only light: brightness and power only");
  }
  if (!connectControl()) {
    return;
  }
  getPropId = sendOn(ctrl, "get_prop", PROPS);
  startMusic();
}

/** What the light should show for a canvas colour: power, the colour at full value, brightness. */
function wanted(r, g, b) {
  const max = Math.max(r, g, b);
  if (max === 0) {
    return { power: "off", rgb: null, bright: null };
  }
  const k = 255 / max;
  const rgb = (Math.round(r * k) << 16) | (Math.round(g * k) << 8) | Math.round(b * k);
  return { power: "on", rgb: supports("set_rgb") ? rgb : null, bright: Math.max(1, Math.round((max * 100) / 255)) };
}

/** The commands that take the light from `last` to `want`, in order. */
function diff(want) {
  const out = [];
  if (want.power === "off") {
    if (last.power !== "off") {
      out.push(["set_power", ["off", "sudden", 0], { power: "off" }]);
    }
    return out;
  }
  if (last.power !== "on") {
    out.push(["set_power", ["on", "sudden", 0], { power: "on" }]);
  }
  if (want.rgb !== null && want.rgb !== last.rgb) {
    out.push(["set_rgb", [want.rgb, "sudden", 0], { rgb: want.rgb }]);
  }
  if (want.bright !== last.bright && supports("set_bright")) {
    out.push(["set_bright", [want.bright, "sudden", 0], { bright: want.bright }]);
  }
  return out;
}

export function Render() {
  if (!target) {
    return;
  }
  const t = now();
  if (mode === "off") {
    if (t - lastConnectTry >= RECONNECT_MS && connectControl()) {
      getPropId = getPropId || sendOn(ctrl, "get_prop", PROPS);
      startMusic();
    }
    return;
  }
  if (mode === "waiting") {
    if (t < musicDeadline) {
      return;
    }
    startFallback("the light did not open the music connection");
  }
  const interval = mode === "music" ? MUSIC_INTERVAL_MS : CONTROL_INTERVAL_MS;
  if (t - lastSendAt < interval) {
    return;
  }
  const c = device.color(0, 0);
  const steps = diff(wanted(c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF));
  if (steps.length === 0) {
    return;
  }
  if (mode === "music") {
    for (const [method, params, change] of steps) {
      if (sendOn(music, method, params)) {
        Object.assign(last, change);
      }
    }
  } else {
    if (!ctrlOk) {
      return;
    }
    // One command per second: the rest follows on the next turns.
    const [method, params, change] = steps[0];
    if (sendOn(ctrl, method, params)) {
      Object.assign(last, change);
    }
  }
  lastSendAt = t;
}

// Leave music mode and put back what get_prop reported (colour or white temperature,
// brightness, power). Without that reply the light keeps the last colour.
export function Shutdown(_suspend) {
  const m = music;
  music = null;
  mode = "off";
  closeQuietly(m);
  closeQuietly(server);
  server = null;
  if (target && (ctrlOk || ((musicRequested || saved) && connectControl()))) {
    if (musicRequested) {
      sendOn(ctrl, "set_music", [0]);
    }
    if (saved) {
      if (saved.power === "on" && last.power === "off") {
        sendOn(ctrl, "set_power", ["on", "sudden", 0]);
      }
      if (saved.power === "on" || last.power !== "off") {
        if (saved.colorMode === 2 && saved.ct > 0 && supports("set_ct_abx")) {
          sendOn(ctrl, "set_ct_abx", [saved.ct, "sudden", 0]);
        } else if (saved.rgb !== null && supports("set_rgb")) {
          sendOn(ctrl, "set_rgb", [saved.rgb, "sudden", 0]);
        }
        if (saved.bright > 0 && supports("set_bright")) {
          sendOn(ctrl, "set_bright", [saved.bright, "sudden", 0]);
        }
      }
      if (saved.power === "off") {
        sendOn(ctrl, "set_power", ["off", "sudden", 0]);
      }
    } else {
      log("no get_prop reply was received; the light keeps the last colour");
    }
  }
  closeQuietly(ctrl);
  ctrl = null;
  ctrlOk = false;
}
