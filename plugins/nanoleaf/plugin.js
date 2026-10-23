// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: Nanoleaf panels on the LAN (Light Panels/Aurora, Canvas, Shapes,
// Elements, Lines) through the Nanoleaf OpenAPI: power-button pairing over HTTP (port 16021),
// the panel layout as one LED per panel, colours streamed with the "extControl" UDP protocol
// (v2 to port 60222; v1 for Light Panels, which answer with their own stream address).
//
// Written from the public Nanoleaf OpenAPI documentation. The extControl packet layouts and the
// choice of v1 for the Light Panels model (NL22) and v2 for the others were checked against
// OpenRGB Controllers/NanoleafController/NanoleafController.cpp and NanoleafController.h
// (GPL-2.0-or-later) at commit d4798c12, https://gitlab.com/CalcProgrammer1/OpenRGB
// Copyright (C) 2022 Nikita Rushmanov (NanoleafController.cpp, "part of the OpenRGB project").
//
// Status: UNTESTED. Nobody working on Lightloom owns Nanoleaf panels; checked only against the
// mocks in tests/. Notes and sources: docs/hardware/nanoleaf.md.

/* global device, service, controller, udp, XMLHttpRequest */

export function Name() { return "Nanoleaf"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/nanoleaf"; }
export function Type() { return "network"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
export function ControllableParameters() { return []; }

// OpenAPI HTTP port; extControl v2 UDP port; the v1 port when the reply does not name one.
const API_PORT = 16021;
const STREAM_PORT_V2 = 60222;
const STREAM_PORT_V1 = 60221;
// Model of the original Light Panels (Aurora), which stream with extControl v1.
const MODEL_LIGHT_PANELS = "NL22";
// shapeType values (OpenAPI "Layout": panel shapes) that carry no LEDs: Rhythm module (1),
// Power Supply (5), Shapes Controller (12), Lines Controller Cap (19), Lines Power Connector (20).
const NO_LED_SHAPES = new Set([1, 5, 12, 19, 20]);
// Pairing window after Pair was pressed; retry delay for an unreachable device.
const LINK_WINDOW_MS = 30000;
const RETRY_MS = 30000;
// Streaming: resend an unchanged frame every second; retry entering extControl every 5 s.
const KEEPALIVE_MS = 1000;
const STREAM_RETRY_MS = 5000;
// extControl transition time, in 100 ms units: 0 = jump to the colour (as OpenRGB does).
const TRANSITION = 0;
// Layout grid: nearest panels end up this many cells apart, the grid at most this big.
const NEIGHBOUR_CELLS = 2;
const MAX_GRID = 120;
const INDEX_ID = "devices";
const INDEX_KEY = "list";

// ---------------------------------------------------------------------------------------------
// HTTP helpers (both contexts). They never throw.

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function httpAsync(method, url, body, cb) {
  try {
    const xhr = new XMLHttpRequest();
    let done = false;
    const finish = () => {
      if (done || xhr.readyState !== 4) return;
      done = true;
      cb(xhr.status, parseJson(xhr.responseText));
    };
    xhr.onreadystatechange = finish;
    xhr.onerror = finish;
    xhr.ontimeout = finish;
    xhr.open(method, url, true);
    xhr.timeout = 3000;
    if (body !== undefined) xhr.setRequestHeader("Content-Type", "application/json");
    xhr.send(body === undefined ? null : body);
  } catch (e) {
    cb(0, null);
  }
}

function httpSync(method, url, body, timeout = 2000) {
  try {
    const xhr = new XMLHttpRequest();
    xhr.open(method, url, false);
    xhr.timeout = timeout;
    if (body !== undefined) xhr.setRequestHeader("Content-Type", "application/json");
    xhr.send(body === undefined ? null : body);
    return { status: xhr.status, json: parseJson(xhr.responseText) };
  } catch (e) {
    return { status: 0, json: null };
  }
}

const ok = (status) => status >= 200 && status < 300;

/** The LED panels of a device info reply's panelLayout, as plain data. */
function ledPanels(info) {
  const layout = info && info.panelLayout && info.panelLayout.layout;
  const list = layout && Array.isArray(layout.positionData) ? layout.positionData : [];
  const out = [];
  for (const p of list) {
    if (!p || typeof p.panelId !== "number" || NO_LED_SHAPES.has(Number(p.shapeType))) continue;
    out.push({ id: p.panelId, x: Number(p.x) || 0, y: Number(p.y) || 0, o: Number(p.o) || 0, shapeType: Number(p.shapeType) || 0 });
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Service context: discovery, pairing, device info.

const runtime = new Map(); // id -> { linking, linkStart, linkInFlight, fetching, announced, retryAt }
const forgotten = new Set();

function rt(id) {
  if (!runtime.has(id)) runtime.set(id, { linking: false, linkStart: 0, linkInFlight: false, fetching: false, announced: false, retryAt: 0 });
  return runtime.get(id);
}

class NanoleafDevice {
  constructor(id, ip, name) {
    this.id = id;
    this.ip = ip;
    this.port = API_PORT;
    this.name = name;
    this.model = "";
    this.firmwareVersion = "";
    this.token = "";
    this.paired = false;
    this.panels = [];
    this.extControl = "v2";
    this.status = "Not paired: hold the power button 5-7 s, then press Pair";
  }

  startLink() {
    const state = rt(this.id);
    state.linking = true;
    state.linkStart = Date.now();
    setStatus(this, "Hold the power button for 5-7 seconds, until the lights flash");
    tryLink(this);
  }
}

function setStatus(dev, text) {
  dev.status = text;
  try {
    service.updateController(dev);
  } catch (e) {
    // the dashboard refreshes on its own too
  }
}

function safeId(raw) {
  const id = String(raw || "").replace(/[^A-Za-z0-9:_-]/g, "-");
  return id.length > 0 ? id : null;
}

function readIndex() {
  const list = parseJson(service.getSetting(INDEX_ID, INDEX_KEY) || "[]");
  return Array.isArray(list) ? list.filter((id) => typeof id === "string" && safeId(id) === id) : [];
}

function writeIndex(list) {
  if (list.length > 0) service.saveSetting(INDEX_ID, INDEX_KEY, JSON.stringify(list));
  else service.removeSetting(INDEX_ID, INDEX_KEY);
}

function all() {
  return service.controllers.map((w) => w.obj || w).filter((c) => c instanceof NanoleafDevice);
}

function addDevice(id, ip, info, manual) {
  if (forgotten.has(id) && !manual) return null;
  forgotten.delete(id);
  let dev = service.getController(id) || all().find((d) => d.ip === ip);
  if (dev) {
    if (dev.ip !== ip) {
      dev.ip = ip;
      service.saveSetting(dev.id, "ip", ip);
      rt(dev.id).announced = false;
      if (dev.paired) fetchInfo(dev);
    }
    return dev;
  }
  dev = new NanoleafDevice(id, ip, info.name || `Nanoleaf ${ip}`);
  dev.model = String(info.model || "");
  service.addController(dev);
  service.saveSetting(id, "ip", ip);
  const index = readIndex();
  if (!index.includes(id)) writeIndex([...index, id]);
  const token = service.getSetting(id, "token");
  if (token) {
    dev.token = token;
    dev.paired = true;
    setStatus(dev, "Reading the panel layout");
    fetchInfo(dev);
  }
  return dev;
}

function unpair(dev, why) {
  dev.token = "";
  dev.paired = false;
  dev.panels = [];
  service.removeSetting(dev.id, "token");
  const state = rt(dev.id);
  if (state.announced) {
    state.announced = false;
    service.suppressController(dev);
  }
  setStatus(dev, why);
}

function fetchInfo(dev) {
  const state = rt(dev.id);
  if (state.fetching || !dev.token) return;
  state.fetching = true;
  httpAsync("GET", `http://${dev.ip}:${dev.port}/api/v1/${dev.token}/`, undefined, (status, json) => {
    state.fetching = false;
    if (status === 401 || status === 403) {
      unpair(dev, "The device no longer accepts Lightloom's token: pair again");
      return;
    }
    if (status !== 200 || !json || typeof json !== "object") {
      state.retryAt = Date.now() + RETRY_MS;
      setStatus(dev, "Device not answering");
      return;
    }
    if (json.name) dev.name = String(json.name);
    dev.model = String(json.model || dev.model || "");
    dev.firmwareVersion = String(json.firmwareVersion || "");
    dev.panels = ledPanels(json);
    dev.extControl = dev.model === MODEL_LIGHT_PANELS ? "v1" : "v2";
    setStatus(dev, `${dev.panels.length} panel${dev.panels.length === 1 ? "" : "s"}`);
    state.announced = true;
    service.announceController(dev);
  });
}

function tryLink(dev) {
  const state = rt(dev.id);
  if (!state.linking || state.linkInFlight) return;
  state.linkInFlight = true;
  httpAsync("POST", `http://${dev.ip}:${dev.port}/api/v1/new`, undefined, (status, json) => {
    state.linkInFlight = false;
    if (!state.linking) return;
    if (status === 200 && json && typeof json.auth_token === "string" && json.auth_token.length > 0) {
      state.linking = false;
      dev.token = json.auth_token;
      dev.paired = true;
      service.saveSetting(dev.id, "token", dev.token);
      setStatus(dev, "Paired: reading the panel layout");
      fetchInfo(dev);
    } else if (status === 403) {
      // Not in pairing mode yet: the user has to hold the power button first.
      setStatus(dev, "Hold the power button for 5-7 seconds, until the lights flash");
    } else {
      setStatus(dev, status === 0 ? "Waiting for the device to answer" : `Waiting for pairing mode (HTTP ${status})`);
    }
  });
}

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/;

export function DiscoveryService() {
  this.IconUrl = "";
  this.MDns = ["_nanoleafapi._tcp.local."];

  this.Initialize = function () {
    for (const id of readIndex()) {
      const ip = service.getSetting(id, "ip");
      if (ip && IPV4.test(ip)) addDevice(id, ip, {}, true);
    }
  };

  this.Update = function () {
    const now = Date.now();
    for (const dev of all()) {
      const state = rt(dev.id);
      if (state.linking) {
        if (now - state.linkStart > LINK_WINDOW_MS) {
          state.linking = false;
          setStatus(dev, "Pairing timed out: hold the power button 5-7 s, then press Pair");
        } else {
          tryLink(dev);
        }
      } else if (dev.paired && !state.announced && !state.fetching && now >= state.retryAt) {
        fetchInfo(dev);
      }
    }
  };

  this.Discovered = function (value) {
    if (!value || !value.ip || !IPV4.test(String(value.ip))) return;
    const id = safeId(value.id || value.name);
    if (!id) return;
    addDevice(id, String(value.ip), { name: value.name, model: value.md }, false);
  };

  this.Removal = function () {
    // A device that stops answering mDNS keeps its controller.
  };

  // The OpenAPI has no unauthenticated info request: any HTTP answer on port 16021 is taken as
  // a Nanoleaf device, identified by its address until mDNS or pairing tells more.
  this.forceDiscover = function (ip) {
    const address = String(ip || "").trim();
    if (!IPV4.test(address)) {
      service.log(`Nanoleaf: '${address}' is not an IPv4 address`);
      return;
    }
    httpAsync("GET", `http://${address}:${API_PORT}/api/v1/`, undefined, (status) => {
      if (status === 0) {
        service.log(`Nanoleaf: nothing answered at ${address}:${API_PORT}`);
        return;
      }
      addDevice(safeId(`ip-${address}`), address, {}, true);
    });
  };

  this.forgetController = function (id) {
    const dev = service.getController(id);
    service.removeSetting(id, "token");
    service.removeSetting(id, "ip");
    writeIndex(readIndex().filter((known) => known !== id));
    forgotten.add(id);
    runtime.delete(id);
    if (dev) {
      service.suppressController(dev);
      service.removeController(dev);
    }
  };
}

// ---------------------------------------------------------------------------------------------
// Device context: the panels as LEDs, streamed over extControl.

let session = null;

function data() {
  return typeof controller === "object" && controller ? controller : {};
}

/**
 * Panel centres (layout units, y up) -> grid cells (y down). The scale puts the two closest
 * panels NEIGHBOUR_CELLS apart, then grows if the grid would exceed MAX_GRID.
 */
function gridLayout(panels) {
  if (panels.length === 0) return { size: [1, 1], cells: [] };
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const p of panels) {
    minX = Math.min(minX, p.x);
    maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
  }
  let nearest = Infinity;
  for (let i = 0; i < panels.length; i++) {
    for (let j = i + 1; j < panels.length; j++) {
      const d = Math.hypot(panels[i].x - panels[j].x, panels[i].y - panels[j].y);
      if (d > 0 && d < nearest) nearest = d;
    }
  }
  let unit = Number.isFinite(nearest) ? nearest / NEIGHBOUR_CELLS : 1;
  unit = Math.max(unit, (maxX - minX) / (MAX_GRID - 1), (maxY - minY) / (MAX_GRID - 1));
  const cells = panels.map((p) => [Math.round((p.x - minX) / unit), Math.round((maxY - p.y) / unit)]);
  const w = Math.max(...cells.map((c) => c[0])) + 1;
  const h = Math.max(...cells.map((c) => c[1])) + 1;
  return { size: [w, h], cells };
}

/** extControl v2: nPanels (u16 BE), then per panel id (u16 BE), R, G, B, W, transition (u16 BE). */
function packetV2(ids, colors) {
  const out = [(ids.length >> 8) & 0xFF, ids.length & 0xFF];
  for (let i = 0; i < ids.length; i++) {
    const c = colors[i];
    out.push((ids[i] >> 8) & 0xFF, ids[i] & 0xFF, c[0], c[1], c[2], 0, (TRANSITION >> 8) & 0xFF, TRANSITION & 0xFF);
  }
  return out;
}

/** extControl v1: nPanels (u8), then per panel id (u8), nFrames (1), R, G, B, W, transition (u8). */
function packetV1(ids, colors) {
  const out = [ids.length & 0xFF];
  for (let i = 0; i < ids.length; i++) {
    const c = colors[i];
    out.push(ids[i] & 0xFF, 1, c[0], c[1], c[2], 0, TRANSITION & 0xFF);
  }
  return out;
}

function apiBase(c) {
  return `http://${c.ip}:${Number(c.port) || API_PORT}/api/v1/${c.token}`;
}

const EXT_CONTROL = (version) => JSON.stringify({ write: { command: "display", animType: "extControl", extControlVersion: version } });

function streamTarget(c, version, json) {
  if (version === "v1") {
    const ip = json && typeof json.streamControlIpAddr === "string" ? json.streamControlIpAddr : c.ip;
    const port = json && Number(json.streamControlPort) > 0 ? Number(json.streamControlPort) : STREAM_PORT_V1;
    return { ip, port };
  }
  return { ip: c.ip, port: STREAM_PORT_V2 };
}

function enterExtControlSync(c) {
  const reply = httpSync("PUT", `${apiBase(c)}/effects`, EXT_CONTROL(session.version));
  if (ok(reply.status)) {
    session.target = streamTarget(c, session.version, reply.json);
  } else {
    session.retryAt = Date.now() + STREAM_RETRY_MS;
    device.log(`Nanoleaf: could not enter external control (HTTP ${reply.status}); retrying`);
  }
}

function enterExtControlAsync(c) {
  session.entering = true;
  const current = session;
  httpAsync("PUT", `${apiBase(c)}/effects`, EXT_CONTROL(session.version), (status, json) => {
    current.entering = false;
    if (ok(status)) {
      current.target = streamTarget(c, current.version, json);
      current.last = null;
    } else {
      current.retryAt = Date.now() + STREAM_RETRY_MS;
    }
  });
}

export function Initialize() {
  const c = data();
  const panels = Array.isArray(c.panels) ? c.panels.filter((p) => p && typeof p.id === "number") : [];
  const grid = gridLayout(panels.map((p) => ({ x: Number(p.x) || 0, y: Number(p.y) || 0 })));
  session = {
    ids: panels.map((p) => p.id),
    cells: grid.cells,
    version: c.extControl === "v1" ? "v1" : "v2",
    target: null,
    retryAt: 0,
    entering: false,
    last: null,
    lastSentAt: 0,
    savedEffect: null,
    savedState: null,
  };
  device.setName(c.name ? String(c.name) : "Nanoleaf");
  device.setSize(grid.size);
  device.setControllableLeds(session.ids.map((id) => `Panel ${id}`), grid.cells);
  device.addFeature("udp");
  if (!c.ip || !c.token) {
    device.log("Nanoleaf: no address or token; nothing is sent");
    return;
  }
  const effect = httpSync("GET", `${apiBase(c)}/effects/select`);
  if (effect.status === 200 && typeof effect.json === "string") session.savedEffect = effect.json;
  const state = httpSync("GET", `${apiBase(c)}/state`);
  if (state.status === 200 && state.json && typeof state.json === "object") session.savedState = state.json;
  if (session.savedEffect === null) device.log("Nanoleaf: could not read the current effect; it will not be restored");
  if (session.savedState && session.savedState.on && session.savedState.on.value === false) {
    httpSync("PUT", `${apiBase(c)}/state`, JSON.stringify({ on: { value: true } }));
  }
  enterExtControlSync(c);
}

function same(a, b) {
  if (!a || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

export function Render() {
  if (!session) return;
  const c = data();
  const colors = session.cells.map(([x, y]) => {
    const rgb = device.color(x, y);
    return [rgb[0] & 0xFF, rgb[1] & 0xFF, rgb[2] & 0xFF];
  });
  if (!c.ip || !c.token || session.ids.length === 0) return;
  const now = Date.now();
  if (!session.target) {
    if (!session.entering && now >= session.retryAt) enterExtControlAsync(c);
    return;
  }
  const packet = session.version === "v1" ? packetV1(session.ids, colors) : packetV2(session.ids, colors);
  if (same(session.last, packet) && now - session.lastSentAt < KEEPALIVE_MS) return;
  if (typeof udp === "undefined" || !udp || typeof udp.send !== "function") return;
  udp.send(session.target.ip, session.target.port, packet);
  session.last = packet;
  session.lastSentAt = now;
}

/** What puts the panels back as Initialize found them: [path, body] requests. */
function restoreRequests(effect, state) {
  const out = [];
  const value = (k) => (state && state[k] && typeof state[k].value === "number" ? state[k].value : null);
  if (typeof effect === "string" && effect.length > 0 && !effect.startsWith("*")) {
    out.push(["/effects", { select: effect }]);
  } else if (state && state.colorMode === "hs" && value("hue") !== null && value("sat") !== null) {
    const body = { hue: { value: value("hue") }, sat: { value: value("sat") } };
    if (value("brightness") !== null) body.brightness = { value: value("brightness") };
    out.push(["/state", body]);
  } else if (state && state.colorMode === "ct" && value("ct") !== null) {
    const body = { ct: { value: value("ct") } };
    if (value("brightness") !== null) body.brightness = { value: value("brightness") };
    out.push(["/state", body]);
  }
  if (state && state.on && state.on.value === false) out.push(["/state", { on: { value: false } }]);
  return out;
}

export function Shutdown(suspend) {
  if (!session) return;
  const c = data();
  if (c.ip && c.token) {
    const requests = restoreRequests(session.savedEffect, session.savedState);
    if (requests.length === 0) device.log(`Nanoleaf: effect '${session.savedEffect}' cannot be selected again; the panels keep the last colours`);
    for (const [path, body] of requests) {
      const reply = httpSync("PUT", `${apiBase(c)}${path}`, JSON.stringify(body), 1000);
      if (!ok(reply.status)) device.log(`Nanoleaf: restore ${path} failed (HTTP ${reply.status})`);
    }
  }
  session = null;
}
