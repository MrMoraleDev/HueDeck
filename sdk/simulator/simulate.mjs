// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin simulator: runs a plugin's lifecycle under Node against plugins/_testkit's
// mock device and mock SMBus bus, with the canvas showing one solid colour, and records what
// the plugin sends. Nothing here opens hardware.
//
//   HID-class plugins:  Validate(endpoint) for each simulated endpoint, Initialize(),
//                       Render() x frames, Shutdown(suspending).
//   SMBus plugins:      Scan(freeBus) on a mock bus where the plugin's SmbusAddresses() answer
//                       (or the registers you give), then the same session on the fixed bus
//                       bound to the first address Scan() returned.
//
// The order and the parameter handling follow the host: ControllableParameters() defaults are
// bound as globals (quoted numbers and booleans coerced) before Initialize(), overrides after.

import { join } from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { createMockBus } from "../../plugins/_testkit/mock-bus.mjs";
import { bytesToHex } from "../../plugins/_testkit/mock-device.mjs";
import { findEntry } from "../lint/lint.mjs";
import { createSimDevice, FEATURE_GLOBALS, importPlugin, installGlobals, parameterDefaults, readExports } from "../lib/plugin.mjs";

/** `#RRGGBB` / `RRGGBB` / [r, g, b] -> [r, g, b]. */
export function parseColor(color) {
  if (Array.isArray(color)) return color.slice(0, 3).map((n) => Number(n) & 0xff);
  const m = /^#?([0-9a-f]{6})$/i.exec(String(color).trim());
  if (!m) throw new Error(`colour '${color}' is not #RRGGBB`);
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

// Node globals the QuickJS host does not have: a plugin calling one fails as it would in HueDeck.
function missingGlobal(name) {
  return () => {
    throw new ReferenceError(`${name} is not defined (the HueDeck plugin host has no ${name})`);
  };
}
const NOT_IN_HOST = Object.fromEntries(
  ["setTimeout", "setInterval", "setImmediate", "clearTimeout", "clearInterval", "fetch"].map((n) => [n, missingGlobal(n)]),
);

// Calls that are not traffic: canvas sampling is summarised, not listed.
const QUIET = new Set(["color", "getBrightness", "subdeviceColor"]);

/**
 * Simulate the plugin in `dir`.
 * @param {string} dir  plugin directory (or a plugin .js file)
 * @param {object} [options]
 * @param {string | number[]} [options.color]  canvas colour, default "#FF0000"
 * @param {number} [options.frames]  Render() calls, default 1
 * @param {Record<string, unknown>} [options.params]  parameter overrides
 * @param {boolean} [options.suspending]  Shutdown(true): the system is going to sleep
 * @param {number} [options.productId]  default: the first ProductId()
 * @param {object[]} [options.endpoints]  HID endpoints offered to Validate()
 * @param {object} [options.bus]  createMockBus() options for SMBus plugins (kind, registers, ...)
 * @param {string} [options.module]  plugin module file inside `dir` (default: the entry)
 */
export async function simulate(dir, options = {}) {
  const entry = options.module
    ? join(dir, options.module)
    : dir.endsWith(".js") || dir.endsWith(".mjs")
      ? dir
      : findEntry(dir, readManifest(dir));
  if (!entry || !existsSync(entry)) throw new Error(`no plugin entry module in ${dir}`);
  const rgb = parseColor(options.color ?? "#FF0000");
  const frames = Math.max(0, options.frames ?? 1);
  const out = { plugin: entry, color: rgb, phases: [], log: [], ok: true };

  // Read the metadata first, in a context of its own (as the host's catalogue scan does).
  const scratch = installGlobals({ device: createSimDevice(), bus: createMockBus().fixed });
  let meta;
  try {
    meta = readExports(await importPlugin(entry));
  } finally {
    scratch();
  }
  const ex = meta.exports;
  out.name = ex.Name;
  out.type = ex.Type === undefined ? "Hid" : String(ex.Type);
  const smbus = out.type.toLowerCase() === "smbus";
  const params = { ...parameterDefaults(ex.ControllableParameters), ...(options.params ?? {}) };

  if (smbus) {
    await simulateSmbus(entry, ex, params, rgb, frames, options, out);
  } else {
    await simulateHid(entry, ex, params, rgb, frames, options, out);
  }
  return out;
}

function readManifest(dir) {
  const p = join(dir, "manifest.json");
  if (!existsSync(p)) return undefined;
  try {
    return JSON.parse(readFileSync(p, "utf8"));
  } catch {
    return undefined;
  }
}

async function simulateHid(entry, ex, params, rgb, frames, options, out) {
  const productIds = Array.isArray(ex.ProductId) ? ex.ProductId : ex.ProductId === undefined ? [] : [ex.ProductId];
  const productId = options.productId ?? (productIds.length ? Number(productIds[0]) : 0);
  out.vendorId = typeof ex.VendorId === "number" ? ex.VendorId : 0;
  out.productId = productId;
  const endpoints = options.endpoints ?? [{ interface: 0, usage: 0x0001, usage_page: 0xff00, collection: 0 }];
  const device = createSimDevice({ color: rgb, productId, vendorId: out.vendorId, endpoints });
  const restore = installGlobals({ device, ...NOT_IN_HOST, ...FEATURE_GLOBALS });
  try {
    const plugin = await importPlugin(entry);
    const run = phaseRunner(out, () => device.calls.length, (from) => hidTraffic(device, from));
    if (typeof plugin.Validate === "function") {
      const accepted = [];
      for (const ep of endpoints) {
        const ok = run(`Validate(${showEndpoint(ep)})`, () => plugin.Validate({ ...ep }));
        if (ok === undefined) return;
        out.phases.at(-1).result = Boolean(ok.value);
        if (ok.value) accepted.push(ep);
      }
      if (accepted.length === 0) {
        out.ok = false;
        out.log.push("Validate() accepted no endpoint: the host would not open the device");
        return;
      }
    }
    await session(plugin, params, frames, options, run);
  } finally {
    out.log.push(...device.logs.map((l) => l.message));
    out.battery = batteryReport(device);
    restore();
  }
}

// What the plugin reported through the `battery` feature, when it added it.
function batteryReport(device) {
  if (!device.features.includes("battery")) return undefined;
  const b = device.battery;
  return { level: b.levels.length ? b.batteryLevel : null, state: b.batteryState, levels: [...b.levels], states: [...b.states] };
}

async function simulateSmbus(entry, ex, params, rgb, frames, options, out) {
  const addresses = Array.isArray(ex.SmbusAddresses) ? ex.SmbusAddresses : [];
  const busOptions = { kind: "amd", name: "Simulated bus", ...(options.bus ?? {}) };
  if (busOptions.registers === undefined && busOptions.indexed === undefined) {
    // Every claimed address answers (unset registers read 0xFF).
    busOptions.registers = Object.fromEntries(addresses.map((a) => [a, {}]));
  }
  out.bus = busOptions.kind;
  // The fixed bus is bound when the mock is made: bind the first claimed address, rebuild if
  // Scan() picks another one (the register state Scan() wrote is replayed by the ops log).
  let mock = createMockBus({ ...busOptions, address: addresses[0] ?? 0 });
  // The host offers a RAM plugin Scan(bus) only when a DIMM's SPD names one of its
  // SpdManufacturers(); simulate one such DDR4 module in the first slot (or `bus.spdSlots`).
  const spd = Array.isArray(ex.SpdManufacturers) ? ex.SpdManufacturers : [];
  const slots = options.bus?.spdSlots ?? spd.slice(0, 1).map((m) => ({
    address: 0x50, memoryType: "DDR4", typeByte: 0x0c, jedecBank: m.bank, jedecId: m.id, partNumber: "SIMULATED",
  }));
  const free = { ...mock.free, SpdSlots: () => slots.map((s) => ({ ...s })) };
  const device = createSimDevice({ color: rgb });
  const restore = installGlobals({ device, bus: mock.fixed, ...NOT_IN_HOST, ...FEATURE_GLOBALS });
  try {
    const plugin = await importPlugin(entry);
    let opsFrom = 0;
    const traffic = () => {
      const lines = mock.ops.slice(opsFrom).map(showOp);
      opsFrom = mock.ops.length;
      return lines;
    };
    const run = phaseRunner(out, () => device.calls.length, (from) => [...traffic(), ...hidTraffic(device, from)]);
    if (typeof plugin.Scan !== "function") {
      out.ok = false;
      out.log.push("the plugin has no Scan(bus) export");
      return;
    }
    const scan = run("Scan(bus)", () => plugin.Scan(free));
    if (scan === undefined) return;
    const found = (Array.isArray(scan.value) ? scan.value : []).filter((a) => addresses.includes(a));
    out.phases.at(-1).result = scan.value;
    if (found.length === 0) {
      out.ok = false;
      out.log.push(`Scan() found no device at ${addresses.map((a) => `0x${a.toString(16)}`).join(", ") || "(no SmbusAddresses())"}`);
      return;
    }
    out.address = found[0];
    if (found[0] !== addresses[0]) {
      const before = mock;
      mock = createMockBus({ ...busOptions, address: found[0] });
      opsFrom = 0;
      for (const o of before.ops) mock.ops.push(o);
      opsFrom = mock.ops.length;
      globalThis.bus = mock.fixed;
    }
    await session(plugin, params, frames, options, run);
  } finally {
    out.log.push(...device.logs.map((l) => l.message), ...mock.logs);
    out.battery = batteryReport(device);
    restore();
  }
}

async function session(plugin, params, frames, options, run) {
  const restoreParams = installGlobals(params);
  try {
    if (run("Initialize()", () => plugin.Initialize?.()) === undefined) return;
    for (let i = 1; i <= frames; i++) {
      if (run(`Render() #${i}`, () => plugin.Render?.()) === undefined) return;
    }
    run(`Shutdown(${options.suspending === true})`, () => plugin.Shutdown?.(options.suspending === true));
  } finally {
    restoreParams();
  }
}

// Runs one lifecycle call and records its traffic; undefined when it threw.
function phaseRunner(out, mark, traffic) {
  return (name, fn) => {
    const from = mark();
    const phase = { phase: name, traffic: [] };
    out.phases.push(phase);
    try {
      const value = fn();
      phase.traffic = traffic(from);
      return { value };
    } catch (e) {
      phase.traffic = traffic(from);
      phase.error = `${e?.name ?? "Error"}: ${e?.message ?? e}`;
      out.ok = false;
      return undefined;
    }
  };
}

function hidTraffic(device, from) {
  const calls = device.calls.slice(from);
  const lines = [];
  let sampled = 0;
  for (const c of calls) {
    if (QUIET.has(c.method)) {
      sampled++;
      continue;
    }
    lines.push(showCall(c));
  }
  if (sampled > 0) lines.push(`(${sampled} canvas sample${sampled === 1 ? "" : "s"})`);
  return lines;
}

const isBytes = (v) => Array.isArray(v) && v.length > 0 && v.every((b) => typeof b === "number");

function showCall({ method, args }) {
  if ((method === "write" || method === "send_report") && Array.isArray(args[0])) {
    const length = Number(args[1]) || args[0].length;
    const bytes = Array.from({ length }, (_, i) => (i < args[0].length ? args[0][i] & 0xff : 0));
    return `${method.padEnd(12)} ${String(length).padStart(4)} B  ${bytesToHex(bytes)}`;
  }
  if (method === "get_report" || method === "read" || method === "input_report") {
    const id = Array.isArray(args[0]) ? args[0][0] : args[0];
    return `${method.padEnd(12)} ${String(args[1]).padStart(4)} B  report 0x${Number(id ?? 0).toString(16).padStart(2, "0").toUpperCase()}`;
  }
  const shown = args.map((a) => (isBytes(a) ? `[${bytesToHex(a)}]` : JSON.stringify(a))).join(", ");
  return `${method}(${shown})`;
}

const h2 = (n) => `0x${Number(n).toString(16).toUpperCase().padStart(2, "0")}`;

function showOp(o) {
  const dry = o.dry ? " (dry run)" : "";
  const reg = o.reg === undefined ? "" : ` reg ${h2(o.reg)}`;
  const data = o.data && o.data.length ? ` ${o.op.startsWith("Write") ? "<-" : "len"} ${o.data.map(h2).join(" ")}` : "";
  return `${o.op.padEnd(24)} @${h2(o.addr)}${reg}${data}${o.locked ? " [locked]" : ""}${dry}`;
}

function showEndpoint(ep) {
  const hx = (n) => `0x${Number(n ?? 0).toString(16).toUpperCase().padStart(4, "0")}`;
  return `interface ${ep.interface}, usage ${hx(ep.usage)}, usage_page ${hx(ep.usage_page)}, collection ${ep.collection ?? 0}`;
}

/** The run as readable text. */
export function formatRun(run) {
  const hex = (n) => Number(n).toString(16).toUpperCase().padStart(4, "0");
  const target = run.type.toLowerCase() === "smbus"
    ? `SMBus (${run.bus} bus${run.address !== undefined ? `, address ${h2(run.address)}` : ""})`
    : `${run.type} ${hex(run.vendorId ?? 0)}:${hex(run.productId ?? 0)}`;
  const color = `#${run.color.map((c) => c.toString(16).toUpperCase().padStart(2, "0")).join("")}`;
  const lines = [`${run.name ?? "(no Name())"}: ${target}, canvas ${color}`];
  for (const p of run.phases) {
    const result = p.result === undefined ? "" : ` -> ${JSON.stringify(p.result)}`;
    lines.push(`${p.phase}${result}`);
    for (const t of p.traffic) lines.push(`  ${t}`);
    if (p.traffic.length === 0 && !p.error) lines.push("  (nothing sent)");
    if (p.error) lines.push(`  THREW ${p.error}`);
  }
  if (run.battery) {
    const states = ["unknown", "draining", "charging", "full (charging)", "full", "wireless charging"];
    lines.push(`battery: ${run.battery.level === null ? "no level reported" : `${run.battery.level} %`}, ${states[run.battery.state] ?? run.battery.state}`);
  }
  if (run.log.length > 0) {
    lines.push("log:");
    for (const l of run.log) lines.push(`  ${l}`);
  }
  lines.push(run.ok ? "ok" : "FAILED");
  return lines.join("\n");
}
