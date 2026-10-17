// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Tiny mock of the SignalRGB-compatible `device` global for first-party plugin tests
// (Node 24, node:test). It records every I/O call so a test can assert the exact bytes a
// plugin would send. Conventions mirror the Lightloom compat host (docs/host-api/hid-usb.md):
//   * write/send_report take (bytes, length), zero-pad or truncate to `length`, and return the
//     number of bytes written (or -1 when the test asked for a failure); they never throw.
//   * get_report/read return the report-id-prefixed byte array that was registered for that
//     report id (or [] when none) and update getLastReadSize().
//   * color(x, y) returns a fresh [r, g, b] array from the configured colour or sampler.

import { pathToFileURL } from "node:url";

/**
 * @param {object} [options]
 * @param {Record<number|string, number[]>} [options.featureReports]  bytes returned by get_report, keyed by report id
 * @param {Record<number|string, number[]>} [options.inputReports]    bytes returned by read, keyed by report id
 * @param {number[] | ((x: number, y: number) => number[])} [options.color]  colour returned by color(x, y)
 * @param {number} [options.productId]
 * @param {number} [options.vendorId]
 * @param {Set<number> | number[]} [options.failSendReportIds]  report ids for which send_report returns -1
 */
export function createMockDevice(options = {}) {
  const featureReports = toReportMap(options.featureReports);
  const inputReports = toReportMap(options.inputReports);
  const failSend = new Set(options.failSendReportIds ?? []);
  let colorSource = options.color ?? [0, 0, 0];
  let lastReadSize = 0;

  const device = {
    // --- recordings ---------------------------------------------------------------------
    calls: [],          // every method call: { method, args }
    sent: [],           // send_report: { reportId, length, bytes }
    written: [],        // write:       { reportId, length, bytes }
    logs: [],           // log:         { message, options }
    pauses: [],         // pause(ms)
    channels: [],       // addChannel(name, ledLimit, defaultLedCount)

    // --- test controls ------------------------------------------------------------------
    setColor(color) { colorSource = color; },
    setFeatureReport(reportId, bytes) { featureReports.set(Number(reportId), Array.from(bytes)); },

    // --- canvas -------------------------------------------------------------------------
    color(x, y) {
      record("color", [x, y]);
      const c = typeof colorSource === "function" ? colorSource(x, y) : colorSource;
      return [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF];
    },
    getBrightness() { record("getBrightness", []); return 100; },

    // --- HID I/O ------------------------------------------------------------------------
    send_report(data, length) {
      record("send_report", [data, length]);
      const bytes = normalise(data, length);
      device.sent.push({ reportId: bytes[0], length, bytes });
      return failSend.has(bytes[0]) ? -1 : length;
    },
    get_report(reportIdBytes, length) {
      record("get_report", [reportIdBytes, length]);
      const id = Number(Array.isArray(reportIdBytes) ? reportIdBytes[0] : reportIdBytes);
      const stored = featureReports.get(id);
      lastReadSize = stored ? stored.length : 0;
      return stored ? stored.slice() : [];
    },
    write(data, length) {
      record("write", [data, length]);
      const bytes = normalise(data, length);
      device.written.push({ reportId: bytes[0], length, bytes });
      return length;
    },
    read(reportIdBytes, length, timeout = 100) {
      record("read", [reportIdBytes, length, timeout]);
      const id = Number(Array.isArray(reportIdBytes) ? reportIdBytes[0] : reportIdBytes);
      const stored = inputReports.get(id);
      lastReadSize = stored ? stored.length : 0;
      return stored ? stored.slice() : [];
    },
    getLastReadSize() { return lastReadSize; },
    clearReadBuffer() { record("clearReadBuffer", []); },
    flush() { record("flush", []); },
    pause(ms) { record("pause", [ms]); device.pauses.push(ms); },

    // --- identity / metadata ------------------------------------------------------------
    productId() { return options.productId ?? 0; },
    vendorId() { return options.vendorId ?? 0; },
    getDeviceInfo() { return { product: "mock" }; },
    setName(name) { record("setName", [name]); },
    setSize(size) { record("setSize", [size]); },
    setControllableLeds(names, positions) { record("setControllableLeds", [names, positions]); },
    SetLedLimit(limit) { record("SetLedLimit", [limit]); },
    addChannel(name, ledLimit, defaultLedCount) {
      record("addChannel", [name, ledLimit, defaultLedCount]);
      device.channels.push({ name, ledLimit, defaultLedCount });
    },

    // --- diagnostics --------------------------------------------------------------------
    log(message, opts) { device.logs.push({ message: String(message), options: opts ?? {} }); },
    notify(title, text, priority) { record("notify", [title, text, priority]); },
  };

  function record(method, args) { device.calls.push({ method, args }); }
  return device;
}

/** Install `device` and ControllableParameters globals; returns a function that restores them. */
export function installGlobals(device, params = {}) {
  const previous = new Map();
  const set = (name, value) => {
    previous.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    globalThis[name] = value;
  };
  set("device", device);
  for (const [name, value] of Object.entries(params)) {
    set(name, value);
  }
  return () => {
    for (const [name, descriptor] of previous) {
      if (descriptor) {
        Object.defineProperty(globalThis, name, descriptor);
      } else {
        delete globalThis[name];
      }
    }
  };
}

/**
 * Import a plugin as a fresh ES module instance (a cache-busting query string gives each test
 * its own module-level state). Globals must be installed before calling this.
 */
export async function loadPlugin(pluginPath) {
  const url = pathToFileURL(pluginPath);
  url.searchParams.set("instance", `${Date.now()}-${Math.random().toString(16).slice(2)}`);
  return import(url.href);
}

/** Parse a hex string ("52 01 02" or "520102") into a byte array. */
export function hexToBytes(hex) {
  const clean = String(hex).replace(/[^0-9a-fA-F]/g, "");
  const bytes = [];
  for (let i = 0; i < clean.length; i += 2) {
    bytes.push(parseInt(clean.slice(i, i + 2), 16));
  }
  return bytes;
}

/** Format a byte array as a spaced upper-case hex string (handy in assertion messages). */
export function bytesToHex(bytes) {
  return Array.from(bytes, (b) => (b & 0xFF).toString(16).toUpperCase().padStart(2, "0")).join(" ");
}

function toReportMap(reports) {
  const map = new Map();
  for (const [id, bytes] of Object.entries(reports ?? {})) {
    map.set(Number(id), Array.from(bytes));
  }
  return map;
}

function normalise(data, length) {
  const out = new Array(length).fill(0);
  const n = Math.min(length, data.length);
  for (let i = 0; i < n; i++) {
    out[i] = data[i] & 0xFF;
  }
  return out;
}
