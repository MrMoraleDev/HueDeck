// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: WLED controllers (https://kno.wled.ge) on the LAN.
// Written from WLED's public documentation: the JSON API (`GET /json/info`,
// https://kno.wled.ge/interfaces/json-api/) and the UDP realtime protocol
// (https://kno.wled.ge/interfaces/udp-realtime/). No SignalRGB or OpenRGB code is used.
//
// Status: UNTESTED. Nobody on the project owns a WLED controller; every packet below is
// derived from the public documentation and checked only against the mock net.
//
// Discovery (service context): mDNS `_wled._tcp.local.` answers and the IPs added by hand
// ("Add by IP", persisted in the plugin settings). Each candidate is asked for `/json/info`
// (asynchronous XHR); it is accepted when the answer looks like WLED (`leds.count` and `ver`).
// No pairing.
//
// Streaming (device context): one strip of `ledCount` LEDs. Each frame goes out as WLED UDP
// realtime packets: DRGB (protocol 2, all LEDs in one datagram) up to 490 LEDs, DNRGB
// (protocol 4, a 16-bit start index and at most 489 LEDs per datagram) above. Byte 1 is the
// timeout: 2 s, so WLED goes back to its own effect 2 s after Lightloom stops sending. A frame is
// sent when the colours changed, and at least once a second as a keep-alive. Shutdown sends the
// last colours once more with a 1 s timeout and stops; WLED's saved state is never touched.
// Details: docs/hardware/wled.md.

/* global device, service, controller, discovery, udp, XMLHttpRequest */

export function Name() { return "WLED"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/wled"; }
export function Type() { return "network"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
export function ControllableParameters() { return []; }

// --- protocol constants (kno.wled.ge/interfaces/udp-realtime) -------------------------------

export const DEFAULT_UDP_PORT = 21324;
const PROTOCOL_DRGB = 2;
const PROTOCOL_DNRGB = 4;
const DRGB_MAX_LEDS = 490;
const DNRGB_MAX_LEDS = 489;
// Seconds WLED waits after the last packet before it returns to its own effect.
const STREAM_TIMEOUT_S = 2;
const SHUTDOWN_TIMEOUT_S = 1;
// Keep-alive well inside the 2 s timeout when the picture does not change.
const KEEPALIVE_MS = 1000;
// Sanity cap: WLED builds for ESP32 drive a few thousand LEDs at most; a larger count is
// almost certainly a bad answer, and 1500 LEDs already take 4 datagrams per frame.
export const MAX_LEDS = 1500;

// --- service context: discovery ---------------------------------------------------------------

const SETTINGS_ID = "wled";
const SETTINGS_FORCED = "forced";
const RETRY_MS = 30000;
const PROBE_TIMEOUT_MS = 3000;

function isIpv4(ip) {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(String(ip ?? ""));
  return m !== null && m.slice(1).every((o) => Number(o) <= 255);
}

function loadForced() {
  try {
    const raw = service.getSetting(SETTINGS_ID, SETTINGS_FORCED);
    const list = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list.filter(isIpv4) : [];
  } catch (e) {
    service.log(`WLED: ignoring unreadable forced IP list (${e})`);
    return [];
  }
}

function saveForced(list) {
  if (list.length === 0) {
    service.removeSetting(SETTINGS_ID, SETTINGS_FORCED);
  } else {
    service.saveSetting(SETTINGS_ID, SETTINGS_FORCED, JSON.stringify(list));
  }
}

/** Does a `/json/info` answer look like WLED (or a WLED-compatible firmware)? */
export function looksLikeWled(info) {
  return info !== null && typeof info === "object" && info.leds !== null && typeof info.leds === "object" &&
    typeof info.leds.count === "number" && typeof info.ver === "string";
}

function normaliseMac(mac) {
  const hex = String(mac ?? "").toLowerCase().replace(/[^0-9a-f]/g, "");
  return hex.length === 12 && hex !== "000000000000" ? hex : "";
}

/** The plain data a device context gets for one WLED controller. */
export class WledController {
  constructor(ip, info) {
    const mac = normaliseMac(info.mac);
    this.id = mac || ip;
    this.mac = mac;
    this.ip = ip;
    this.name = typeof info.name === "string" && info.name !== "" ? info.name : `WLED ${ip}`;
    this.ledCount = Math.max(0, Math.floor(info.leds.count));
    const port = Number(info.udpport);
    this.udpPort = Number.isInteger(port) && port > 0 && port < 65536 ? port : DEFAULT_UDP_PORT;
    this.version = info.ver;
    this.brand = typeof info.brand === "string" ? info.brand : "";
    // Documentation only: WLED fills the W channel itself from RGB packets.
    this.rgbw = info.leds.rgbw === true;
    this.lightCapabilities = typeof info.leds.lc === "number" ? info.leds.lc : null;
    this.status = "";
  }
}

function sameData(a, b) {
  return a.ip === b.ip && a.name === b.name && a.ledCount === b.ledCount && a.udpPort === b.udpPort;
}

export function DiscoveryService() {
  this.IconUrl = "";
  this.MDns = ["_wled._tcp.local."];
  this.forced = [];
  this.probes = {};        // ip -> { pending, lastFail }
  this.ips = {};           // ip -> controller id

  this.Initialize = function () {
    this.forced = loadForced();
    for (const ip of this.forced) {
      this.probe(ip, "added by IP");
    }
  };

  this.Update = function () {
    const now = Date.now();
    for (const ip of this.forced) {
      const p = this.probes[ip];
      if (!this.ips[ip] && (!p || (!p.pending && now - p.lastFail >= RETRY_MS))) {
        this.probe(ip, "added by IP (retry)");
      }
    }
  };

  this.Discovered = function (value) {
    const ip = value?.ip;
    if (!isIpv4(ip)) {
      return;
    }
    const p = this.probes[ip];
    if (this.ips[ip] || p?.pending || (p && Date.now() - p.lastFail < RETRY_MS)) {
      return;
    }
    this.probe(ip, `mDNS ${value.name ?? value.hostname ?? ""}`.trim());
  };

  this.Removal = function (value) {
    const ip = value?.ip;
    const id = this.ips[ip];
    if (!id || this.forced.includes(ip)) {
      return;
    }
    const obj = service.getController(id);
    service.log(`WLED: ${ip} stopped answering mDNS, removing it`);
    if (obj) {
      service.suppressController(obj);
      service.removeController(obj);
    }
    delete this.ips[ip];
  };

  this.probe = function (ip, why) {
    const state = this.probes[ip] ?? (this.probes[ip] = { pending: false, lastFail: 0 });
    state.pending = true;
    const xhr = new XMLHttpRequest();
    xhr.timeout = PROBE_TIMEOUT_MS;
    xhr.onreadystatechange = () => {
      if (xhr.readyState !== 4) {
        return;
      }
      state.pending = false;
      let info = null;
      if (xhr.status === 200) {
        try {
          info = JSON.parse(xhr.responseText);
        } catch {
          info = null;
        }
      }
      if (!looksLikeWled(info)) {
        state.lastFail = Date.now();
        service.log(`WLED: ${ip} (${why}) did not answer /json/info like WLED (status ${xhr.status})`);
        this.markForcedOffline(ip);
        return;
      }
      this.accept(ip, info);
    };
    try {
      xhr.open("GET", `http://${ip}/json/info`, true);
      xhr.send();
    } catch (e) {
      state.pending = false;
      state.lastFail = Date.now();
      service.log(`WLED: request to ${ip} failed (${e})`);
    }
  };

  this.markForcedOffline = function (ip) {
    if (!this.forced.includes(ip)) {
      return;
    }
    const id = `ip-${ip}`;
    if (!service.hasController(id)) {
      service.addController({ id, name: `WLED ${ip}`, ip, status: "not answering" });
    }
  };

  this.accept = function (ip, info) {
    const fresh = new WledController(ip, info);
    if (info.brand !== undefined && info.brand !== "WLED") {
      service.log(`WLED: ${ip} reports brand '${info.brand}', treating it as WLED-compatible`);
    }
    if (fresh.ledCount > MAX_LEDS) {
      service.log(`WLED: ${ip} reports ${fresh.ledCount} LEDs, more than Lightloom drives; using the first ${MAX_LEDS}`);
      fresh.ledCount = MAX_LEDS;
    }
    // A placeholder row shown while a forced IP did not answer.
    const placeholder = service.getController(`ip-${ip}`);
    if (placeholder && fresh.id !== placeholder.id) {
      service.removeController(placeholder);
    }
    // The same controller at a new address (DHCP): forget the old address.
    for (const [oldIp, id] of Object.entries(this.ips)) {
      if (id === fresh.id && oldIp !== ip) {
        delete this.ips[oldIp];
      }
    }
    this.ips[ip] = fresh.id;
    const known = service.getController(fresh.id);
    if (fresh.ledCount < 1) {
      fresh.status = "no LEDs configured";
      service.log(`WLED: ${fresh.name} (${ip}) has no LEDs configured; not streaming to it`);
      if (known) {
        Object.assign(known, fresh);
        service.updateController(known);
      } else {
        service.addController(fresh);
      }
      return;
    }
    fresh.status = `v${fresh.version}, ${fresh.ledCount} LEDs`;
    if (known && sameData(known, fresh)) {
      return;
    }
    if (known) {
      Object.assign(known, fresh);
      service.updateController(known);
      service.announceController(known);
      return;
    }
    service.log(`WLED: found ${fresh.name} at ${ip} (${fresh.ledCount} LEDs, version ${fresh.version})`);
    service.addController(fresh);
    service.announceController(fresh);
  };

  this.forceDiscover = function (ip) {
    const addr = String(ip ?? "").trim();
    if (!isIpv4(addr)) {
      service.log(`WLED: '${addr}' is not an IPv4 address`);
      return;
    }
    if (!this.forced.includes(addr)) {
      this.forced.push(addr);
      saveForced(this.forced);
    }
    delete this.probes[addr];
    this.probe(addr, "added by IP");
  };

  this.forgetController = function (id) {
    const obj = service.getController(id);
    const ip = obj?.ip ?? Object.keys(this.ips).find((k) => this.ips[k] === id);
    if (ip && this.forced.includes(ip)) {
      this.forced = this.forced.filter((f) => f !== ip);
      saveForced(this.forced);
    }
    if (ip) {
      delete this.ips[ip];
      delete this.probes[ip];
    }
    if (obj) {
      service.suppressController(obj);
      service.removeController(obj);
    }
  };
}

// --- device context: streaming ----------------------------------------------------------------

let ledCount = 0;
let target = null;          // { ip, port }
let last = null;            // last colours sent, flat RGB
let lastSentAt = 0;
let sendFailLogged = false;

/**
 * WLED UDP realtime datagrams for one frame of flat RGB bytes: DRGB up to 490 LEDs, DNRGB
 * (start index big-endian, at most 489 LEDs each) above.
 */
export function buildPackets(rgb, timeout) {
  const count = Math.floor(rgb.length / 3);
  if (count <= DRGB_MAX_LEDS) {
    return [[PROTOCOL_DRGB, timeout & 0xFF, ...rgb.slice(0, count * 3)]];
  }
  const packets = [];
  for (let start = 0; start < count; start += DNRGB_MAX_LEDS) {
    const n = Math.min(DNRGB_MAX_LEDS, count - start);
    packets.push([PROTOCOL_DNRGB, timeout & 0xFF, (start >> 8) & 0xFF, start & 0xFF, ...rgb.slice(start * 3, (start + n) * 3)]);
  }
  return packets;
}

function send(rgb, timeout) {
  if (!target || typeof udp === "undefined" || typeof udp.send !== "function") {
    return;
  }
  for (const packet of buildPackets(rgb, timeout)) {
    let sent = -1;
    try {
      sent = udp.send(target.ip, target.port, packet, "big");
    } catch (e) {
      sent = -1;
    }
    if (sent < 0 && !sendFailLogged) {
      sendFailLogged = true;
      device.log(`WLED: sending to ${target.ip}:${target.port} failed (further failures are not logged)`);
    }
  }
  lastSentAt = Date.now();
}

export function Initialize() {
  const c = typeof controller === "object" && controller !== null ? controller : {};
  device.setName(typeof c.name === "string" && c.name !== "" ? c.name : "WLED");
  const count = Number(c.ledCount);
  ledCount = Number.isFinite(count) ? Math.min(MAX_LEDS, Math.max(0, Math.floor(count))) : 0;
  if (Number.isFinite(count) && count > MAX_LEDS) {
    device.log(`WLED: ${count} LEDs reported, driving the first ${MAX_LEDS}`);
  }
  const port = Number(c.udpPort);
  target = typeof c.ip === "string" && c.ip !== "" ?
    { ip: c.ip, port: Number.isInteger(port) && port > 0 && port < 65536 ? port : DEFAULT_UDP_PORT } : null;
  last = null;
  lastSentAt = 0;
  sendFailLogged = false;
  device.addFeature("udp");
  if (ledCount < 1 || !target) {
    device.log("WLED: controller has no LED count or address; nothing will be sent");
    ledCount = 0;
    device.setSize([1, 1]);
    device.setControllableLeds([], []);
    return;
  }
  const names = [];
  const positions = [];
  for (let i = 0; i < ledCount; i++) {
    names.push(`LED ${i + 1}`);
    positions.push([i, 0]);
  }
  device.setSize([ledCount, 1]);
  device.setControllableLeds(names, positions);
}

function frame() {
  const rgb = new Array(ledCount * 3);
  for (let i = 0; i < ledCount; i++) {
    const c = device.color(i, 0);
    rgb[i * 3] = c[0] & 0xFF;
    rgb[i * 3 + 1] = c[1] & 0xFF;
    rgb[i * 3 + 2] = c[2] & 0xFF;
  }
  return rgb;
}

function equal(a, b) {
  if (!a || a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}

export function Render() {
  if (ledCount < 1) {
    return;
  }
  const rgb = frame();
  if (equal(last, rgb) && Date.now() - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  last = rgb;
  send(rgb, STREAM_TIMEOUT_S);
}

// Same on exit and on suspend: one last packet with a 1 s timeout, so WLED shows the last frame
// briefly and then returns to its own (saved) effect. Nothing else is changed on the controller.
export function Shutdown() {
  if (ledCount > 0 && last) {
    send(last, SHUTDOWN_TIMEOUT_S);
  }
  last = null;
}
