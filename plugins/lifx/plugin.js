// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: LIFX Wi-Fi bulbs over the public LIFX LAN protocol
// (https://lan.developer.lifx.com/docs), binary messages on UDP port 56700.
//
// Status: UNTESTED. Nobody working on Lightloom owns a LIFX bulb; every packet is worked out from
// the public documentation only.
//
// Discovery (service context): GetService (2) broadcast to 255.255.255.255:56700 at start and
// every 30 s (unicast to every IP added by hand too); StateService (3) replies give the bulb's
// MAC (the header's target) and UDP port. GetLabel (23) / GetVersion (32) are then asked from a
// service socket for the name; the bulb is announced when the label arrives (or after 3 s).
// Device: one bulb = one LED. Get (101) at Initialize keeps the LightState (107) for Shutdown,
// SetLightPower (117) turns it on, then SetColor (102) HSBK per frame: at most 20 messages a
// second (LIFX guidance), only on change. Multizone strips and matrix tiles get one colour.
// Protocol notes and sources: docs/hardware/lifx.md.

/* global device, service, controller, discovery */

import udp from "@SignalRGB/udp";

export function Name() { return "LIFX"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/lifx"; }
export function Type() { return "network"; }
// Lightloom extension (docs/host-api/save-shutdown.md): Shutdown() hands the light back to its
// own state ("restore"); for "keep" and "off" the host falls back to that too.
export function ShutdownModes() { return ["restore"]; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Bulb"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
export function ControllableParameters() { return []; }

const PORT = 56700;
const PROTOCOL = 1024;
// Our client id, echoed in replies ("hdek" in little-endian bytes); never 0 or 1 (the docs warn
// some firmware treats those specially).
const SOURCE = 0x6B656468;

// Message types (LIFX LAN docs: Discovery, Device, Light messages).
const GET_SERVICE = 2;
const STATE_SERVICE = 3;
const GET_LABEL = 23;
const STATE_LABEL = 25;
const GET_VERSION = 32;
const STATE_VERSION = 33;
const LIGHT_GET = 101;
const LIGHT_SET_COLOR = 102;
const LIGHT_STATE = 107;
const LIGHT_SET_POWER = 117;

const SERVICE_UDP = 1;
const HEADER_SIZE = 36;

const DISCOVERY_INTERVAL_MS = 30000;
const STALE_MS = 3 * DISCOVERY_INTERVAL_MS + 5000;
const LABEL_WAIT_MS = 3000;
// LIFX: no more than 20 messages per second per device.
const MIN_INTERVAL_MS = 50;
// Transition of each SetColor: one frame interval, so the stream looks smooth.
const DURATION_MS = 50;
const KELVIN = 3500;

const FORCED_ID = "forced";
const FORCED_KEY = "ips";

function now() { return Date.now(); }

function u16(out, v) { out.push(v & 0xFF, (v >>> 8) & 0xFF); }
function u32(out, v) { out.push(v & 0xFF, (v >>> 8) & 0xFF, (v >>> 16) & 0xFF, (v >>> 24) & 0xFF); }
function readU16(b, o) { return (b[o] | (b[o + 1] << 8)) >>> 0; }
function readU32(b, o) { return (b[o] | (b[o + 1] << 8) | (b[o + 2] << 16) | (b[o + 3] << 24)) >>> 0; }

/** "d073d5010203" -> [0xD0, 0x73, 0xD5, 0x01, 0x02, 0x03]; anything else -> all zeros. */
function macBytes(mac) {
  const hex = String(mac ?? "").replace(/[^0-9a-fA-F]/g, "");
  if (hex.length !== 12) {
    return [0, 0, 0, 0, 0, 0];
  }
  const out = [];
  for (let i = 0; i < 12; i += 2) {
    out.push(parseInt(hex.slice(i, i + 2), 16));
  }
  return out;
}

/**
 * One LIFX message: frame header (size, protocol 1024 | addressable | tagged, source), frame
 * address (target MAC + 2 zero bytes, 6 reserved, flags, sequence), protocol header (8 reserved,
 * type, 2 reserved), payload. Little endian. Flags (res_required/ack_required) stay 0: Get
 * messages answer anyway and streamed Set messages need no acknowledgement.
 */
function message(type, payload, { mac = null, tagged = false, sequence = 0 } = {}) {
  const out = [];
  u16(out, HEADER_SIZE + payload.length);
  u16(out, PROTOCOL | 0x1000 | (tagged ? 0x2000 : 0));
  u32(out, SOURCE);
  out.push(...(mac ? macBytes(mac) : [0, 0, 0, 0, 0, 0]), 0, 0);
  out.push(0, 0, 0, 0, 0, 0);
  out.push(0, sequence & 0xFF);
  out.push(0, 0, 0, 0, 0, 0, 0, 0);
  u16(out, type);
  out.push(0, 0);
  out.push(...payload);
  return out;
}

/** Header fields of a received message, or null when it is not a LIFX message. */
function parse(buffer) {
  const b = Array.isArray(buffer) ? buffer : Array.from(buffer ?? []);
  if (b.length < HEADER_SIZE) {
    return null;
  }
  const size = readU16(b, 0);
  if ((readU16(b, 2) & 0x0FFF) !== PROTOCOL || size < HEADER_SIZE || size > b.length) {
    return null;
  }
  const mac = b.slice(8, 14).map((x) => x.toString(16).padStart(2, "0")).join("");
  return { size, source: readU32(b, 4), mac, sequence: b[23], type: readU16(b, 32), payload: b.slice(HEADER_SIZE, size) };
}

/** UTF-8 text of a NUL-padded byte field (no TextDecoder in QuickJS). */
function utf8(bytes) {
  let s = "";
  for (let i = 0; i < bytes.length; i++) {
    let c = bytes[i];
    if (c === 0) {
      break;
    }
    if (c >= 0xC0 && c < 0xE0 && i + 1 < bytes.length) {
      c = ((c & 0x1F) << 6) | (bytes[++i] & 0x3F);
    } else if (c >= 0xE0 && c < 0xF0 && i + 2 < bytes.length) {
      c = ((c & 0x0F) << 12) | ((bytes[++i] & 0x3F) << 6) | (bytes[++i] & 0x3F);
    } else if (c >= 0xF0 && i + 3 < bytes.length) {
      c = ((c & 0x07) << 18) | ((bytes[++i] & 0x3F) << 12) | ((bytes[++i] & 0x3F) << 6) | (bytes[++i] & 0x3F);
    }
    s += String.fromCodePoint(c);
  }
  return s.trim();
}

/** RGB -> HSBK, each 0..65535 (hue 0..65535 = 0..360 degrees), kelvin 3500. */
function hsbk(r, g, b) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === r) {
      h = 60 * (((g - b) / d) % 6);
    } else if (max === g) {
      h = 60 * ((b - r) / d + 2);
    } else {
      h = 60 * ((r - g) / d + 4);
    }
  }
  if (h < 0) {
    h += 360;
  }
  return {
    hue: Math.round((h * 65535) / 360) % 65536,
    saturation: max === 0 ? 0 : Math.round((d / max) * 65535),
    brightness: Math.round((max / 255) * 65535),
    kelvin: KELVIN,
  };
}

function setColorPayload(c, duration) {
  const out = [0];
  u16(out, c.hue);
  u16(out, c.saturation);
  u16(out, c.brightness);
  u16(out, c.kelvin);
  u32(out, duration);
  return out;
}

function setPowerPayload(level, duration) {
  const out = [];
  u16(out, level);
  u32(out, duration);
  return out;
}

// ---------------------------------------------------------------------------------------------
// Service context
// ---------------------------------------------------------------------------------------------

class LifxController {
  constructor(mac, ip, port, forced) {
    this.id = `lifx-${mac}`;
    this.mac = mac;
    this.name = `LIFX ${mac.slice(6).toUpperCase()}`;
    this.ip = ip;
    this.port = port;
    this.product = 0;
    this.forced = forced === true;
    this.status = "asking for its name";
  }
}

class PendingController {
  constructor(ip) {
    this.id = `forced-${String(ip).replace(/[^0-9A-Za-z_-]/g, "-")}`;
    this.name = "LIFX (added by IP)";
    this.ip = ip;
    this.status = "waiting for a reply";
    this.pending = true;
  }
}

export class DiscoveryService {
  constructor() {
    this.IconUrl = "";
    this.UdpBroadcastAddress = "255.255.255.255";
    this.UdpBroadcastPort = PORT;
    this.lastDiscovery = 0;
    this.sequence = 0;
    this.socket = null;
    this.forced = [];
    this.seen = {};
    this.found = {};
  }

  Initialize() {
    this.forced = this.loadForced();
    for (const ip of this.forced) {
      service.addController(new PendingController(ip));
    }
    try {
      this.socket = udp.createSocket();
      this.socket.on("message", (msg) => this.handle(msg?.buffer, msg?.address));
      this.socket.on("error", (code, message) => service.log(`LIFX: socket error ${code} ${message ?? ""}`));
      this.socket.bind(0);
    } catch (e) {
      service.log(`LIFX: no service socket (${e}); bulbs keep a default name`);
      this.socket = null;
    }
    this.discover();
  }

  nextSequence() {
    this.sequence = (this.sequence + 1) & 0xFF;
    return this.sequence;
  }

  discover() {
    this.lastDiscovery = now();
    service.broadcast(message(GET_SERVICE, [], { tagged: true, sequence: this.nextSequence() }));
    for (const ip of this.forced) {
      this.write(message(GET_SERVICE, [], { tagged: true, sequence: this.nextSequence() }), ip, PORT);
    }
  }

  write(bytes, ip, port) {
    if (!this.socket) {
      return;
    }
    if (this.socket.write(bytes, ip, port) < 0) {
      service.log(`LIFX: sending to ${ip}:${port} failed`);
    }
  }

  Update() {
    const t = now();
    if (t - this.lastDiscovery >= DISCOVERY_INTERVAL_MS) {
      this.discover();
    }
    for (const wrapper of [...service.controllers]) {
      const obj = wrapper.obj ?? wrapper;
      if (obj.pending) {
        continue;
      }
      if (!obj.announced && t - (this.found[obj.id] ?? t) >= LABEL_WAIT_MS) {
        this.announce(obj, "online (no label reply)");
      }
      if (t - (this.seen[obj.id] ?? 0) > STALE_MS) {
        service.log(`LIFX: ${obj.name} (${obj.ip}) stopped answering, removed`);
        this.drop(obj);
      }
    }
  }

  Discovered(value) {
    this.handle(value?.buffer, value?.ip);
  }

  handle(buffer, fromIp) {
    const m = parse(buffer);
    if (!m || !fromIp) {
      return;
    }
    const id = `lifx-${m.mac}`;
    if (m.type === STATE_SERVICE && m.payload.length >= 5) {
      if (m.payload[0] !== SERVICE_UDP) {
        return;
      }
      this.onStateService(m.mac, String(fromIp), readU32(m.payload, 1) || PORT);
    } else if (m.type === STATE_LABEL) {
      const obj = service.getController(id);
      if (obj) {
        const label = utf8(m.payload.slice(0, 32));
        if (label) {
          obj.name = label;
        }
        this.announce(obj, "online");
      }
    } else if (m.type === STATE_VERSION && m.payload.length >= 8) {
      const obj = service.getController(id);
      if (obj && !obj.announced) {
        obj.product = readU32(m.payload, 4);
      }
    }
    // GetService (our own broadcast looped back) and anything else: ignored.
  }

  onStateService(mac, ip, port) {
    if (/^0+$/.test(mac)) {
      return;
    }
    const id = `lifx-${mac}`;
    this.seen[id] = now();
    const pending = service.getController(`forced-${ip.replace(/[^0-9A-Za-z_-]/g, "-")}`);
    if (pending) {
      service.removeController(pending);
    }
    const existing = service.getController(id);
    if (existing) {
      if (existing.ip !== ip || existing.port !== port) {
        existing.ip = ip;
        existing.port = port;
        service.updateController(existing);
        if (existing.announced) {
          service.announceController(existing);
        }
      }
      return;
    }
    const obj = new LifxController(mac, ip, port, this.forced.includes(ip));
    obj.announced = false;
    this.found[id] = now();
    service.addController(obj);
    this.write(message(GET_LABEL, [], { mac, sequence: this.nextSequence() }), ip, port);
    this.write(message(GET_VERSION, [], { mac, sequence: this.nextSequence() }), ip, port);
  }

  announce(obj, status) {
    if (obj.announced) {
      return;
    }
    obj.status = status;
    service.log(`LIFX: found ${obj.name} (${obj.mac}) at ${obj.ip}`);
    service.updateController(obj);
    // `announced` is set after the copy: the device data stays the same on later announces.
    service.announceController(obj);
    obj.announced = true;
  }

  drop(obj) {
    if (obj.announced) {
      service.suppressController(obj);
    }
    service.removeController(obj);
    delete this.seen[obj.id];
    delete this.found[obj.id];
    if (obj.forced && this.forced.includes(obj.ip)) {
      service.addController(new PendingController(obj.ip));
    }
  }

  forceDiscover(ip) {
    const addr = String(ip ?? "").trim();
    if (!/^[0-9A-Za-z.:-]+$/.test(addr)) {
      service.log(`LIFX: '${addr}' is not an IP address`);
      return;
    }
    if (!this.forced.includes(addr)) {
      this.forced.push(addr);
      service.saveSetting(FORCED_ID, FORCED_KEY, JSON.stringify(this.forced));
    }
    if (!service.controllers.some((c) => (c.obj ?? c).ip === addr)) {
      service.addController(new PendingController(addr));
    }
    this.write(message(GET_SERVICE, [], { tagged: true, sequence: this.nextSequence() }), addr, PORT);
  }

  forgetController(id) {
    const obj = service.getController(id);
    if (!obj) {
      return;
    }
    if (this.forced.includes(obj.ip)) {
      this.forced = this.forced.filter((ip) => ip !== obj.ip);
      service.saveSetting(FORCED_ID, FORCED_KEY, JSON.stringify(this.forced));
    }
    if (obj.announced) {
      service.suppressController(obj);
    }
    service.removeController(obj);
    delete this.seen[obj.id];
    delete this.found[obj.id];
  }

  loadForced() {
    try {
      const list = JSON.parse(service.getSetting(FORCED_ID, FORCED_KEY) ?? "[]");
      return Array.isArray(list) ? list.filter((ip) => typeof ip === "string" && ip.length > 0) : [];
    } catch {
      return [];
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Device context
// ---------------------------------------------------------------------------------------------

let sock = null;
let target = null;
let sequence = 0;
let lastColor = null;
let lastSendAt = -Infinity;
let saved = null;
let sendFailureLogged = false;

function log(text) {
  try {
    device.log(`LIFX: ${text}`);
  } catch {
    // never throw from logging
  }
}

function send(type, payload) {
  if (!sock || !target) {
    return false;
  }
  const bytes = message(type, payload, { mac: target.mac, sequence });
  sequence = (sequence + 1) & 0xFF;
  let sent = -1;
  try {
    sent = sock.write(bytes, target.ip, target.port);
  } catch {
    sent = -1;
  }
  if (sent < 0 && !sendFailureLogged) {
    sendFailureLogged = true;
    log(`sending to ${target.ip}:${target.port} failed (further failures are not logged)`);
  }
  return sent >= 0;
}

function onMessage(msg) {
  const m = parse(msg?.buffer);
  if (!m || m.type !== LIGHT_STATE || m.payload.length < 12 || saved) {
    return;
  }
  const p = m.payload;
  saved = {
    hue: readU16(p, 0),
    saturation: readU16(p, 2),
    brightness: readU16(p, 4),
    kelvin: readU16(p, 6),
    power: readU16(p, 10),
  };
}

export function Initialize() {
  const c = typeof controller === "object" && controller ? controller : {};
  target = c.ip ? { ip: String(c.ip), port: Number(c.port) || PORT, mac: c.mac ?? null } : null;
  sequence = 0;
  saved = null;
  lastColor = null;
  lastSendAt = -Infinity;
  sendFailureLogged = false;
  try {
    device.setName(c.name || "LIFX");
    device.setSize([1, 1]);
    device.setControllableLeds(["Bulb"], [[0, 0]]);
  } catch (e) {
    log(`layout failed: ${e}`);
  }
  if (!target) {
    log("the controller has no IP address; nothing will be sent");
    return;
  }
  try {
    sock = udp.createSocket();
    sock.on("message", onMessage);
    sock.on("error", (code, text) => log(`socket error ${code} ${text ?? ""}`));
    sock.bind(0);
  } catch (e) {
    log(`no UDP socket: ${e}`);
    sock = null;
    return;
  }
  // The LightState reply arrives before a later Render and is kept for Shutdown.
  send(LIGHT_GET, []);
  send(LIGHT_SET_POWER, setPowerPayload(0xFFFF, 0));
}

export function Render() {
  if (!sock || !target) {
    return;
  }
  const t = now();
  if (t - lastSendAt < MIN_INTERVAL_MS) {
    return;
  }
  const c = device.color(0, 0);
  const rgb = [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF];
  if (lastColor && lastColor[0] === rgb[0] && lastColor[1] === rgb[1] && lastColor[2] === rgb[2]) {
    return;
  }
  send(LIGHT_SET_COLOR, setColorPayload(hsbk(rgb[0], rgb[1], rgb[2]), DURATION_MS));
  lastColor = rgb;
  lastSendAt = t;
}

// Put back the colour and power the bulb reported at Initialize; without that reply it keeps
// the last colour.
export function Shutdown(_suspend) {
  if (sock && target) {
    if (saved) {
      send(LIGHT_SET_COLOR, setColorPayload(saved, 0));
      send(LIGHT_SET_POWER, setPowerPayload(saved.power ? 0xFFFF : 0, 0));
    } else {
      log("no LightState reply was received; the bulb keeps the last colour");
    }
  }
  try {
    sock?.close();
  } catch {
    // already closed
  }
  sock = null;
  lastColor = null;
}
