// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: generic E1.31 (sACN, "Streaming ACN") output, ANSI E1.31-2018,
// unicast over UDP port 5568 to a receiver added by IP. Written from the public ANSI E1.31-2018
// standard (ESTA, https://tsp.esta.org/tsp/documents/published_docs.php). OpenRGB's E131
// controller (Controllers/E131Controller, GPL-2.0-or-later, commit d4798c12) was read for the
// LED-to-universe convention only; no code is taken from it, and no SignalRGB code is used.
//
// Status: UNTESTED. Nobody on the project owns an E1.31 receiver; every packet below is derived
// from the standard and checked only against the mock net.
//
// There is no discovery: "Add by IP" (discovery.forceDiscover) adds a controller with the id
// "e131-<ip>" and a CID (16 random bytes, a version-4 UUID) generated once and kept with the
// controller list in the plugin settings, so the receiver sees the same source after a restart.
// "Forget" removes it. Universe, start channel, LED count, colour order and priority are device
// parameters.
//
// Universes: 170 RGB LEDs (510 slots) per universe, no LED split across two universes. The first
// universe starts at `startChannel` and holds floor((513 - startChannel) / 3) LEDs; each next
// universe holds up to 170 LEDs from slot 1. Each universe gets one E1.31 data packet per frame
// (Root, Framing and DMP layers, start code 0, as many slots as are used), with its own sequence
// number. A frame is sent when it changed and at least once a second (the standard's data-loss
// timeout is 2.5 s). Shutdown sends three Stream_Terminated packets per universe (§6.2.6).
// Details: docs/hardware/e131.md.

/* global device, service, controller, discovery, udp, universe, startChannel, ledCount, colorOrder, priority */

export function Name() { return "E1.31 (sACN)"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/e131"; }
export function Type() { return "network"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }

export const COLOR_ORDERS = ["RGB", "RBG", "GRB", "GBR", "BRG", "BGR"];
export const MAX_LEDS = 1700;
const DEFAULTS = { universe: 1, startChannel: 1, ledCount: 170, priority: 100 };
const LIMITS = { universe: [1, 63999], startChannel: [1, 512], ledCount: [1, MAX_LEDS], priority: [0, 200] };

export function ControllableParameters() {
  return [
    { property: "universe", group: "lighting", label: "Start Universe",
      description: "E1.31 universe of the first LED (1-63999). LEDs continue in the next universes, 170 per universe.",
      type: "number", min: "1", max: "63999", step: "1", default: "1" },
    { property: "startChannel", group: "lighting", label: "Start Channel",
      description: "DMX channel (slot) of the first LED's first colour in the start universe (1-512).",
      type: "number", min: "1", max: "512", step: "1", default: "1" },
    { property: "ledCount", group: "lighting", label: "LED Count",
      description: "Number of RGB LEDs the receiver drives (E1.31 cannot ask the receiver).",
      type: "number", min: "1", max: String(MAX_LEDS), step: "1", default: "170" },
    { property: "colorOrder", group: "lighting", label: "Color Order",
      description: "Byte order the receiver expects. Keep RGB unless colours come out swapped.",
      type: "combobox", values: COLOR_ORDERS, default: "RGB" },
    { property: "priority", group: "lighting", label: "Priority",
      description: "E1.31 source priority (0-200, default 100). A receiver follows the highest-priority source.",
      type: "number", min: "0", max: "200", step: "1", default: "100" },
  ];
}

// --- E1.31 packet (ANSI E1.31-2018 §4-§7) ------------------------------------------------------

export const E131_PORT = 5568;
// "ASC-E1.17\0\0\0" (§5.3).
const ACN_PACKET_IDENTIFIER = [0x41, 0x53, 0x43, 0x2D, 0x45, 0x31, 0x2E, 0x31, 0x37, 0x00, 0x00, 0x00];
const VECTOR_ROOT_E131_DATA = 0x00000004;
const VECTOR_E131_DATA_PACKET = 0x00000002;
const VECTOR_DMP_SET_PROPERTY = 0x02;
const DMP_ADDRESS_DATA_TYPE = 0xA1;
const SOURCE_NAME = "Lightloom";
// Framing options (§6.2.6): bit 6 = Stream_Terminated.
export const OPTION_STREAM_TERMINATED = 0x40;
const SLOTS_PER_UNIVERSE = 512;
export const LEDS_PER_UNIVERSE = 170;
const KEEPALIVE_MS = 1000;
const TERMINATE_REPEAT = 3;

function u16(v) { return [(v >> 8) & 0xFF, v & 0xFF]; }
function u32(v) { return [(v >>> 24) & 0xFF, (v >>> 16) & 0xFF, (v >>> 8) & 0xFF, v & 0xFF]; }
// Flags and Length (§5.4, §6.2, §7.2): high 4 bits 0x7, low 12 bits = length from this field.
function flagsLength(len) { return u16(0x7000 | (len & 0x0FFF)); }

function sourceName() {
  // 64 bytes, UTF-8, null-terminated and null-padded (§6.2.2). "Lightloom" is ASCII.
  const out = new Array(64).fill(0);
  for (let i = 0; i < SOURCE_NAME.length && i < 63; i++) {
    out[i] = SOURCE_NAME.charCodeAt(i) & 0x7F;
  }
  return out;
}

/**
 * One E1.31 data packet: `cid` 16 bytes, `slots` the DMX data after the start code
 * (1..512 bytes), `options` 0 or OPTION_STREAM_TERMINATED.
 */
export function buildPacket({ cid, universe: uni, sequence, priority: prio, options = 0, slots }) {
  const total = 126 + slots.length;
  return [
    // Root layer (§5)
    ...u16(0x0010),               // preamble size
    ...u16(0x0000),               // post-amble size
    ...ACN_PACKET_IDENTIFIER,
    ...flagsLength(total - 16),
    ...u32(VECTOR_ROOT_E131_DATA),
    ...cid,
    // Framing layer (§6)
    ...flagsLength(total - 38),
    ...u32(VECTOR_E131_DATA_PACKET),
    ...sourceName(),
    prio & 0xFF,
    ...u16(0),                    // synchronization address: none
    sequence & 0xFF,
    options & 0xFF,
    ...u16(uni),
    // DMP layer (§7)
    ...flagsLength(total - 115),
    VECTOR_DMP_SET_PROPERTY,
    DMP_ADDRESS_DATA_TYPE,
    ...u16(0x0000),               // first property address
    ...u16(0x0001),               // address increment
    ...u16(slots.length + 1),     // property value count (start code + slots)
    0x00,                         // start code: dimmer data
    ...slots,
  ];
}

/**
 * Where each LED goes: [{ universe, firstLed, leds, firstSlot }] with firstSlot 1-based. No LED
 * is split across universes; universes above 63999 are dropped.
 */
export function universeLayout(startUniverse, firstChannel, count) {
  const out = [];
  let led = 0;
  let uni = startUniverse;
  let slot = firstChannel;
  while (led < count && uni <= LIMITS.universe[1]) {
    const fit = Math.floor((SLOTS_PER_UNIVERSE + 1 - slot) / 3);
    const n = Math.min(fit, count - led);
    if (n > 0) {
      out.push({ universe: uni, firstLed: led, leds: n, firstSlot: slot });
      led += n;
    }
    uni += 1;
    slot = 1;
  }
  return out;
}

// --- service context ---------------------------------------------------------------------------

const SETTINGS_ID = "e131";
const SETTINGS_KEY = "controllers";
const HEX32 = /^[0-9a-f]{32}$/;

function isIpv4(ip) {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(String(ip ?? ""));
  return m !== null && m.slice(1).every((o) => Number(o) <= 255);
}

/** A random version-4 UUID (RFC 4122) as 32 lower-case hex digits: the E1.31 CID (§5.6). */
export function newCid() {
  const b = [];
  for (let i = 0; i < 16; i++) {
    b.push(Math.floor(Math.random() * 256) & 0xFF);
  }
  b[6] = (b[6] & 0x0F) | 0x40;
  b[8] = (b[8] & 0x3F) | 0x80;
  return b.map((x) => x.toString(16).padStart(2, "0")).join("");
}

function cidBytes(hex) {
  const out = [];
  for (let i = 0; i < 32; i += 2) {
    out.push(parseInt(hex.slice(i, i + 2), 16));
  }
  return out;
}

export class E131Controller {
  constructor(ip, cid) {
    this.id = `e131-${ip}`;
    this.name = `E1.31 ${ip}`;
    this.ip = ip;
    this.port = E131_PORT;
    this.cid = cid;
    this.status = `sACN unicast, UDP ${E131_PORT}`;
  }
}

export function DiscoveryService() {
  this.IconUrl = "";
  this.entries = [];   // [{ ip, cid }]

  this.Initialize = function () {
    try {
      const raw = service.getSetting(SETTINGS_ID, SETTINGS_KEY);
      const list = raw ? JSON.parse(raw) : [];
      this.entries = Array.isArray(list) ?
        list.filter((e) => e && isIpv4(e.ip) && HEX32.test(String(e.cid))) : [];
    } catch (e) {
      service.log(`E1.31: ignoring unreadable controller list (${e})`);
      this.entries = [];
    }
    for (const e of this.entries) {
      this.add(e);
    }
  };

  this.Update = function () {};
  this.Discovered = function () {};

  this.add = function (entry) {
    const c = new E131Controller(entry.ip, entry.cid);
    if (!service.hasController(c.id)) {
      service.addController(c);
      service.announceController(c);
    }
  };

  this.save = function () {
    if (this.entries.length === 0) {
      service.removeSetting(SETTINGS_ID, SETTINGS_KEY);
    } else {
      service.saveSetting(SETTINGS_ID, SETTINGS_KEY, JSON.stringify(this.entries));
    }
  };

  this.forceDiscover = function (ip) {
    const addr = String(ip ?? "").trim();
    if (!isIpv4(addr)) {
      service.log(`E1.31: '${addr}' is not an IPv4 address`);
      return;
    }
    let entry = this.entries.find((e) => e.ip === addr);
    if (!entry) {
      entry = { ip: addr, cid: newCid() };
      this.entries.push(entry);
      this.save();
    }
    service.log(`E1.31: added ${addr} (set universe, start channel and LED count in the device's settings)`);
    this.add(entry);
  };

  this.forgetController = function (id) {
    const obj = service.getController(id);
    const ip = obj?.ip ?? String(id).replace(/^e131-/, "");
    this.entries = this.entries.filter((e) => e.ip !== ip);
    this.save();
    if (obj) {
      service.suppressController(obj);
      service.removeController(obj);
    }
  };
}

// --- device context ------------------------------------------------------------------------------

let target = null;
let cid = null;
let count = 0;
let order = [0, 1, 2];
let prio = DEFAULTS.priority;
let layout = [];
let sequences = {};       // universe -> next sequence number
let sent = {};            // universe -> slots last sent (until that universe is terminated)
let force = true;         // resend the next frame even when unchanged
let lastSentAt = 0;
let sendFailLogged = false;

// The ControllableParameters globals (undefined in a context that has none).
function raw(name) {
  switch (name) {
    case "universe": return typeof universe === "undefined" ? undefined : universe;
    case "startChannel": return typeof startChannel === "undefined" ? undefined : startChannel;
    case "ledCount": return typeof ledCount === "undefined" ? undefined : ledCount;
    case "priority": return typeof priority === "undefined" ? undefined : priority;
    default: return undefined;
  }
}

function param(name) {
  const g = raw(name);
  const [lo, hi] = LIMITS[name];
  const n = Math.floor(Number(g === undefined ? DEFAULTS[name] : g));
  if (!Number.isFinite(n)) {
    return DEFAULTS[name];
  }
  if (n < lo || n > hi) {
    device.log(`E1.31: ${name} ${n} is outside ${lo}..${hi}, using ${Math.min(hi, Math.max(lo, n))}`);
  }
  return Math.min(hi, Math.max(lo, n));
}

function paramOrder() {
  const value = typeof colorOrder === "undefined" ? "RGB" : String(colorOrder).toUpperCase();
  const name = COLOR_ORDERS.includes(value) ? value : "RGB";
  return Array.from(name, (ch) => "RGB".indexOf(ch));
}

function send(packet) {
  if (!target || typeof udp === "undefined" || typeof udp.send !== "function") {
    return;
  }
  let sent = -1;
  try {
    sent = udp.send(target.ip, target.port, packet, "big");
  } catch (e) {
    sent = -1;
  }
  if (sent < 0 && !sendFailLogged) {
    sendFailLogged = true;
    device.log(`E1.31: sending to ${target.ip}:${target.port} failed (further failures are not logged)`);
  }
}

function nextSequence(uni) {
  const s = sequences[uni] ?? 0;
  sequences[uni] = (s + 1) & 0xFF;
  return s;
}

// Universes that were streamed get three Stream_Terminated packets with their last data.
function terminate(universes) {
  for (const uni of universes) {
    const slots = sent[uni];
    if (!slots) {
      continue;
    }
    delete sent[uni];
    for (let i = 0; i < TERMINATE_REPEAT; i++) {
      send(buildPacket({ cid, universe: uni, sequence: nextSequence(uni), priority: prio,
        options: OPTION_STREAM_TERMINATED, slots }));
    }
  }
}

/** Re-read universe / start channel / LED count; terminate universes that are no longer used. */
function relayout() {
  const before = layout.map((u) => u.universe);
  const start = param("universe");
  const channel = param("startChannel");
  const wanted = param("ledCount");
  layout = universeLayout(start, channel, wanted);
  const placed = layout.reduce((n, u) => n + u.leds, 0);
  if (placed < wanted) {
    device.log(`E1.31: only ${placed} of ${wanted} LEDs fit below universe 63999`);
  }
  const now = new Set(layout.map((u) => u.universe));
  const gone = before.filter((u) => !now.has(u));
  terminate(gone);
  if (placed !== count) {
    count = placed;
    const names = [];
    const positions = [];
    for (let i = 0; i < count; i++) {
      names.push(`LED ${i + 1}`);
      positions.push([i, 0]);
    }
    device.setSize([Math.max(1, count), 1]);
    device.setControllableLeds(names, positions);
  }
  force = true;
}

export function Initialize() {
  const c = typeof controller === "object" && controller !== null ? controller : {};
  device.setName(typeof c.name === "string" && c.name !== "" ? c.name : "E1.31");
  const port = Number(c.port);
  target = typeof c.ip === "string" && c.ip !== "" ?
    { ip: c.ip, port: Number.isInteger(port) && port > 0 && port < 65536 ? port : E131_PORT } : null;
  if (!target) {
    device.log("E1.31: controller has no address; nothing will be sent");
  }
  const hex = String(c.cid ?? "").toLowerCase();
  if (HEX32.test(hex)) {
    cid = cidBytes(hex);
  } else {
    cid = cidBytes(newCid());
    device.log("E1.31: controller has no CID; using a random one for this session");
  }
  order = paramOrder();
  prio = param("priority");
  layout = [];
  count = -1;
  sequences = {};
  sent = {};
  lastSentAt = 0;
  sendFailLogged = false;
  device.addFeature("udp");
  relayout();
}

export function onuniverseChanged() { relayout(); }
export function onstartChannelChanged() { relayout(); }
export function onledCountChanged() { relayout(); }
export function oncolorOrderChanged() { order = paramOrder(); force = true; }
export function onpriorityChanged() { prio = param("priority"); force = true; }

function universeSlots(u, colors) {
  const slots = new Array(u.firstSlot - 1 + u.leds * 3).fill(0);
  for (let i = 0; i < u.leds; i++) {
    const c = colors[u.firstLed + i];
    const at = u.firstSlot - 1 + i * 3;
    slots[at] = c[order[0]] & 0xFF;
    slots[at + 1] = c[order[1]] & 0xFF;
    slots[at + 2] = c[order[2]] & 0xFF;
  }
  return slots;
}

function same(a, b) {
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
  const colors = [];
  for (let i = 0; i < count; i++) {
    colors.push(device.color(i, 0));
  }
  const frame = {};
  let changed = force;
  for (const u of layout) {
    frame[u.universe] = universeSlots(u, colors);
    if (!changed && !same(sent[u.universe], frame[u.universe])) {
      changed = true;
    }
  }
  if (!changed && Date.now() - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  force = false;
  for (const u of layout) {
    sent[u.universe] = frame[u.universe];
    send(buildPacket({ cid, universe: u.universe, sequence: nextSequence(u.universe), priority: prio,
      slots: frame[u.universe] }));
  }
  lastSentAt = Date.now();
}

// §6.2.6: a source that stops sends three packets with Stream_Terminated set, so receivers
// release the universe at once instead of waiting for the 2.5 s data-loss timeout.
export function Shutdown() {
  if (target && cid) {
    terminate(Object.keys(sent).map(Number));
  }
  sent = {};
}
