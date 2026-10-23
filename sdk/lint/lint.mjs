// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin lint: checks one plugin directory and returns findings, never throws for a
// broken plugin. What it checks:
//
//   * manifest.json against schemas/plugin-manifest-v2.schema.json (sdk/lint/schema.mjs), plus
//     what the schema cannot say: SMBus addresses inside 0x03..0x77, `tested` ids among the
//     claimed ones, the entry file exists, balanced licence parentheses;
//   * the plugin's exports, read the way the host reads them (sdk/lib/plugin.mjs): the
//     required lifecycle and metadata exports, `Type()`, `LedNames()` / `LedPositions()` /
//     `Size()`, `ControllableParameters()` against schemas/controllable-parameters.schema.json;
//   * exports against the manifest: `VendorId()` / `ProductId()` against the USB claims,
//     `HidUsages()` against the hidUsage claims, `SmbusAddresses()`, `SmbusDeniedRegisters()`,
//     `SpdManufacturers()`, `SmbusBatchWrites()` and `BrandGPUList()` against the SMBus claim;
//   * the sources for globals a HueDeck plugin does not have (DOM, Node, timers).
//
// plugins/_testkit/manifests.test.mjs runs this over every first-party plugin.

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { basename, dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockBus } from "../../plugins/_testkit/mock-bus.mjs";
import { createSimDevice, FEATURE_GLOBALS, importPlugin, installGlobals, readExports } from "../lib/plugin.mjs";
import { deepEqual, validate } from "./schema.mjs";

const schemas = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "schemas");
const loadSchema = (name) => JSON.parse(readFileSync(join(schemas, name), "utf8"));
export const MANIFEST_SCHEMA = loadSchema("plugin-manifest-v2.schema.json");
export const PARAMETERS_SCHEMA = loadSchema("controllable-parameters.schema.json");

/** @typedef {{ level: "error" | "warning", area: "manifest" | "exports" | "source", code: string, message: string, where?: string }} Finding */

const TYPES = ["hid", "rawusb", "hybrid", "serial", "smbus", "network"];
const DEVICE_TYPES = MANIFEST_SCHEMA.properties.deviceType.enum;
// Globals the host defines; a ControllableParameters property with one of these names would replace it.
const HOST_GLOBALS = ["device", "bus", "console", "service", "controller", "discovery", "XmlHttp", "XMLHttpRequest", "BinaryUtils", "keyboard", "mouse", "battery", "udp", "dtls", "base64"];

/** Globals a plugin must not use: the HueDeck host is QuickJS, not a browser and not Node. */
export const FORBIDDEN_GLOBALS = Object.freeze({
  // DOM and browser APIs (SignalRGB plugins have none either; effects do, plugins do not).
  window: "DOM", document: "DOM", navigator: "DOM", location: "DOM", localStorage: "DOM", sessionStorage: "DOM",
  indexedDB: "DOM", fetch: "DOM", alert: "DOM", confirm: "DOM", prompt: "DOM", HTMLElement: "DOM",
  HTMLCanvasElement: "DOM", Image: "DOM", ImageData: "DOM", OffscreenCanvas: "DOM", WebSocket: "DOM",
  Worker: "DOM", requestAnimationFrame: "DOM", cancelAnimationFrame: "DOM", Blob: "DOM", FileReader: "DOM",
  // Node.js.
  require: "Node.js", process: "Node.js", Buffer: "Node.js", __dirname: "Node.js", __filename: "Node.js",
  module: "Node.js", global: "Node.js",
  // Timers: the host has no event loop between calls; use device.pause(ms) or count Render() frames.
  setTimeout: "timer", setInterval: "timer", clearTimeout: "timer", clearInterval: "timer", setImmediate: "timer",
});

const hexOrNumber = (v) => (typeof v === "number" ? v : /^(0x)?[0-9a-f]+$/i.test(String(v).trim()) ? parseInt(String(v).trim().replace(/^0x/i, ""), 16) : NaN);
const decOrHex = (v) => (typeof v === "number" ? v : /^0x/i.test(String(v)) ? parseInt(String(v), 16) : Number(v));
const hex = (n, width = 4) => `0x${Number(n).toString(16).toUpperCase().padStart(width, "0")}`;
const sortNum = (a) => [...a].sort((x, y) => x - y);
const show = (v) => JSON.stringify(v);

/**
 * Lint the plugin in `dir`.
 * @param {string} dir
 * @returns {Promise<{ dir: string, id?: string, version?: string, entry?: string, findings: Finding[] }>}
 */
export async function lintPlugin(dir) {
  /** @type {Finding[]} */
  const findings = [];
  const add = (level, area, code, message, where) => findings.push(where === undefined ? { level, area, code, message } : { level, area, code, message, where });
  const result = { dir, findings };

  // ------------------------------------------------------------------ manifest
  const manifestPath = join(dir, "manifest.json");
  let manifest;
  if (existsSync(manifestPath)) {
    try {
      manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    } catch (e) {
      add("error", "manifest", "manifest-json", `manifest.json is not valid JSON: ${e.message}`);
    }
  } else {
    add("warning", "manifest", "no-manifest", "no manifest.json: HueDeck loads it as a compat-v1 plugin from its exports alone; add a manifest (schemas/plugin-manifest-v2.schema.json) to declare devices and permissions");
  }
  if (manifest !== undefined) {
    result.id = manifest.id;
    result.version = manifest.version;
    for (const e of validate(MANIFEST_SCHEMA, manifest)) {
      add("error", "manifest", "schema", e.message, e.path || "/");
    }
    checkManifest(manifest, dir, add);
  }

  // ------------------------------------------------------------------ entry
  const entry = findEntry(dir, manifest);
  if (entry === undefined) {
    add("error", "exports", "no-entry", "cannot find the entry module: name it in manifest.json \"entry\", or call it plugin.js or index.js");
    return result;
  }
  result.entry = relative(dir, entry).replace(/\\/g, "/");
  if (!existsSync(entry)) {
    return result; // reported by checkManifest
  }

  // ------------------------------------------------------------------ sources
  for (const file of sourceFiles(dir)) {
    for (const hit of scanForbiddenGlobals(readFileSync(file, "utf8"))) {
      add("error", "source", "forbidden-global", `${hit.name} is a ${FORBIDDEN_GLOBALS[hit.name]} global the HueDeck plugin host does not have`, `${relative(dir, file).replace(/\\/g, "/")}:${hit.line}`);
    }
  }

  // ------------------------------------------------------------------ exports
  // The host loads every module of the directory that exports plugin metadata, so sibling
  // modules (one per VendorId(), e.g.) are plugins too; helper modules are skipped.
  const modules = [];
  for (const file of [entry, ...siblingModules(dir, entry)]) {
    const device = createSimDevice();
    const mockBus = createMockBus({ kind: "amd" });
    const restore = installGlobals({ device, bus: mockBus.fixed, ...FEATURE_GLOBALS });
    const where = relative(dir, file).replace(/\\/g, "/");
    try {
      const ns = await importPlugin(file);
      const reading = readExports(ns);
      // Network plugins: the DiscoveryService constructor only sets fields, so it runs here.
      if (typeof ns.DiscoveryService === "function") {
        try {
          const d = new ns.DiscoveryService();
          reading.mdns = Array.isArray(d.MDns) ? d.MDns.map((t) => String(t).replace(/\.local\.?$/, "")) : [];
        } catch (e) {
          add("error", "exports", "discovery-service", `new DiscoveryService() throws: ${e?.message ?? e}`, where);
        }
      }
      if (file === entry || isPluginModule(reading)) modules.push({ where, reading });
    } catch (e) {
      if (file === entry) add("error", "exports", "load", `the module does not load: ${e?.name ?? "Error"}: ${e?.message ?? e}`, where);
    } finally {
      restore();
    }
  }
  const multi = modules.length > 1;
  for (const { where, reading } of modules) {
    const before = findings.length;
    checkExports(reading, add);
    if (multi) for (const f of findings.slice(before)) f.where = f.where ? `${where} ${f.where}` : where;
  }
  if (manifest !== undefined && modules.length > 0) checkExportsAgainstManifest(modules.map((m) => m.reading), manifest, add);
  result.modules = modules.map((m) => m.where);
  return result;
}

// Top-level .js/.mjs files of `dir` other than the entry.
function siblingModules(dir, entry) {
  return readdirSync(dir)
    .filter((f) => /\.m?js$/.test(f) && !/\.test\.m?js$/.test(f))
    .map((f) => join(dir, f))
    .filter((f) => f !== entry);
}

// Whether a module's exports make it a plugin the host would load (not a helper module).
function isPluginModule(reading) {
  const ex = reading.exports;
  return ("VendorId" in ex && "ProductId" in ex) || "HidUsages" in ex || "SmbusAddresses" in ex || reading.lifecycle.DiscoveryService;
}

/** The entry module of a plugin directory (manifest `entry`, else plugin.js / index.js / the only .js). */
export function findEntry(dir, manifest) {
  if (manifest && typeof manifest.entry === "string") return join(dir, manifest.entry);
  for (const name of ["plugin.js", "index.js", "plugin.mjs", "index.mjs"]) {
    if (existsSync(join(dir, name))) return join(dir, name);
  }
  const js = readdirSync(dir).filter((f) => /\.m?js$/.test(f) && !/\.test\.m?js$/.test(f));
  return js.length === 1 ? join(dir, js[0]) : undefined;
}

// Plugin sources: every .js/.mjs below `dir` except tests and fixtures.
function sourceFiles(dir) {
  const out = [];
  const walk = (d) => {
    for (const ent of readdirSync(d, { withFileTypes: true })) {
      if (ent.isDirectory()) {
        if (!["tests", "test", "fixtures", "node_modules"].includes(ent.name) && !ent.name.startsWith(".")) walk(join(d, ent.name));
      } else if (/\.m?js$/.test(ent.name) && !/\.test\.m?js$/.test(ent.name)) {
        out.push(join(d, ent.name));
      }
    }
  };
  walk(dir);
  return out;
}

// ---------------------------------------------------------------------- manifest checks

function checkManifest(manifest, dir, add) {
  if (typeof manifest.entry === "string" && !existsSync(join(dir, manifest.entry))) {
    add("error", "manifest", "entry-missing", `entry '${manifest.entry}' does not exist`, "/entry");
  }
  if (typeof manifest.license === "string") {
    let depth = 0;
    for (const ch of manifest.license) {
      depth += ch === "(" ? 1 : ch === ")" ? -1 : 0;
      if (depth < 0) break;
    }
    if (depth !== 0) add("error", "manifest", "license-parens", `unbalanced parentheses in licence expression '${manifest.license}'`, "/license");
  }
  const devices = Array.isArray(manifest.devices) ? manifest.devices : [];
  devices.forEach((claim, i) => {
    if (!claim || typeof claim !== "object") return;
    const at = `/devices/${i}`;
    if (Array.isArray(claim.pids) && Array.isArray(claim.tested)) {
      const pids = claim.pids.map(hexOrNumber);
      for (const t of claim.tested) {
        if (!pids.includes(hexOrNumber(t))) add("error", "manifest", "tested-unclaimed", `tested ${t} is not one of the claimed pids`, `${at}/tested`);
      }
    }
    const smbus = claim.smbus;
    if (smbus && typeof smbus === "object") {
      for (const a of Array.isArray(smbus.addresses) ? smbus.addresses : []) {
        if (Number.isInteger(a) && (a < 0x03 || a > 0x77)) {
          add("error", "manifest", "smbus-address", `address ${hex(a, 2)} is outside 0x03..0x77 (the host refuses it)`, `${at}/smbus/addresses`);
        }
      }
      const tuple = (t) => ["vendor", "device", "subVendor", "subDevice"].map((k) => hexOrNumber(t?.[k])).join(":");
      const cards = (Array.isArray(smbus.pci) ? smbus.pci : []).map(tuple);
      for (const t of Array.isArray(smbus.tested) ? smbus.tested : []) {
        if (!cards.includes(tuple(t))) add("error", "manifest", "tested-unclaimed", `tested card ${tuple(t)} is not one of the claimed pci cards`, `${at}/smbus/tested`);
      }
      if (manifest.deviceType === "ram" && cards.length === 0 && !(Array.isArray(smbus.spdManufacturers) && smbus.spdManufacturers.length > 0)) {
        add("error", "manifest", "ram-spd", "a RAM plugin must declare smbus.spdManufacturers: without it the host never offers it a chipset bus", `${at}/smbus`);
      }
    }
  });
  const perms = Array.isArray(manifest.permissions) ? manifest.permissions : [];
  const needs = (perm, why) => {
    if (!perms.includes(perm)) add("warning", "manifest", "permission", `devices claim ${why} but permissions do not list '${perm}'`, "/permissions");
  };
  for (const claim of devices) {
    if (!claim || typeof claim !== "object") continue;
    if ("hidUsage" in claim || (("vid" in claim) && (claim.transport ?? "hid") === "hid")) needs("hid", "HID devices");
    if ("vid" in claim && (claim.transport === "rawusb" || claim.transport === "hybrid")) needs("rawusb", `${claim.transport} devices`);
    if ("vid" in claim && claim.transport === "serial") needs("serial", "serial devices");
    if (claim.smbus && typeof claim.smbus === "object") {
      if (Array.isArray(claim.smbus.pci) && claim.smbus.pci.length > 0) needs("gpu-i2c", "GPU I2C cards");
      else needs("smbus", "an SMBus bus");
    }
  }
}

// ---------------------------------------------------------------------- export checks

function checkExports(reading, add) {
  const { exports: ex, errors, lifecycle } = reading;
  for (const [name, message] of Object.entries(errors)) {
    add("error", "exports", "export-throws", `${name}() throws: ${message}`, name);
  }
  const type = ex.Type === undefined ? "hid" : String(ex.Type).toLowerCase();
  if (ex.Type !== undefined && !TYPES.includes(type)) {
    add("error", "exports", "type", `Type() returns ${show(ex.Type)}; expected one of Hid, rawusb, hybrid, serial, SMBUS, network`, "Type");
  }
  const required = (name, why) => {
    if (!(name in ex) && !(name in errors) && !lifecycle[name]) add("error", "exports", "missing-export", `${name} is not exported (${why})`, name);
  };
  required("Name", "every plugin");
  if (type !== "network") {
    for (const fn of ["Initialize", "Render", "Shutdown"]) required(fn, "every device plugin");
  }
  if (type === "smbus") {
    required("Scan", "SMBus plugins: the host calls Scan(bus) with the free-address bus");
    required("SmbusAddresses", "HueDeck runs no SMBus plugin without it (docs/host-api/smbus.md)");
  } else if (type === "network") {
    required("DiscoveryService", "network plugins");
  } else if (!("HidUsages" in ex)) {
    required("VendorId", "USB plugins (or export HidUsages() for a generic HID driver)");
    required("ProductId", "USB plugins (or export HidUsages() for a generic HID driver)");
    if (["hid", "hybrid"].includes(type) && !lifecycle.Validate) {
      add("warning", "exports", "missing-export", "Validate(endpoint) is not exported: every HID collection of the device will be accepted", "Validate");
    }
  }
  if ("VendorId" in ex) {
    const v = ex.VendorId;
    if (!Number.isInteger(v) || v < 0 || v > 0xffff) add("error", "exports", "vendor-id", `VendorId() returns ${show(v)}; expected one 16-bit number`, "VendorId");
  }
  if ("ProductId" in ex) {
    const list = Array.isArray(ex.ProductId) ? ex.ProductId : [ex.ProductId];
    for (const p of list) {
      const n = decOrHex(p);
      if (!Number.isInteger(n) || n < 0 || n > 0xffff) add("error", "exports", "product-id", `ProductId() has ${show(p)}; expected 16-bit numbers (or decimal strings)`, "ProductId");
    }
  }
  if ("HidUsages" in ex) {
    const ok = Array.isArray(ex.HidUsages) && ex.HidUsages.every((u) => Array.isArray(u) && u.length === 2 && u.every((n) => Number.isInteger(n) && n >= 0 && n <= 0xffff));
    if (!ok) add("error", "exports", "hid-usages", `HidUsages() returns ${show(ex.HidUsages)}; expected [[usagePage, usage], ...]`, "HidUsages");
  }
  if ("DeviceType" in ex && !DEVICE_TYPES.includes(String(ex.DeviceType).toLowerCase())) {
    add("warning", "exports", "device-type", `DeviceType() returns ${show(ex.DeviceType)}; known types: ${DEVICE_TYPES.join(", ")}`, "DeviceType");
  }

  // Geometry.
  let size;
  if ("Size" in ex) {
    const s = ex.Size;
    if (Array.isArray(s) && s.length === 2 && s.every((n) => Number.isInteger(n) && n > 0)) size = s;
    else add("error", "exports", "size", `Size() returns ${show(s)}; expected [width, height] in whole cells, both > 0`, "Size");
  }
  const names = ex.LedNames;
  const positions = ex.LedPositions;
  if ("LedNames" in ex && !(Array.isArray(names) && names.every((n) => typeof n === "string"))) {
    add("error", "exports", "led-names", `LedNames() returns ${show(names)}; expected an array of strings`, "LedNames");
  }
  if ("LedPositions" in ex) {
    const ok = Array.isArray(positions) && positions.every((p) => Array.isArray(p) && p.length >= 2 && Number.isInteger(p[0]) && Number.isInteger(p[1]));
    if (!ok) {
      add("error", "exports", "led-positions", "LedPositions() must return [[x, y], ...] with whole-cell coordinates", "LedPositions");
    } else {
      if (Array.isArray(names) && names.length !== positions.length) {
        add("error", "exports", "led-count", `LedNames() has ${names.length} names but LedPositions() ${positions.length} positions`, "LedPositions");
      }
      if (size) {
        const outside = positions.findIndex(([x, y]) => x < 0 || y < 0 || x >= size[0] || y >= size[1]);
        if (outside >= 0) add("error", "exports", "led-outside", `LedPositions()[${outside}] ${show(positions[outside])} lies outside Size() ${show(size)}`, "LedPositions");
      }
      const seen = new Map();
      positions.forEach(([x, y], i) => {
        const k = `${x},${y}`;
        if (seen.has(k)) add("warning", "exports", "led-overlap", `LEDs ${seen.get(k)} and ${i} share the cell [${x}, ${y}]: they always show the same colour`, "LedPositions");
        else seen.set(k, i);
      });
    }
  } else if ("LedNames" in ex) {
    add("error", "exports", "led-count", "LedNames() is exported but LedPositions() is not", "LedPositions");
  }

  // Parameters.
  if ("ControllableParameters" in ex) {
    const params = ex.ControllableParameters;
    for (const e of validate(PARAMETERS_SCHEMA, params)) {
      add("error", "exports", "parameters", e.message, `ControllableParameters${e.path}`);
    }
    if (Array.isArray(params)) {
      const seen = new Set();
      params.forEach((p, i) => {
        if (!p || typeof p.property !== "string") return;
        if (seen.has(p.property)) add("error", "exports", "parameters", `property '${p.property}' is declared twice`, `ControllableParameters/${i}`);
        seen.add(p.property);
        if (HOST_GLOBALS.includes(p.property)) add("error", "exports", "parameters", `property '${p.property}' would replace the host global of that name`, `ControllableParameters/${i}`);
        const type = String(p.type ?? "").toLowerCase();
        if ((type === "combobox" || type === "list") && Array.isArray(p.values) && p.default !== undefined) {
          const keys = p.values.map((v) => (v && typeof v === "object" ? v.key : v));
          if (!keys.some((k) => String(k) === String(p.default))) {
            add("warning", "exports", "parameters", `default ${show(p.default)} of '${p.property}' is not one of its values`, `ControllableParameters/${i}`);
          }
        }
        if (type === "color" && typeof p.default === "string" && !/^#[0-9A-Fa-f]{6}$/.test(p.default)) {
          add("warning", "exports", "parameters", `default ${show(p.default)} of colour '${p.property}' is not #RRGGBB`, `ControllableParameters/${i}`);
        }
      });
    }
  }
}

function checkExportsAgainstManifest(readings, manifest, add) {
  const ex = readings[0].exports;
  const devices = Array.isArray(manifest.devices) ? manifest.devices.filter((c) => c && typeof c === "object") : [];
  const usb = devices.filter((c) => "vid" in c);
  const usages = devices.filter((c) => "hidUsage" in c);
  const smbus = devices.filter((c) => "smbus" in c);
  const network = devices.filter((c) => "network" in c);
  const type = ex.Type === undefined ? "hid" : String(ex.Type).toLowerCase();

  // USB: VendorId / ProductId of every plugin module against the claims.
  const exportedIds = new Map(); // vid -> Set(pid)
  for (const r of readings) {
    const e = r.exports;
    if (!("VendorId" in e) || !("ProductId" in e)) continue;
    const set = exportedIds.get(e.VendorId) ?? new Set();
    for (const p of Array.isArray(e.ProductId) ? e.ProductId : [e.ProductId]) set.add(decOrHex(p));
    exportedIds.set(e.VendorId, set);
  }
  if (usb.length > 0 || exportedIds.size > 0) {
    const claimed = new Map();
    for (const c of usb) {
      const vid = hexOrNumber(c.vid);
      const set = claimed.get(vid) ?? new Set();
      for (const p of Array.isArray(c.pids) ? c.pids : []) set.add(hexOrNumber(p));
      claimed.set(vid, set);
    }
    for (const [vid, pids] of exportedIds) {
      for (const p of pids) {
        if (!claimed.get(vid)?.has(p)) add("error", "manifest", "pid-unclaimed", `VendorId()/ProductId() list ${hex(vid)}:${hex(p)}, which no manifest claim lists`, "/devices");
      }
    }
    for (const [vid, pids] of claimed) {
      for (const p of pids) {
        if (!exportedIds.get(vid)?.has(p)) add("error", "manifest", "pid-unexported", `the manifest claims ${hex(vid)}:${hex(p)}, which no plugin module's VendorId()/ProductId() lists`, "/devices");
      }
    }
    const types = new Set(readings.filter((r) => "VendorId" in r.exports).map((r) => (r.exports.Type === undefined ? "hid" : String(r.exports.Type).toLowerCase())));
    for (const c of usb) {
      const transport = c.transport ?? "hid";
      if (types.size > 0 && !types.has(transport) && !types.has("smbus") && !types.has("network")) {
        add("warning", "manifest", "transport", `claim transport '${transport}' differs from Type() ${show([...types])}`, "/devices");
      }
    }
  }

  // Generic HID drivers.
  if (usages.length > 0 || "HidUsages" in ex) {
    const fromManifest = usages.map((c) => `${hexOrNumber(c.hidUsage?.usagePage)}:${hexOrNumber(c.hidUsage?.usage)}`).sort();
    const fromExport = (Array.isArray(ex.HidUsages) ? ex.HidUsages : []).map((u) => `${u?.[0]}:${u?.[1]}`).sort();
    if (!deepEqual(fromManifest, fromExport)) {
      add("error", "manifest", "hid-usage-mismatch", `hidUsage claims [${fromManifest}] differ from HidUsages() [${fromExport}] (page:usage, decimal)`, "/devices");
    }
  }

  // SMBus.
  if (type === "smbus" && smbus.length === 0) {
    add("error", "manifest", "smbus-claim", "Type() is SMBUS but the manifest has no smbus claim", "/devices");
  }
  if (smbus.length > 0 && type !== "smbus") {
    add("error", "manifest", "smbus-claim", `the manifest has an smbus claim but Type() returns ${show(ex.Type)}`, "/devices");
  }
  for (const c of smbus) {
    const s = c.smbus ?? {};
    const same = (what, a, b) => {
      if (!deepEqual(a, b)) add("error", "manifest", "smbus-mismatch", `${what}: export ${show(a)}, manifest ${show(b)}`, "/devices");
    };
    same("SmbusAddresses() differs from smbus.addresses", ex.SmbusAddresses, s.addresses);
    same("SmbusDeniedRegisters() differs from smbus.deniedRegisters", ex.SmbusDeniedRegisters ?? [], s.deniedRegisters ?? []);
    same("SpdManufacturers() differs from smbus.spdManufacturers", ex.SpdManufacturers ?? [], s.spdManufacturers ?? []);
    same("SmbusBatchWrites() differs from smbus.batchWrites", ex.SmbusBatchWrites === true, s.batchWrites === true);
    const gpus = Array.isArray(ex.BrandGPUList) ? ex.BrandGPUList : [];
    const fromExport = gpus.map((g) => [g?.Vendor, g?.Device, g?.SubVendor, g?.SubDevice].map(Number).join(":")).sort();
    const fromManifest = (Array.isArray(s.pci) ? s.pci : []).map((t) => [t?.vendor, t?.device, t?.subVendor, t?.subDevice].map(hexOrNumber).join(":")).sort();
    if (!deepEqual([...new Set(fromExport)], [...new Set(fromManifest)])) {
      add("error", "manifest", "smbus-mismatch", `BrandGPUList() cards [${fromExport}] differ from smbus.pci [${fromManifest}]`, "/devices");
    }
    if (Array.isArray(ex.SmbusBuses) && Array.isArray(s.buses)) {
      const norm = (list) => sortNum(list.map((b) => String(b).toLowerCase()).filter((b) => b !== "any").map((b) => ["system", "amd", "intel", "nvidia", "amdgpu", "nuvoton"].indexOf(b)));
      if (!deepEqual(norm(ex.SmbusBuses), norm(s.buses))) {
        add("warning", "manifest", "smbus-mismatch", `SmbusBuses() ${show(ex.SmbusBuses)} differs from smbus.buses ${show(s.buses)}`, "/devices");
      }
    }
  }

  // Network.
  if (type === "network" && network.length === 0) {
    add("error", "manifest", "network-claim", "Type() is network but the manifest has no network claim", "/devices");
  }
  if (network.length > 0 && type !== "network") {
    add("error", "manifest", "network-claim", `the manifest has a network claim but Type() returns ${show(ex.Type)}`, "/devices");
  }
  for (const c of network) {
    const mdns = Array.isArray(c.network?.mdns) ? [...c.network.mdns].sort() : [];
    const browsed = [...(readings[0].mdns ?? [])].sort();
    if (!deepEqual(mdns, browsed)) {
      add("error", "manifest", "network-mismatch", `DiscoveryService MDns ${show(browsed)} (without .local.) differ from network.mdns ${show(mdns)}`, "/devices");
    }
    const perms = Array.isArray(manifest.permissions) ? manifest.permissions : [];
    if (mdns.length > 0 && !perms.includes("mdns")) {
      add("error", "manifest", "permission", "a plugin that browses mDNS needs the 'mdns' permission", "/permissions");
    }
  }

  // Informational fields.
  if (typeof manifest.deviceType === "string" && "DeviceType" in ex && String(ex.DeviceType).toLowerCase() !== manifest.deviceType) {
    add("warning", "manifest", "device-type", `deviceType '${manifest.deviceType}' differs from DeviceType() ${show(ex.DeviceType)}`, "/deviceType");
  }
  if (typeof manifest.version === "string" && typeof ex.Version === "string" && ex.Version !== manifest.version) {
    add("warning", "manifest", "version", `version '${manifest.version}' differs from Version() '${ex.Version}'`, "/version");
  }
}

// ---------------------------------------------------------------------- source scan

/**
 * Blank out comments, strings, template literals and regular-expression literals (newlines
 * kept, so line numbers still match). A heuristic tokenizer, good enough for a lint.
 */
export function stripCode(src) {
  let out = "";
  let i = 0;
  let prevSignificant = "";
  const blank = (s) => s.replace(/[^\n]/g, " ");
  while (i < src.length) {
    const c = src[i];
    const n = src[i + 1];
    let end;
    if (c === "/" && n === "/") {
      end = src.indexOf("\n", i);
      if (end < 0) end = src.length;
    } else if (c === "/" && n === "*") {
      end = src.indexOf("*/", i + 2);
      end = end < 0 ? src.length : end + 2;
    } else if (c === '"' || c === "'" || c === "`") {
      end = i + 1;
      while (end < src.length && src[end] !== c) {
        if (src[end] === "\\") end++;
        else if (c !== "`" && src[end] === "\n") break;
        end++;
      }
      end = Math.min(end + 1, src.length);
    } else if (c === "/" && (prevSignificant === "" || /[(,=:[!&|?{};+\-*%<>~^]/.test(prevSignificant) || /\b(return|typeof|case|in|of)$/.test(out.trimEnd()))) {
      end = i + 1;
      let inClass = false;
      while (end < src.length && src[end] !== "\n") {
        if (src[end] === "\\") end++;
        else if (src[end] === "[") inClass = true;
        else if (src[end] === "]") inClass = false;
        else if (src[end] === "/" && !inClass) break;
        end++;
      }
      end++;
      while (end < src.length && /[a-z]/i.test(src[end])) end++;
    }
    if (end !== undefined) {
      out += blank(src.slice(i, end));
      i = end;
      prevSignificant = c === "/" && n !== "/" && n !== "*" ? "x" : prevSignificant;
      if (c === '"' || c === "'" || c === "`") prevSignificant = "x";
      continue;
    }
    out += c;
    if (!/\s/.test(c)) prevSignificant = c;
    i++;
  }
  return out;
}

/** Uses of FORBIDDEN_GLOBALS in `src`: [{ name, line }]; locally declared names are not globals. */
export function scanForbiddenGlobals(src) {
  const code = stripCode(src);
  const declared = new Set();
  const decl = /\b(?:function\*?|class|const|let|var)\s+([A-Za-z_$][\w$]*)|\bimport\s+([A-Za-z_$][\w$]*)|\bas\s+([A-Za-z_$][\w$]*)/g;
  for (const m of code.matchAll(decl)) declared.add(m[1] ?? m[2] ?? m[3]);
  for (const m of code.matchAll(/\bimport\s*\{([^}]*)\}/g)) {
    for (const part of m[1].split(",")) {
      const name = part.trim().split(/\s+as\s+/).pop();
      if (name) declared.add(name);
    }
  }
  const hits = [];
  const names = Object.keys(FORBIDDEN_GLOBALS).filter((n) => !declared.has(n));
  if (names.length === 0) return hits;
  const re = new RegExp(`(?<![\\w$.])(?<!\\?\\.)(${names.map((n) => n.replace(/\$/g, "\\$")).join("|")})(?![\\w$])`, "g");
  for (const m of code.matchAll(re)) {
    const before = code.slice(Math.max(0, m.index - 8), m.index);
    const after = code.slice(m.index + m[0].length, m.index + m[0].length + 3);
    if (/typeof\s+$/.test(before)) continue; // feature test: `typeof window !== "undefined"`
    if (/^\s*:(?!:)/.test(after) && /[{,]\s*$/.test(code.slice(0, m.index))) continue; // object key
    const line = code.slice(0, m.index).split("\n").length;
    hits.push({ name: m[1], line });
  }
  return hits;
}

// ---------------------------------------------------------------------- directories and output

/** Plugin directories under `dir`: `dir` itself when it holds a plugin, else its subdirectories that do. */
export function pluginDirs(dir) {
  const isPlugin = (d) => existsSync(join(d, "manifest.json")) || findEntry(d, undefined) !== undefined;
  if (isPlugin(dir)) return [dir];
  return readdirSync(dir)
    .filter((name) => !name.startsWith("_") && !name.startsWith("."))
    .map((name) => join(dir, name))
    .filter((d) => statSync(d).isDirectory() && isPlugin(d));
}

/** A readable report for one lint result. */
export function formatResult(result, { color = false } = {}) {
  const paint = (code, s) => (color ? `\x1b[${code}m${s}\x1b[0m` : s);
  const title = [basename(result.dir), result.id && result.id !== basename(result.dir) ? `(${result.id}${result.version ? ` ${result.version}` : ""})` : result.version ? `(${result.version})` : ""].filter(Boolean).join(" ");
  const lines = [paint("1", title)];
  for (const f of result.findings) {
    const level = f.level === "error" ? paint("31", "error  ") : paint("33", "warning");
    lines.push(`  ${level} ${f.area.padEnd(8)} ${f.where ? `${paint("2", f.where)}: ` : ""}${f.message} ${paint("2", `[${f.code}]`)}`);
  }
  const errors = result.findings.filter((f) => f.level === "error").length;
  const warnings = result.findings.length - errors;
  lines.push(errors + warnings === 0 ? `  ${paint("32", "ok")}` : `  ${errors} error(s), ${warnings} warning(s)`);
  return lines.join("\n");
}
