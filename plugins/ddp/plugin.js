// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: generic DDP output (Distributed Display Protocol, 3waylabs,
// http://www.3waylabs.com/ddp/). WLED, xLights, ESPixelStick and many pixel controllers take
// DDP on UDP port 4048. Written from the public DDP specification; no SignalRGB or OpenRGB code
// is used (OpenRGB's DDPController was only compared for the port and the 1440-byte limit).
//
// Status: UNTESTED. Nobody on the project owns a DDP receiver; every packet below is derived
// from the specification and checked only against the mock net.
//
// There is no discovery: "Add by IP" (discovery.forceDiscover) adds a controller with the id
// "ddp-<ip>", kept in the plugin settings; "Forget" removes it. The strip length and the colour
// order are device parameters (ledCount, colorOrder) because DDP cannot ask the receiver.
//
// Each frame is the strip's RGB bytes split into packets of at most 1440 data bytes (480 LEDs):
// a 10-byte header (flags: version 1, push on the last packet of the frame; sequence 1..15; data
// type RGB 8 bits per element; destination 1 = default output device; 32-bit byte offset;
// 16-bit length, both big-endian) and the data. A frame is sent when it changed and at least
// once a second (receivers such as WLED leave realtime mode after a few seconds of silence).
// Shutdown sends nothing: DDP has no "stop" message; the receiver keeps the last frame or
// returns to its own program after its own timeout. Details: docs/hardware/ddp.md.

/* global device, service, controller, discovery, udp, ledCount, colorOrder */

export function Name() { return "DDP"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/ddp"; }
export function Type() { return "network"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }

export const COLOR_ORDERS = ["RGB", "RBG", "GRB", "GBR", "BRG", "BGR"];
export const DEFAULT_LEDS = 60;
export const MAX_LEDS = 1500;

export function ControllableParameters() {
  return [
    { property: "ledCount", group: "lighting", label: "LED Count",
      description: "Number of LEDs the receiver drives on this output (DDP cannot ask the receiver).",
      type: "number", min: "1", max: String(MAX_LEDS), step: "1", default: String(DEFAULT_LEDS) },
    { property: "colorOrder", group: "lighting", label: "Color Order",
      description: "Byte order the receiver expects. Most receivers reorder themselves: keep RGB unless colours come out swapped.",
      type: "combobox", values: COLOR_ORDERS, default: "RGB" },
  ];
}

// --- DDP header (3waylabs DDP specification) ---------------------------------------------------

export const DDP_PORT = 4048;
const HEADER_SIZE = 10;
// Byte 0: VV x T S R Q P. VV = 01 (version 1) -> 0x40; P (push) = 0x01.
const FLAG_VER1 = 0x40;
const FLAG_PUSH = 0x01;
// Byte 2: C R TTT SSS. C = 0 (standard types), TTT = 001 (RGB), SSS = 011 (8 bits per element).
const TYPE_RGB8 = 0x0B;
// Byte 3: destination id 1 = default output device ("display").
const ID_DISPLAY = 0x01;
// Data bytes per packet: 1440 = 480 RGB pixels (keeps the datagram under a 1500-byte MTU).
export const MAX_DATA = 1440;
const KEEPALIVE_MS = 1000;

/**
 * The DDP packets of one frame of RGB bytes, starting at sequence number `seq` (1..15).
 * Returns { packets, seq } with the next sequence number.
 */
export function buildPackets(data, seq) {
  const packets = [];
  let next = seq >= 1 && seq <= 15 ? seq : 1;
  for (let offset = 0; offset < data.length; offset += MAX_DATA) {
    const len = Math.min(MAX_DATA, data.length - offset);
    const last = offset + len >= data.length;
    const header = new Array(HEADER_SIZE);
    header[0] = FLAG_VER1 | (last ? FLAG_PUSH : 0);
    header[1] = next;
    header[2] = TYPE_RGB8;
    header[3] = ID_DISPLAY;
    header[4] = (offset >>> 24) & 0xFF;
    header[5] = (offset >>> 16) & 0xFF;
    header[6] = (offset >>> 8) & 0xFF;
    header[7] = offset & 0xFF;
    header[8] = (len >> 8) & 0xFF;
    header[9] = len & 0xFF;
    packets.push(header.concat(data.slice(offset, offset + len)));
    next = next >= 15 ? 1 : next + 1;
  }
  return { packets, seq: next };
}

// --- service context ---------------------------------------------------------------------------

const SETTINGS_ID = "ddp";
const SETTINGS_KEY = "controllers";

function isIpv4(ip) {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(String(ip ?? ""));
  return m !== null && m.slice(1).every((o) => Number(o) <= 255);
}

export class DdpController {
  constructor(ip) {
    this.id = `ddp-${ip}`;
    this.name = `DDP ${ip}`;
    this.ip = ip;
    this.port = DDP_PORT;
    this.status = `UDP ${DDP_PORT}`;
  }
}

export function DiscoveryService() {
  this.IconUrl = "";
  this.ips = [];

  this.Initialize = function () {
    try {
      const raw = service.getSetting(SETTINGS_ID, SETTINGS_KEY);
      const list = raw ? JSON.parse(raw) : [];
      this.ips = Array.isArray(list) ? list.filter(isIpv4) : [];
    } catch (e) {
      service.log(`DDP: ignoring unreadable controller list (${e})`);
      this.ips = [];
    }
    for (const ip of this.ips) {
      this.add(ip);
    }
  };

  this.Update = function () {};
  this.Discovered = function () {};

  this.add = function (ip) {
    const c = new DdpController(ip);
    if (!service.hasController(c.id)) {
      service.addController(c);
      service.announceController(c);
    }
  };

  this.save = function () {
    if (this.ips.length === 0) {
      service.removeSetting(SETTINGS_ID, SETTINGS_KEY);
    } else {
      service.saveSetting(SETTINGS_ID, SETTINGS_KEY, JSON.stringify(this.ips));
    }
  };

  this.forceDiscover = function (ip) {
    const addr = String(ip ?? "").trim();
    if (!isIpv4(addr)) {
      service.log(`DDP: '${addr}' is not an IPv4 address`);
      return;
    }
    if (!this.ips.includes(addr)) {
      this.ips.push(addr);
      this.save();
    }
    service.log(`DDP: added ${addr} (set the LED count in the device's settings)`);
    this.add(addr);
  };

  this.forgetController = function (id) {
    const obj = service.getController(id);
    const ip = obj?.ip ?? String(id).replace(/^ddp-/, "");
    this.ips = this.ips.filter((x) => x !== ip);
    this.save();
    if (obj) {
      service.suppressController(obj);
      service.removeController(obj);
    }
  };
}

// --- device context ------------------------------------------------------------------------------

let count = 0;
let order = [0, 1, 2];
let target = null;
let last = null;
let lastSentAt = 0;
let seq = 1;
let sendFailLogged = false;

function paramLedCount() {
  const n = Math.floor(Number(typeof ledCount === "undefined" ? DEFAULT_LEDS : ledCount));
  if (!Number.isFinite(n) || n < 1) {
    return DEFAULT_LEDS;
  }
  if (n > MAX_LEDS) {
    device.log(`DDP: LED count ${n} is above the ${MAX_LEDS} limit, using ${MAX_LEDS}`);
    return MAX_LEDS;
  }
  return n;
}

function paramOrder() {
  const value = typeof colorOrder === "undefined" ? "RGB" : String(colorOrder).toUpperCase();
  const name = COLOR_ORDERS.includes(value) ? value : "RGB";
  return Array.from(name, (ch) => "RGB".indexOf(ch));
}

function layout() {
  count = paramLedCount();
  const names = [];
  const positions = [];
  for (let i = 0; i < count; i++) {
    names.push(`LED ${i + 1}`);
    positions.push([i, 0]);
  }
  device.setSize([count, 1]);
  device.setControllableLeds(names, positions);
  last = null;
}

export function Initialize() {
  const c = typeof controller === "object" && controller !== null ? controller : {};
  device.setName(typeof c.name === "string" && c.name !== "" ? c.name : "DDP");
  const port = Number(c.port);
  target = typeof c.ip === "string" && c.ip !== "" ?
    { ip: c.ip, port: Number.isInteger(port) && port > 0 && port < 65536 ? port : DDP_PORT } : null;
  if (!target) {
    device.log("DDP: controller has no address; nothing will be sent");
  }
  order = paramOrder();
  seq = 1;
  lastSentAt = 0;
  sendFailLogged = false;
  device.addFeature("udp");
  layout();
}

export function onledCountChanged() {
  layout();
}

export function oncolorOrderChanged() {
  order = paramOrder();
  last = null;
}

function frame() {
  const data = new Array(count * 3);
  for (let i = 0; i < count; i++) {
    const c = device.color(i, 0);
    data[i * 3] = c[order[0]] & 0xFF;
    data[i * 3 + 1] = c[order[1]] & 0xFF;
    data[i * 3 + 2] = c[order[2]] & 0xFF;
  }
  return data;
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
  if (!target || count < 1) {
    return;
  }
  const data = frame();
  if (equal(last, data) && Date.now() - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  last = data;
  if (typeof udp === "undefined" || typeof udp.send !== "function") {
    return;
  }
  const built = buildPackets(data, seq);
  seq = built.seq;
  for (const packet of built.packets) {
    let sent = -1;
    try {
      sent = udp.send(target.ip, target.port, packet, "big");
    } catch (e) {
      sent = -1;
    }
    if (sent < 0 && !sendFailLogged) {
      sendFailLogged = true;
      device.log(`DDP: sending to ${target.ip}:${target.port} failed (further failures are not logged)`);
    }
  }
  lastSentAt = Date.now();
}

// DDP has no "end of stream" message: nothing is sent, the receiver keeps the last frame or goes
// back to its own program after its own timeout (WLED: its realtime timeout).
export function Shutdown() {
  last = null;
}
