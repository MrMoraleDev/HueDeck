// SPDX-License-Identifier: GPL-3.0-or-later
// Optional per-segment ("razer" / DreamView) streaming adapted from OpenRGB
// Controllers/GoveeController/GoveeController.cpp and RGBController_Govee.cpp
// (GPL-2.0-or-later) at commit d4798c12, https://gitlab.com/CalcProgrammer1/OpenRGB
// Copyright (C) 2023 Adam Honse (calcprogrammer1@gmail.com)
//
// HueDeck first-party plugin: Govee Wi-Fi lights over Govee's public LAN API (the user turns on
// "LAN Control" for the light in the Govee Home app). Everything else (scan, turn, brightness,
// colorwc, devStatus) is written from Govee's "LAN API" developer document.
//
// Status: UNTESTED. Nobody working on HueDeck owns a Govee light; the packets are worked out
// from the public document and OpenRGB's code only.
//
// Discovery (service context): a scan request is multicast to 239.255.255.250:4001 at start and
// every 60 s (unicast to every IP added by hand too); lights answer on UDP 4002 with their
// device id, SKU and IP. A light that has not answered for three rounds is removed.
// Device: JSON commands to <ip>:4003. Single colour mode (default): one LED, "colorwc" at most
// 10 times a second, only on change, re-sent every 30 s. Segment mode (ControllableParameter,
// off by default): OpenRGB's "razer" frames, one colour per segment.
// Protocol notes and sources: docs/hardware/govee.md.

/* global device, service, controller, discovery, segmentMode, segmentCount */

import udp from "@SignalRGB/udp";
import { encode } from "@SignalRGB/base64";

export function Name() { return "Govee"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/govee"; }
export function Type() { return "network"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Light"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
export function ControllableParameters() {
  return [
    {
      property: "segmentMode",
      group: "lighting",
      label: "Per-segment streaming (DreamView, untested)",
      description: "Stream one colour per segment with the \"razer\" command (not in Govee's public LAN document; adapted from OpenRGB). Off: one colour for the whole light.",
      type: "boolean",
      default: "false",
    },
    {
      property: "segmentCount",
      group: "lighting",
      label: "Segments",
      description: "Number of segments the light has in per-segment mode (OpenRGB lists e.g. H619A: 20, H6046: 20, H612F: 12, H607C: 174)",
      type: "number",
      min: "1",
      max: "255",
      default: "20",
    },
  ];
}

// Govee LAN API ports: lights listen for scans on 4001 (multicast group 239.255.255.250),
// answer on 4002 and take commands on 4003.
const MULTICAST = "239.255.255.250";
const SCAN_PORT = 4001;
const REPLY_PORT = 4002;
const CONTROL_PORT = 4003;
const SCAN = JSON.stringify({ msg: { cmd: "scan", data: { account_topic: "reserve" } } });
const SCAN_INTERVAL_MS = 60000;
// Removed after this long without an answer: three scan rounds and a little slack.
const STALE_MS = 3 * SCAN_INTERVAL_MS + 5000;
// Device side: at most 10 commands a second, the last state re-sent every 30 s (OpenRGB's keepalive).
const MIN_INTERVAL_MS = 100;
const KEEPALIVE_MS = 30000;

const FORCED_ID = "forced";
const FORCED_KEY = "ips";

function now() { return Date.now(); }

function parseJson(text) {
  try {
    return JSON.parse(String(text ?? "").replace(/\0+$/, ""));
  } catch {
    return null;
  }
}

function safeId(text) {
  return String(text).replace(/[^0-9A-Za-z:_-]/g, "-");
}

// ---------------------------------------------------------------------------------------------
// Service context
// ---------------------------------------------------------------------------------------------

class GoveeController {
  constructor(id, ip, sku, info, forced) {
    this.id = id;
    this.name = sku ? `Govee ${sku}` : "Govee";
    this.ip = ip;
    this.port = CONTROL_PORT;
    this.sku = sku ?? "";
    this.bleVersionHard = info?.bleVersionHard ?? "";
    this.bleVersionSoft = info?.bleVersionSoft ?? "";
    this.wifiVersionHard = info?.wifiVersionHard ?? "";
    this.wifiVersionSoft = info?.wifiVersionSoft ?? "";
    this.forced = forced === true;
    this.status = "online";
  }
}

// A hand-added IP that has not answered yet: shown on the dashboard, never announced.
class PendingController {
  constructor(ip) {
    this.id = `forced-${safeId(ip)}`;
    this.name = "Govee (added by IP)";
    this.ip = ip;
    this.status = "waiting for a scan reply (is LAN Control on?)";
    this.pending = true;
  }
}

export class DiscoveryService {
  constructor() {
    this.IconUrl = "";
    this.UdpBroadcastAddress = MULTICAST;
    this.UdpBroadcastPort = SCAN_PORT;
    this.UdpListenPort = REPLY_PORT;
    this.lastScan = 0;
    this.socket = null;
    this.forced = [];
    this.seen = {};
  }

  Initialize() {
    this.forced = this.loadForced();
    for (const ip of this.forced) {
      service.addController(new PendingController(ip));
    }
    try {
      this.socket = udp.createSocket();
      this.socket.on("message", (msg) => this.handleText(msg?.data, msg?.address));
      this.socket.on("error", (code, message) => service.log(`Govee: socket error ${code} ${message ?? ""}`));
      this.socket.bind(0);
    } catch (e) {
      service.log(`Govee: no unicast socket (${e}); added IPs answer on port 4002 only`);
      this.socket = null;
    }
    this.scan();
  }

  Update() {
    const t = now();
    if (t - this.lastScan >= SCAN_INTERVAL_MS) {
      this.scan();
    }
    for (const wrapper of [...service.controllers]) {
      const obj = wrapper.obj ?? wrapper;
      if (obj.pending) {
        continue;
      }
      const last = this.seen[obj.id] ?? 0;
      if (t - last > STALE_MS) {
        service.log(`Govee: ${obj.id} (${obj.ip}) stopped answering, removed`);
        this.drop(obj);
      }
    }
  }

  scan() {
    this.lastScan = now();
    service.broadcast(SCAN);
    for (const ip of this.forced) {
      this.unicastScan(ip);
    }
  }

  unicastScan(ip) {
    if (!this.socket) {
      return;
    }
    if (this.socket.write(SCAN, ip, SCAN_PORT) < 0) {
      service.log(`Govee: scan to ${ip} could not be sent`);
    }
  }

  Discovered(value) {
    this.handleText(value?.response, value?.ip);
  }

  handleText(text, fromIp) {
    const reply = parseJson(text);
    const msg = reply?.msg;
    if (!msg || msg.cmd !== "scan" || !msg.data || msg.data.device === undefined) {
      // Our own scan (looped back), other clients' scans, devStatus replies: nothing to do here.
      return;
    }
    this.handleScanReply(msg.data, fromIp);
  }

  handleScanReply(data, fromIp) {
    const id = safeId(data.device);
    const ip = String(data.ip || fromIp || "");
    if (!ip) {
      return;
    }
    this.seen[id] = now();
    const forced = this.forced.includes(ip);
    const pending = service.getController(`forced-${safeId(ip)}`);
    if (pending) {
      service.removeController(pending);
    }
    const existing = service.getController(id);
    if (existing) {
      existing.status = "online";
      if (existing.ip !== ip || existing.sku !== String(data.sku ?? "")) {
        existing.ip = ip;
        existing.sku = String(data.sku ?? "");
        existing.name = existing.sku ? `Govee ${existing.sku}` : "Govee";
        service.updateController(existing);
        service.announceController(existing);
      }
      return;
    }
    const obj = new GoveeController(id, ip, data.sku ? String(data.sku) : "", data, forced);
    service.log(`Govee: found ${obj.name} (${id}) at ${ip}`);
    service.addController(obj);
    service.announceController(obj);
  }

  drop(obj) {
    service.suppressController(obj);
    service.removeController(obj);
    delete this.seen[obj.id];
    if (obj.forced && this.forced.includes(obj.ip)) {
      service.addController(new PendingController(obj.ip));
    }
  }

  forceDiscover(ip) {
    const addr = String(ip ?? "").trim();
    if (!/^[0-9A-Za-z.:-]+$/.test(addr)) {
      service.log(`Govee: '${addr}' is not an IP address`);
      return;
    }
    if (!this.forced.includes(addr)) {
      this.forced.push(addr);
      this.saveForced();
    }
    if (!service.controllers.some((c) => (c.obj ?? c).ip === addr)) {
      service.addController(new PendingController(addr));
    }
    this.unicastScan(addr);
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
    if (!obj.pending) {
      service.suppressController(obj);
    }
    service.removeController(obj);
    delete this.seen[obj.id];
  }

  loadForced() {
    const list = parseJson(service.getSetting(FORCED_ID, FORCED_KEY));
    return Array.isArray(list) ? list.filter((ip) => typeof ip === "string" && ip.length > 0) : [];
  }

  saveForced() {
    service.saveSetting(FORCED_ID, FORCED_KEY, JSON.stringify(this.forced));
  }
}

// ---------------------------------------------------------------------------------------------
// Device context
// ---------------------------------------------------------------------------------------------

/** The commands, exactly as sent (JSON.stringify keeps this key order). */
function turnCommand(on) {
  return JSON.stringify({ msg: { cmd: "turn", data: { value: on ? 1 : 0 } } });
}
function brightnessCommand(value) {
  return JSON.stringify({ msg: { cmd: "brightness", data: { value: Math.max(0, Math.min(100, Math.round(value))) } } });
}
function colorCommand(r, g, b, kelvin = 0) {
  return JSON.stringify({ msg: { cmd: "colorwc", data: { color: { r, g, b }, colorTemInKelvin: kelvin } } });
}
const STATUS_COMMAND = JSON.stringify({ msg: { cmd: "devStatus", data: {} } });

// OpenRGB GoveeController::CalculateXorChecksum / SendRazerData / SendRazerEnable / SendRazerDisable:
// BB len_hi len_lo B0 01 count (R G B)*count xor, len = 2 + 3 * count; base64 in {"pt": ...}.
function xor(bytes) {
  let x = 0;
  for (const b of bytes) {
    x ^= b;
  }
  return x;
}
function razerCommand(bytes) {
  return JSON.stringify({ msg: { cmd: "razer", data: { pt: encode(bytes) } } });
}
function razerFrame(colors) {
  const count = Math.min(colors.length, 255);
  const len = 2 + 3 * count;
  const pkt = [0xBB, (len >> 8) & 0xFF, len & 0xFF, 0xB0, 0x01, count];
  for (let i = 0; i < count; i++) {
    pkt.push(colors[i][0] & 0xFF, colors[i][1] & 0xFF, colors[i][2] & 0xFF);
  }
  pkt.push(xor(pkt));
  return razerCommand(pkt);
}
function razerEnable() { return razerCommand([0xBB, 0x00, 0x01, 0xB1, 0x01, 0x0A]); }
function razerDisable() { return razerCommand([0xBB, 0x00, 0x01, 0xB1, 0x00, 0x0B]); }

let sock = null;
let target = null;
let segments = false;
let count = 1;
let lastFrame = null;
let lastSendAt = 0;
let lastKeepAlive = 0;
let saved = null;
let sendFailureLogged = false;

function log(message) {
  try {
    device.log(`Govee: ${message}`);
  } catch {
    // never throw from logging
  }
}

function truthy(value) {
  return value === true || value === 1 || value === "true" || value === "1";
}

function segmentSetting() {
  return typeof segmentMode !== "undefined" && truthy(segmentMode);
}

function countSetting() {
  const n = typeof segmentCount !== "undefined" ? Math.round(Number(segmentCount)) : 20;
  return Number.isFinite(n) ? Math.max(1, Math.min(255, n)) : 20;
}

function send(text) {
  if (!sock || !target) {
    return false;
  }
  let sent = -1;
  try {
    sent = sock.write(text, target.ip, target.port);
  } catch (e) {
    sent = -1;
  }
  if (sent < 0 && !sendFailureLogged) {
    sendFailureLogged = true;
    log(`sending to ${target.ip}:${target.port} failed (further failures are not logged)`);
  }
  return sent >= 0;
}

function onMessage(msg) {
  const reply = parseJson(msg?.data);
  const data = reply?.msg?.cmd === "devStatus" ? reply.msg.data : null;
  if (data && !saved) {
    saved = {
      onOff: Number(data.onOff ?? 1),
      brightness: Number(data.brightness ?? 100),
      color: data.color && typeof data.color === "object"
        ? { r: Number(data.color.r) & 0xFF, g: Number(data.color.g) & 0xFF, b: Number(data.color.b) & 0xFF }
        : null,
      colorTemInKelvin: Number(data.colorTemInKelvin ?? 0) || 0,
    };
  }
}

function layout() {
  segments = segmentSetting();
  count = segments ? countSetting() : 1;
  const names = [];
  const positions = [];
  for (let i = 0; i < count; i++) {
    names.push(count === 1 ? "Light" : `Segment ${i + 1}`);
    positions.push([i, 0]);
  }
  device.setSize([count, 1]);
  device.setControllableLeds(names, positions);
}

export function Initialize() {
  const c = typeof controller === "object" && controller ? controller : {};
  target = c.ip ? { ip: String(c.ip), port: Number(c.port) || CONTROL_PORT } : null;
  saved = null;
  lastFrame = null;
  lastSendAt = 0;
  lastKeepAlive = 0;
  sendFailureLogged = false;
  try {
    device.setName(c.name || (c.sku ? `Govee ${c.sku}` : "Govee"));
    layout();
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
    sock.on("error", (code, message) => log(`socket error ${code} ${message ?? ""}`));
    sock.bind(0);
  } catch (e) {
    log(`no UDP socket: ${e}`);
    sock = null;
    return;
  }
  // State to restore at Shutdown, then: on, full brightness (the colour carries the intensity).
  send(STATUS_COMMAND);
  send(turnCommand(true));
  send(brightnessCommand(100));
  if (segments) {
    send(razerEnable());
  }
  lastKeepAlive = now();
}

function sample() {
  const colors = [];
  for (let i = 0; i < count; i++) {
    const c = device.color(i, 0);
    colors.push([c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF]);
  }
  return colors;
}

function sameFrame(a, b) {
  if (!a || !b || a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) {
      return false;
    }
  }
  return true;
}

export function Render() {
  if (!sock || !target) {
    return;
  }
  const t = now();
  if (t - lastSendAt < MIN_INTERVAL_MS) {
    return;
  }
  const colors = sample();
  const keepAlive = t - lastKeepAlive >= KEEPALIVE_MS;
  if (!keepAlive && sameFrame(colors, lastFrame)) {
    return;
  }
  if (keepAlive && segments) {
    // OpenRGB re-asserts the external-control session so the light does not fall back.
    send(razerEnable());
  }
  if (segments) {
    send(razerFrame(colors));
  } else {
    send(colorCommand(colors[0][0], colors[0][1], colors[0][2]));
  }
  lastFrame = colors;
  lastSendAt = t;
  if (keepAlive) {
    lastKeepAlive = t;
  }
}

function relayout() {
  try {
    const wasSegments = segments;
    layout();
    if (wasSegments !== segments) {
      send(segments ? razerEnable() : razerDisable());
    }
    lastFrame = null;
  } catch (e) {
    log(`layout failed: ${e}`);
  }
}

export function onsegmentModeChanged() { relayout(); }
export function onsegmentCountChanged() { relayout(); }

// Stop segment streaming and put back what devStatus reported at Initialize (colour or white
// temperature, brightness, power). Without that reply the light keeps the last colour.
export function Shutdown(_suspend) {
  if (sock && target) {
    if (segments) {
      send(razerDisable());
    }
    if (saved) {
      if (saved.colorTemInKelvin > 0) {
        send(colorCommand(0, 0, 0, saved.colorTemInKelvin));
      } else if (saved.color) {
        send(colorCommand(saved.color.r, saved.color.g, saved.color.b));
      }
      send(brightnessCommand(saved.brightness));
      send(turnCommand(saved.onOff !== 0));
    } else {
      log("no devStatus reply was received; the light keeps the last colour");
    }
  }
  try {
    sock?.close();
  } catch {
    // already closed
  }
  sock = null;
  lastFrame = null;
}
