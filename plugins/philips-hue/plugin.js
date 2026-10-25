// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: Philips Hue bridges on the LAN. Each colour-capable lamp is its own
// subdevice (one LED), so every lamp can be placed on the canvas on its own.
//
// Two ways to drive the lamps, chosen per bridge:
//   * Entertainment API (when the bridge has CLIP v2, Lightloom holds its clientkey and an
//     entertainment area exists): the area is started over CLIP v2 (https), then every Render()
//     streams one HueStream v2 frame for the area's lamps over DTLS 1.2 with a pre-shared key
//     (UDP 2100). Lamps outside the area, and every lamp when streaming cannot start, use:
//   * the CLIP v1 REST API over plain HTTP (http://<bridge>/api/...), rate-limited.
//
// Status: UNTESTED. Nobody working on Lightloom owns a Hue bridge; everything here is written from
// Philips' public developer documentation (docs/hardware/philips-hue.md, Sources) and checked
// only against the mocks in tests/.
//
// Rate limit (REST): Philips asks for about 10 light commands per second per bridge. Render() sends
// at most one PUT every 100 ms, round-robin over the lamps whose colour changed noticeably, with at
// most one request in flight per lamp.
//
// HTTPS: the bridge's certificate is self-signed (or signed by Signify's private CA) with the
// bridge id as its common name. Every https request names that id (`xhr.tlsName`, a Lightloom
// extension) and the host accepts no other certificate (docs/security.md).

/* global device, service, controller, XMLHttpRequest, dtls */

export function Name() { return "Philips Hue"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.2.0"; }
export function Documentation() { return "hardware/philips-hue"; }
export function Type() { return "network"; }
// Lightloom extension (docs/host-api/save-shutdown.md): Shutdown() hands the light back to its
// own state ("restore"); for "keep" and "off" the host falls back to that too.
export function ShutdownModes() { return ["restore"]; }
export function DeviceType() { return "lightingcontroller"; }
// The bridge has no LEDs of its own: only its lamps (subdevices) are placed on the canvas.
export function SubdeviceController() { return true; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
export function ControllableParameters() { return []; }

// "devicetype" of the whitelist entry the bridge creates when pairing: "<app>#<device>",
// app name at most 20 characters, device name at most 19 (Configuration API, 7.1).
const DEVICE_TYPE = "lightloom#pc";
// How long pairing keeps asking the bridge after Pair was pressed.
const LINK_WINDOW_MS = 30000;
// Retry delay when the bridge did not answer the lamp list.
const RETRY_MS = 30000;
// Rate limit: one light command every 100 ms per bridge (about 10/s).
const SEND_INTERVAL_MS = 100;
// A lamp is resent only when a channel moved by at least this much since the last command.
const CHANGE_THRESHOLD = 3;
// Bridge error types (CLIP v1 "Error messages"): 1 unauthorized user, 101 link button not pressed.
const ERROR_UNAUTHORIZED = 1;
const ERROR_LINK_BUTTON = 101;
// Setting id/key of the list of known bridge ids (each bridge then has "ip", "username",
// "clientkey" and, optionally, "area": the id of the entertainment area to stream).
const INDEX_ID = "bridges";
const INDEX_KEY = "list";
// Entertainment streaming: the bridge's DTLS port, the most channels of one HueStream frame, and
// how long a handshake may take before the area's lamps fall back to REST.
const DTLS_PORT = 2100;
const MAX_CHANNELS = 20;
const HANDSHAKE_GIVE_UP_MS = 10000;
// CLIP v2 resources.
const CLIP_V2 = "/clip/v2/resource";
const CLIENT_KEY = /^[0-9a-fA-F]{32}$/;

// Colour gamuts (Philips "RGB to xy Color conversion for Hue lights", gamut table): red, green,
// blue corners in CIE 1931 xy.
const GAMUTS = {
  A: [[0.704, 0.296], [0.2151, 0.7106], [0.138, 0.08]],
  B: [[0.675, 0.322], [0.409, 0.518], [0.167, 0.04]],
  C: [[0.692, 0.308], [0.17, 0.7], [0.153, 0.048]],
};

// ---------------------------------------------------------------------------------------------
// HTTP helpers (both contexts). They never throw.

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/** Header and certificate options of a CLIP v2 (https) request. */
function prepare(xhr, options, hasBody) {
  if (options.bridgeId) xhr.tlsName = options.bridgeId;
  if (options.key) xhr.setRequestHeader("hue-application-key", options.key);
  if (hasBody) xhr.setRequestHeader("Content-Type", "application/json");
}

/**
 * Asynchronous request: cb(status, json, xhr) once it completes (status 0: unreachable).
 * `options.key`: the CLIP v2 application key header; `options.bridgeId`: the certificate name an
 * https request expects.
 */
function httpAsync(method, url, body, cb, options = {}) {
  try {
    const xhr = new XMLHttpRequest();
    let done = false;
    const finish = () => {
      if (done || xhr.readyState !== 4) return;
      done = true;
      cb(xhr.status, parseJson(xhr.responseText), xhr);
    };
    xhr.onreadystatechange = finish;
    xhr.onerror = finish;
    xhr.ontimeout = finish;
    xhr.open(method, url, true);
    xhr.timeout = 3000;
    prepare(xhr, options, body !== undefined);
    xhr.send(body === undefined ? null : body);
  } catch (e) {
    cb(0, null, null);
  }
}

/** Synchronous request: { status, json }. Options as for httpAsync. */
function httpSync(method, url, body, timeout = 2000, options = {}) {
  try {
    const xhr = new XMLHttpRequest();
    xhr.open(method, url, false);
    xhr.timeout = timeout;
    prepare(xhr, options, body !== undefined);
    xhr.send(body === undefined ? null : body);
    return { status: xhr.status, json: parseJson(xhr.responseText) };
  } catch (e) {
    return { status: 0, json: null };
  }
}

/** The bridge's error type in a CLIP v1 reply ([{ error: { type } }]), or 0. */
function errorType(json) {
  const first = Array.isArray(json) ? json[0] : null;
  return first && first.error ? Number(first.error.type) || -1 : 0;
}

/** The `data` array of a CLIP v2 reply ({ errors, data }), or null. */
function v2Data(status, json) {
  return status === 200 && json && typeof json === "object" && Array.isArray(json.data) ? json.data : null;
}

// ---------------------------------------------------------------------------------------------
// Colour conversion (Philips "RGB to xy Color conversion for Hue lights").

function gammaExpand(c) {
  const v = c / 255;
  return v > 0.04045 ? Math.pow((v + 0.055) / (1.0 + 0.055), 2.4) : v / 12.92;
}

function cross(a, b) {
  return a[0] * b[1] - a[1] * b[0];
}

function inTriangle(p, [r, g, b]) {
  const v1 = [g[0] - r[0], g[1] - r[1]];
  const v2 = [b[0] - r[0], b[1] - r[1]];
  const q = [p[0] - r[0], p[1] - r[1]];
  const d = cross(v1, v2);
  const s = cross(q, v2) / d;
  const t = cross(v1, q) / d;
  return s >= 0 && t >= 0 && s + t <= 1;
}

function closestOnLine(a, b, p) {
  const ap = [p[0] - a[0], p[1] - a[1]];
  const ab = [b[0] - a[0], b[1] - a[1]];
  let t = (ap[0] * ab[0] + ap[1] * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]);
  t = Math.min(1, Math.max(0, t));
  return [a[0] + ab[0] * t, a[1] + ab[1] * t];
}

function clampToGamut(p, gamut) {
  if (!gamut || inTriangle(p, gamut)) return p;
  const [r, g, b] = gamut;
  let best = null;
  let bestDistance = Infinity;
  for (const candidate of [closestOnLine(r, g, p), closestOnLine(g, b, p), closestOnLine(b, r, p)]) {
    const d = Math.hypot(candidate[0] - p[0], candidate[1] - p[1]);
    if (d < bestDistance) {
      bestDistance = d;
      best = candidate;
    }
  }
  return best;
}

const round4 = (v) => Math.round(v * 10000) / 10000;

/**
 * [r, g, b] (0..255) -> { xy: [x, y], bri } or null for black.
 * xy: gamma correction, the "Wide RGB D65" matrix, x = X / (X + Y + Z), y = Y / (X + Y + Z),
 * then the closest point of the lamp's gamut triangle when outside it; rounded to 4 decimals.
 * bri: the brightest channel scaled to 1..254 (see docs/hardware/philips-hue.md: the bridge
 * already scales "bri" to the brightest the lamp can do at that xy, so the guide's Y would
 * make saturated blue about 5 % bright).
 */
function rgbToXyBri(rgb, gamut) {
  const max = Math.max(rgb[0], rgb[1], rgb[2]);
  if (max === 0) return null;
  const r = gammaExpand(rgb[0]);
  const g = gammaExpand(rgb[1]);
  const b = gammaExpand(rgb[2]);
  const X = r * 0.664511 + g * 0.154324 + b * 0.162028;
  const Y = r * 0.283881 + g * 0.668433 + b * 0.047685;
  const Z = r * 0.000088 + g * 0.07231 + b * 0.986039;
  const sum = X + Y + Z;
  const p = clampToGamut([X / sum, Y / sum], gamut);
  return { xy: [round4(p[0]), round4(p[1])], bri: Math.max(1, Math.round((max / 255) * 254)) };
}

/** A lamp's gamut triangle from its capabilities, or null (the bridge then clamps it itself). */
function gamutOf(light) {
  const control = light && light.capabilities && light.capabilities.control;
  const points = control && control.colorgamut;
  if (Array.isArray(points) && points.length === 3 && points.every((pt) => Array.isArray(pt) && pt.length === 2)) {
    return points.map((pt) => [Number(pt[0]), Number(pt[1])]);
  }
  const type = control && typeof control.colorgamuttype === "string" ? control.colorgamuttype.toUpperCase() : "";
  return GAMUTS[type] ? GAMUTS[type].map((pt) => pt.slice()) : null;
}

/** The colour-capable lamps of a GET /api/<user>/lights reply, as plain data. */
function colourLights(json) {
  const out = [];
  if (!json || typeof json !== "object" || Array.isArray(json)) return out;
  for (const id of Object.keys(json)) {
    const light = json[id] || {};
    const state = light.state || {};
    const type = String(light.type || "");
    const colour = Array.isArray(state.xy) || /^(extended )?color light$/i.test(type);
    if (!colour) continue;
    out.push({ id: String(id), name: String(light.name || `Light ${id}`), type, modelid: String(light.modelid || ""), gamut: gamutOf(light) });
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Entertainment (CLIP v2): the area to stream and the CLIP v1 lamp behind each of its channels.

/** "/lights/3" -> "3". */
function v1Id(idV1) {
  const m = /^\/lights\/(\d+)$/.exec(String(idV1 || ""));
  return m ? m[1] : null;
}

/**
 * The entertainment area to stream, as plain data, from three CLIP v2 replies (entertainment
 * configurations, entertainment services, lights): { id, name, channels: [{ id, light }] } where
 * `light` is the CLIP v1 id of the lamp the channel belongs to (a gradient lamp has several
 * channels on one lamp). `wanted`: the configured area id, else the first area. Null when there
 * is no usable area.
 */
function entertainmentArea(configs, services, lights, wanted) {
  if (!Array.isArray(configs)) return null;
  const valid = configs.filter((c) => c && typeof c.id === "string" && /^[0-9a-f-]{36}$/i.test(c.id));
  const config = valid.find((c) => c.id === wanted) || valid[0];
  if (!config) return null;
  const lightOfService = new Map();
  for (const s of Array.isArray(services) ? services : []) {
    const ref = s && s.renderer_reference;
    if (s && typeof s.id === "string" && ref && ref.rtype === "light") lightOfService.set(s.id, ref.rid);
  }
  const v1OfLight = new Map();
  for (const l of Array.isArray(lights) ? lights : []) {
    if (l && typeof l.id === "string") v1OfLight.set(l.id, v1Id(l.id_v1));
  }
  const channels = [];
  for (const ch of Array.isArray(config.channels) ? config.channels : []) {
    const id = Number(ch && ch.channel_id);
    if (!Number.isInteger(id) || id < 0 || id > 255) continue;
    const member = Array.isArray(ch.members) ? ch.members.find((m) => m && m.service && m.service.rtype === "entertainment") : null;
    const light = member ? v1OfLight.get(lightOfService.get(member.service.rid)) ?? null : null;
    if (light !== null) channels.push({ id, light });
  }
  if (channels.length === 0) return null;
  const name = (config.metadata && config.metadata.name) || config.name || "Entertainment area";
  return { id: config.id.toLowerCase(), name: String(name), channels: channels.slice(0, MAX_CHANNELS) };
}

// ---------------------------------------------------------------------------------------------
// Service context: discovery, pairing, the lamp list, the entertainment area.

const runtime = new Map(); // bridge id -> { linking, linkStart, linkInFlight, fetching, announced, retryAt }
const forgotten = new Set(); // ids forgotten this session: mDNS does not bring them back

function rt(id) {
  if (!runtime.has(id)) runtime.set(id, { linking: false, linkStart: 0, linkInFlight: false, fetching: false, announced: false, retryAt: 0 });
  return runtime.get(id);
}

class HueBridge {
  constructor(id, ip, name) {
    this.id = id;
    this.ip = ip;
    this.name = name;
    this.modelid = "";
    this.apiversion = "";
    this.username = "";
    this.clientkey = "";
    this.appId = "";
    this.paired = false;
    this.lights = [];
    this.entertainment = null;
    this.status = "Not paired: press Pair, then the bridge's link button";
  }

  startLink() {
    const state = rt(this.id);
    state.linking = true;
    state.linkStart = Date.now();
    setStatus(this, "Press the link button on the bridge");
    tryLink(this);
  }
}

function setStatus(bridge, text) {
  bridge.status = text;
  try {
    service.updateController(bridge);
  } catch (e) {
    // the dashboard refreshes on its own too
  }
}

function readIndex() {
  const list = parseJson(service.getSetting(INDEX_ID, INDEX_KEY) || "[]");
  return Array.isArray(list) ? list.filter((id) => typeof id === "string" && /^[0-9a-f]+$/.test(id)) : [];
}

function writeIndex(list) {
  if (list.length > 0) service.saveSetting(INDEX_ID, INDEX_KEY, JSON.stringify(list));
  else service.removeSetting(INDEX_ID, INDEX_KEY);
}

function normaliseId(bridgeid) {
  const id = String(bridgeid || "").toLowerCase().replace(/[^0-9a-f]/g, "");
  return id.length > 0 ? id : null;
}

function addBridge(id, ip, info, manual) {
  if (forgotten.has(id) && !manual) return null;
  forgotten.delete(id);
  let bridge = service.getController(id);
  if (bridge) {
    if (info.name) bridge.name = info.name;
    if (bridge.ip !== ip) {
      bridge.ip = ip;
      service.saveSetting(id, "ip", ip);
      rt(id).announced = false;
      if (bridge.paired) fetchLights(bridge);
    }
    return bridge;
  }
  bridge = new HueBridge(id, ip, info.name || `Hue bridge ${id.slice(-6)}`);
  bridge.modelid = String(info.modelid || "");
  bridge.apiversion = String(info.apiversion || "");
  service.addController(bridge);
  service.saveSetting(id, "ip", ip);
  const index = readIndex();
  if (!index.includes(id)) writeIndex([...index, id]);
  const username = service.getSetting(id, "username");
  if (username) {
    bridge.username = username;
    const clientkey = service.getSetting(id, "clientkey") || "";
    bridge.clientkey = CLIENT_KEY.test(clientkey) ? clientkey : "";
    bridge.paired = true;
    setStatus(bridge, "Reading lamps");
    fetchLights(bridge);
  }
  return bridge;
}

function unpair(bridge, why) {
  bridge.username = "";
  bridge.clientkey = "";
  bridge.appId = "";
  bridge.paired = false;
  bridge.lights = [];
  bridge.entertainment = null;
  service.removeSetting(bridge.id, "username");
  service.removeSetting(bridge.id, "clientkey");
  const state = rt(bridge.id);
  if (state.announced) {
    state.announced = false;
    service.suppressController(bridge);
  }
  setStatus(bridge, why);
}

function lampSummary(bridge) {
  const n = bridge.lights.length;
  const lamps = `${n} colour lamp${n === 1 ? "" : "s"}`;
  return bridge.entertainment ? `${lamps}; streams to "${bridge.entertainment.name}"` : lamps;
}

function announce(bridge) {
  const state = rt(bridge.id);
  state.fetching = false;
  setStatus(bridge, lampSummary(bridge));
  state.announced = true;
  service.announceController(bridge);
}

/**
 * CLIP v2 over https (certificate name: the bridge id): the entertainment areas, the
 * entertainment services (the lamp behind each), the lights (their CLIP v1 ids), then the
 * application id, then announce. Any failure: announce without an area (REST only).
 */
function fetchEntertainment(bridge) {
  const base = `https://${bridge.ip}${CLIP_V2}`;
  const options = { key: bridge.username, bridgeId: bridge.id };
  const replies = {};
  const steps = [
    ["configs", `${base}/entertainment_configuration`],
    ["services", `${base}/entertainment`],
    ["lights", `${base}/light`],
  ];
  const next = (i) => {
    if (i < steps.length) {
      const [name, url] = steps[i];
      httpAsync("GET", url, undefined, (status, json) => {
        const data = v2Data(status, json);
        if (!data) {
          service.log(`Philips Hue: CLIP v2 ${name} not available (HTTP ${status}); using CLIP v1 only`);
          bridge.entertainment = null;
          announce(bridge);
          return;
        }
        replies[name] = data;
        next(i + 1);
      }, options);
      return;
    }
    const wanted = service.getSetting(bridge.id, "area");
    bridge.entertainment = entertainmentArea(replies.configs, replies.services, replies.lights, wanted);
    if (!bridge.entertainment) {
      announce(bridge);
      return;
    }
    // CLIP v2 streaming uses the application id as the PSK identity (GET /auth/v1 answers it in
    // the hue-application-id header); the device context falls back to the username.
    httpAsync("GET", `https://${bridge.ip}/auth/v1`, undefined, (status, json, xhr) => {
      const appId = status === 200 && xhr ? xhr.getResponseHeader("hue-application-id") : null;
      bridge.appId = appId ? String(appId) : "";
      announce(bridge);
    }, options);
  };
  next(0);
}

function fetchLights(bridge) {
  const state = rt(bridge.id);
  if (state.fetching || !bridge.username) return;
  state.fetching = true;
  httpAsync("GET", `http://${bridge.ip}/api/${bridge.username}/lights`, undefined, (status, json) => {
    if (errorType(json) === ERROR_UNAUTHORIZED || status === 401 || status === 403) {
      state.fetching = false;
      unpair(bridge, "The bridge no longer knows Lightloom: press Pair again");
      return;
    }
    if (status !== 200 || !json || typeof json !== "object" || Array.isArray(json)) {
      state.fetching = false;
      state.retryAt = Date.now() + RETRY_MS;
      setStatus(bridge, "Bridge not answering");
      return;
    }
    bridge.lights = colourLights(json);
    if (bridge.clientkey) {
      fetchEntertainment(bridge);
    } else {
      bridge.entertainment = null;
      announce(bridge);
    }
  });
}

function tryLink(bridge) {
  const state = rt(bridge.id);
  if (!state.linking || state.linkInFlight) return;
  state.linkInFlight = true;
  // generateclientkey: the bridge also returns the PSK of Entertainment streaming (API 1.22+).
  httpAsync("POST", `http://${bridge.ip}/api`, JSON.stringify({ devicetype: DEVICE_TYPE, generateclientkey: true }), (status, json) => {
    state.linkInFlight = false;
    if (!state.linking) return;
    const first = Array.isArray(json) ? json[0] : null;
    if (first && first.success && first.success.username) {
      state.linking = false;
      bridge.username = String(first.success.username);
      const clientkey = String(first.success.clientkey || "");
      bridge.clientkey = CLIENT_KEY.test(clientkey) ? clientkey : "";
      bridge.paired = true;
      service.saveSetting(bridge.id, "username", bridge.username);
      if (bridge.clientkey) service.saveSetting(bridge.id, "clientkey", bridge.clientkey);
      setStatus(bridge, "Paired: reading lamps");
      fetchLights(bridge);
    } else if (errorType(json) === ERROR_LINK_BUTTON || status === 0) {
      setStatus(bridge, "Press the link button on the bridge");
    } else {
      state.linking = false;
      const why = first && first.error && first.error.description ? first.error.description : `HTTP ${status}`;
      setStatus(bridge, `Pairing failed: ${why}`);
    }
  });
}

function probe(ip, manual) {
  httpAsync("GET", `http://${ip}/api/config`, undefined, (status, json) => {
    const id = status === 200 && json && typeof json === "object" ? normaliseId(json.bridgeid) : null;
    if (!id) {
      service.log(`Philips Hue: no bridge answered at ${ip}`);
      return;
    }
    addBridge(id, ip, { name: json.name, modelid: json.modelid, apiversion: json.apiversion }, manual);
  });
}

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/;

export function DiscoveryService() {
  this.IconUrl = "";
  this.MDns = ["_hue._tcp.local."];

  this.Initialize = function () {
    for (const id of readIndex()) {
      const ip = service.getSetting(id, "ip");
      if (ip && IPV4.test(ip)) addBridge(id, ip, {}, true);
    }
  };

  this.Update = function () {
    const now = Date.now();
    for (const wrapper of service.controllers) {
      const bridge = wrapper.obj || wrapper;
      if (!(bridge instanceof HueBridge)) continue;
      const state = rt(bridge.id);
      if (state.linking) {
        if (now - state.linkStart > LINK_WINDOW_MS) {
          state.linking = false;
          setStatus(bridge, "Pairing timed out: press Pair, then the bridge's link button");
        } else {
          tryLink(bridge);
        }
      } else if (bridge.paired && !state.announced && !state.fetching && now >= state.retryAt) {
        fetchLights(bridge);
      }
    }
  };

  this.Discovered = function (value) {
    if (!value || !value.ip || !IPV4.test(String(value.ip))) return;
    const ip = String(value.ip);
    const id = normaliseId(value.bridgeid);
    if (id) {
      addBridge(id, ip, { name: value.name, modelid: value.modelid }, false);
    } else {
      probe(ip, false);
    }
  };

  this.Removal = function () {
    // A bridge that stops answering mDNS keeps its controller: lamps keep their last colour.
  };

  this.forceDiscover = function (ip) {
    const address = String(ip || "").trim();
    if (!IPV4.test(address)) {
      service.log(`Philips Hue: '${address}' is not an IPv4 address`);
      return;
    }
    probe(address, true);
  };

  this.forgetController = function (id) {
    const bridge = service.getController(id);
    for (const key of ["username", "clientkey", "area", "ip"]) service.removeSetting(id, key);
    writeIndex(readIndex().filter((known) => known !== id));
    forgotten.add(id);
    runtime.delete(id);
    if (bridge) {
      service.suppressController(bridge);
      service.removeController(bridge);
    }
  };
}

// ---------------------------------------------------------------------------------------------
// Device context: one bridge, its lamps as subdevices.

let session = null;

function bridgeData() {
  return typeof controller === "object" && controller ? controller : {};
}

function subdeviceId(light) {
  return `light-${light.id}`;
}

function lightUrl(c, light) {
  return `http://${c.ip}/api/${c.username}/lights/${light.id}/state`;
}

function captureStates(c) {
  if (!c.ip || !c.username) return {};
  const reply = httpSync("GET", `http://${c.ip}/api/${c.username}/lights`);
  if (reply.status !== 200 || !reply.json || typeof reply.json !== "object" || Array.isArray(reply.json)) {
    device.log(`Philips Hue: could not read the lamps' state (HTTP ${reply.status}); they will not be restored`);
    return {};
  }
  const saved = {};
  for (const id of Object.keys(reply.json)) {
    const state = (reply.json[id] || {}).state;
    if (state && typeof state === "object") saved[id] = state;
  }
  return saved;
}

/** The area's channels whose lamp is one of this session's subdevices. */
function streamChannels(area) {
  if (!area || typeof area.id !== "string" || !Array.isArray(area.channels)) return [];
  const known = new Set(session.lights.map((l) => l.id));
  return area.channels.filter((ch) => ch && Number.isInteger(ch.id) && known.has(String(ch.light)));
}

function areaUrl(c) {
  return `https://${c.ip}${CLIP_V2}/entertainment_configuration/${c.entertainment.id}`;
}

/** Start streaming: the area over CLIP v2, then the DTLS handshake (it completes between calls). */
function startStreaming(c) {
  const channels = streamChannels(c.entertainment);
  if (channels.length === 0 || !CLIENT_KEY.test(String(c.clientkey || ""))) return;
  const reply = httpSync("PUT", areaUrl(c), JSON.stringify({ action: "start" }), 2000, { key: c.username, bridgeId: c.id });
  if (reply.status !== 200) {
    device.log(`Philips Hue: could not start entertainment area "${c.entertainment.name}" (HTTP ${reply.status}); using REST`);
    return;
  }
  session.stream = {
    channels,
    lamps: new Set(channels.map((ch) => String(ch.light))),
    configId: c.entertainment.id,
    // PSK identity: the application id (CLIP v2), then the username (Entertainment API v1).
    identities: [c.appId, c.username].filter((v, i, all) => v && all.indexOf(v) === i).map(String),
    attempt: 0,
    state: "starting",
    since: Date.now(),
    seq: 0,
  };
  device.addFeature("dtls");
  dtls.onConnectionEstablished(() => {
    if (!session || !session.stream) return;
    session.stream.state = "streaming";
    device.log(`Philips Hue: streaming to entertainment area "${c.entertainment.name}"`);
  });
  dtls.onConnectionError((message) => streamFailed(c, message));
  dtls.onConnectionClosed((message) => streamFailed(c, message || "the bridge closed the connection"));
  connect(c);
}

function connect(c) {
  const st = session.stream;
  st.state = "starting";
  st.since = Date.now();
  dtls.createConnection(c.ip, DTLS_PORT, st.identities[st.attempt], c.clientkey);
}

/** The handshake failed or the stream ended: try the next PSK identity, else REST for every lamp. */
function streamFailed(c, message) {
  const st = session && session.stream;
  if (!st || st.state === "stopping") return;
  if (st.state === "starting" && st.attempt + 1 < st.identities.length) {
    st.attempt += 1;
    connect(c);
    return;
  }
  device.log(`Philips Hue: entertainment streaming stopped (${message || "no reason"}); using REST`);
  stopStreaming(c, false);
}

/** End streaming: close DTLS and stop the area (synchronously on Shutdown). */
function stopStreaming(c, sync) {
  const st = session && session.stream;
  if (!st) return;
  st.state = "stopping";
  try {
    dtls.CloseConnection();
  } catch (e) {
    // already closed
  }
  const body = JSON.stringify({ action: "stop" });
  const options = { key: c.username, bridgeId: c.id };
  if (sync) httpSync("PUT", areaUrl(c), body, 1000, options);
  else httpAsync("PUT", areaUrl(c), body, () => {}, options);
  session.stream = null;
  // The streamed lamps now get REST commands for their current colour.
  for (const light of session.lights) light.sent = null;
}

/**
 * One HueStream v2 frame: "HueStream", version 2.0, sequence, 2 reserved, colour space 0 (RGB),
 * 1 reserved, the entertainment configuration id (36 ASCII characters), then per channel its id
 * and 16-bit big-endian R, G, B (8-bit value v as v * 257).
 */
function hueStreamFrame(st, colourOf) {
  const out = [];
  for (const ch of "HueStream") out.push(ch.charCodeAt(0));
  out.push(0x02, 0x00, st.seq & 0xff, 0x00, 0x00, 0x00, 0x00);
  for (const ch of st.configId) out.push(ch.charCodeAt(0));
  for (const channel of st.channels) {
    const [r, g, b] = colourOf(String(channel.light));
    out.push(channel.id & 0xff, r, r, g, g, b, b);
  }
  st.seq = (st.seq + 1) & 0xff;
  return out;
}

export function Initialize() {
  const c = bridgeData();
  session = { lights: [], next: 0, lastSendAt: 0, failureLogged: false, stream: null };
  device.setName(c.name ? String(c.name) : "Philips Hue bridge");
  device.setSize([1, 1]);
  device.setControllableLeds([], []);
  const lights = Array.isArray(c.lights) ? c.lights : [];
  const saved = captureStates(c);
  for (const light of lights) {
    if (!light || light.id === undefined) continue;
    const sid = subdeviceId(light);
    const name = String(light.name || `Light ${light.id}`);
    device.createSubdevice(sid);
    device.setSubdeviceName(sid, name);
    device.setSubdeviceSize(sid, 1, 1);
    device.setSubdeviceLeds(sid, [name], [[0, 0]]);
    session.lights.push({ id: String(light.id), sid, gamut: Array.isArray(light.gamut) ? light.gamut : null, target: [0, 0, 0], sent: null, inFlight: false, saved: saved[String(light.id)] || null });
  }
  if (c.ip && c.username && c.entertainment) startStreaming(c);
}

function changed(sent, target) {
  if (!sent) return true;
  const black = (c) => c[0] === 0 && c[1] === 0 && c[2] === 0;
  if (black(sent) !== black(target)) return true;
  return Math.abs(sent[0] - target[0]) >= CHANGE_THRESHOLD ||
    Math.abs(sent[1] - target[1]) >= CHANGE_THRESHOLD ||
    Math.abs(sent[2] - target[2]) >= CHANGE_THRESHOLD;
}

function commandFor(light, rgb) {
  const converted = rgbToXyBri(rgb, light.gamut);
  return converted ? { on: true, xy: converted.xy, bri: converted.bri, transitiontime: 1 } : { on: false };
}

function sendLight(c, light) {
  const rgb = light.target.slice();
  light.sent = rgb;
  light.inFlight = true;
  httpAsync("PUT", lightUrl(c, light), JSON.stringify(commandFor(light, rgb)), (status, json) => {
    light.inFlight = false;
    if (status !== 200 || errorType(json) !== 0) {
      light.sent = null;
      if (!session || !session.failureLogged) {
        if (session) session.failureLogged = true;
        device.log(`Philips Hue: lamp ${light.id} command failed (HTTP ${status}); further failures are not logged`);
      }
    }
  });
}

export function Render() {
  if (!session) return;
  const c = bridgeData();
  for (const light of session.lights) {
    const color = device.subdeviceColor(light.sid, 0, 0);
    light.target = [color[0] & 0xFF, color[1] & 0xFF, color[2] & 0xFF];
  }
  if (!c.ip || !c.username || session.lights.length === 0) return;
  if (session.stream && session.stream.state === "starting" && Date.now() - session.stream.since > HANDSHAKE_GIVE_UP_MS) {
    session.stream.attempt = session.stream.identities.length; // no more identities to try
    streamFailed(c, "the DTLS handshake did not complete");
  }
  const stream = session.stream;
  if (stream && stream.state === "streaming") {
    const byId = new Map(session.lights.map((l) => [l.id, l.target]));
    dtls.send(hueStreamFrame(stream, (id) => byId.get(id) || [0, 0, 0]), 1);
  }
  // REST for every lamp the stream does not cover (all of them without a stream).
  const now = Date.now();
  if (now - session.lastSendAt < SEND_INTERVAL_MS) return;
  const n = session.lights.length;
  for (let k = 0; k < n; k++) {
    const index = (session.next + k) % n;
    const light = session.lights[index];
    if (stream && stream.lamps.has(light.id)) continue;
    if (light.inFlight || !changed(light.sent, light.target)) continue;
    sendLight(c, light);
    session.next = (index + 1) % n;
    session.lastSendAt = now;
    return;
  }
}

/** The command that puts a lamp back in the state read at Initialize(). */
function restoreCommand(state) {
  if (state.on === false) return { on: false };
  const body = { on: true };
  if (typeof state.bri === "number") body.bri = state.bri;
  if (state.colormode === "ct" && typeof state.ct === "number") {
    body.ct = state.ct;
  } else if (state.colormode === "hs" && typeof state.hue === "number" && typeof state.sat === "number") {
    body.hue = state.hue;
    body.sat = state.sat;
  } else if (Array.isArray(state.xy) && state.xy.length === 2) {
    body.xy = [Number(state.xy[0]), Number(state.xy[1])];
  }
  return body;
}

export function Shutdown(suspend) {
  if (!session) return;
  const c = bridgeData();
  if (session.stream) stopStreaming(c, true);
  if (c.ip && c.username) {
    for (const light of session.lights) {
      if (!light.saved) continue;
      const reply = httpSync("PUT", lightUrl(c, light), JSON.stringify(restoreCommand(light.saved)), 1000);
      if (reply.status !== 200) device.log(`Philips Hue: could not restore lamp ${light.id} (HTTP ${reply.status})`);
    }
  }
  session = null;
}
