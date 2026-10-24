// SPDX-License-Identifier: GPL-3.0-or-later
//
// Loading a plugin module under Node the way the HueDeck host does, for `sdk/lint` and
// `sdk/simulator`: host globals installed first (`device`, `bus`, parameter globals), then
// the module imported as a fresh instance, then its metadata exports read with the
// call-if-function rule (crates/huedeck-plugin-host js/host.js `readMeta`).
//
// The `device` global is plugins/_testkit's mock device (it records the HID traffic) with
// the rest of the documented `device.*` surface filled in as recording no-ops; a member the
// host does not know throws `HostApiMissing`, as in a real device session.

import { createMockDevice, loadPlugin } from "../../plugins/_testkit/mock-device.mjs";
import { registerHostModules } from "./host-modules.mjs";
import { createFeatureGlobal as createBase64Feature } from "../../prelude/base64.js";

/** Metadata exports the host reads (host.js `METADATA`). */
export const METADATA = Object.freeze([
  "Name",
  "Publisher",
  "VendorId",
  "ProductId",
  "Size",
  "DefaultPosition",
  "DefaultScale",
  "LedNames",
  "LedPositions",
  "ImageUrl",
  "Image",
  "Documentation",
  "Version",
  "Type",
  "DeviceType",
  "ControllableParameters",
  "ConflictingProcesses",
  "SubdeviceController",
  "SupportsSubdevices",
  "LacksOnBoardLeds",
  "DefaultComponentBrand",
  "SupportsFanControl",
  "DeviceMessages",
  "UsesCorsairMutex",
  "UsesUSBSensorMutex",
  "AllowConflictBypass",
  "BrandGPUList",
  "HidUsages",
  "SmbusAddresses",
  "SmbusBuses",
  "SmbusDeniedRegisters",
  "SpdManufacturers",
  "SmbusBatchWrites",
  "SaveCapabilities",
  "ShutdownModes",
]);

/** Lifecycle exports (host.js `LIFECYCLE`). */
export const LIFECYCLE = Object.freeze(["Validate", "Initialize", "Render", "Shutdown", "Scan", "DiscoveryService", "SaveToDevice"]);

/**
 * Read the metadata exports of a plugin namespace: each one called when it is a function,
 * a `ProductId` object turned into its keys. Throws are collected, not raised.
 * @returns {{ exports: Record<string, unknown>, errors: Record<string, string>, lifecycle: Record<string, boolean>, hooks: string[], names: string[] }}
 */
export function readExports(ns) {
  const exports = {};
  const errors = {};
  for (const name of METADATA) {
    if (!(name in ns)) continue;
    try {
      let v = ns[name];
      if (typeof v === "function") v = v();
      if (name === "ProductId" && v && typeof v === "object" && !Array.isArray(v)) v = Object.keys(v);
      exports[name] = v;
    } catch (e) {
      errors[name] = `${e?.name ?? "Error"}: ${e?.message ?? e}`;
    }
  }
  const lifecycle = Object.fromEntries(LIFECYCLE.map((n) => [n, typeof ns[n] === "function"]));
  const names = Object.keys(ns);
  const hooks = names.filter((k) => /^on.+Changed$/.test(k) && typeof ns[k] === "function");
  return { exports, errors, lifecycle, hooks, names };
}

/** A typed error like the host's `HostApiMissing` (prelude/HostApiMissing.js). */
export class HostApiMissing extends Error {
  constructor(member) {
    super(`${member} is not implemented by this host`);
    this.name = "HostApiMissing";
    this.member = member;
  }
}

const noop = () => undefined;

// Feature globals of `device.addFeature(name)` (host.js `featureFactories`, stub answers).
const FEATURES = {
  base64: () => ({ ...createBase64Feature() }),
  keyboard: () => ({ sendHid: noop, sendKey: noop, sendEvent: noop }),
  mouse: () => ({ sendHid: noop, sendEvent: noop }),
  // The _testkit mock's recording battery (levels / states it was given).
  battery: (mock) => mock.battery,
};

/** Feature globals `device.addFeature()` may install: spread into installGlobals() so they are removed after a run. */
export const FEATURE_GLOBALS = Object.freeze(Object.fromEntries(Object.keys(FEATURES).map((name) => [name, undefined])));

/**
 * The `device` global of a simulated session: the _testkit mock (HID traffic, canvas colour,
 * channels, subdevices) plus the remaining documented members as recording no-ops.
 * @param {Parameters<typeof createMockDevice>[0] & { endpoints?: object[] }} [options]
 */
export function createSimDevice(options = {}) {
  const mock = createMockDevice(options);
  const properties = new Map();
  const record = (method) => (...args) => {
    mock.calls.push({ method, args });
    return undefined;
  };
  const extras = {
    getHidEndpoints: () => (options.endpoints ?? []).map((e) => ({ ...e })),
    getHidInfo: () => ({ readLength: 64, writeLength: 64 }),
    input_report: (id, length) => mock.get_report(id, length),
    set_endpoint: record("set_endpoint"),
    setFrameRateTarget: record("setFrameRateTarget"),
    setRefreshRate: record("setRefreshRate"),
    setFrameDelay: record("setFrameDelay"),
    resetFrameRateTarget: record("resetFrameRateTarget"),
    setImageFromUrl: record("setImageFromUrl"),
    setImageFromBase64: record("setImageFromBase64"),
    repollName: noop,
    repollLeds: noop,
    repollSize: noop,
    removeChannel: record("removeChannel"),
    getChannelNames: () => mock.channels.map((c) => c.name),
    getLedCount: () => 0,
    getChannelPulseColor: () => "#009EDB",
    removeSubdevice: record("removeSubdevice"),
    getCurrentSubdevices: () => mock.subdevices.map((s) => s.id),
    setSubdeviceLedMap: record("setSubdeviceLedMap"),
    setSubdeviceImageUrl: record("setSubdeviceImageUrl"),
    setSubdeviceImage: record("setSubdeviceImage"),
    SetIsSubdeviceController: record("SetIsSubdeviceController"),
    denotify: noop,
    addMessage: record("addMessage"),
    removeMessage: record("removeMessage"),
    fanControlDisabled: () => true,
    addProperty: (obj) => {
      if (obj && typeof obj.property === "string") {
        properties.set(obj.property, obj);
        if (!(obj.property in globalThis)) globalThis[obj.property] = obj.default;
      }
    },
    removeProperty: (name) => properties.delete(String(name)),
    getProperty: (name) => properties.get(String(name)),
    hasProperty: (name) => properties.has(String(name)),
    getPropertyNames: () => [...properties.keys()],
    setUserProperty: (name, value) => {
      globalThis[String(name)] = value;
    },
  };
  for (const [k, v] of Object.entries(extras)) {
    if (!(k in mock)) mock[k] = v;
  }
  // Features install their global, as in the host (the testkit mock only records the call).
  const recordFeature = mock.addFeature;
  mock.addFeature = (name) => {
    if (typeof recordFeature === "function") recordFeature(name);
    else mock.calls.push({ method: "addFeature", args: [name] });
    const make = FEATURES[String(name)];
    if (make === undefined) throw new HostApiMissing(`device.addFeature(${name})`);
    const value = make(mock);
    if (value !== undefined && globalThis[name] !== value) globalThis[name] = value;
  };
  // Raw USB (rawusb / hybrid plugins): recorded; IN transfers answer zeros,
  // OUT transfers the length, as the stub host does.
  const usbDefaults = {
    bulk_transfer: (endpoint, data, length, timeout) => {
      mock.calls.push({ method: "bulk_transfer", args: [endpoint, data, length, timeout] });
      const n = Number(length) || (data && data.length) || 0;
      return Number(endpoint) & 0x80 ? new Array(n).fill(0) : n;
    },
    control_transfer: (requestType, request, value, index, data, length, timeout) => {
      mock.calls.push({ method: "control_transfer", args: [requestType, request, value, index, data, length, timeout] });
      const n = Number(length) || (data && data.length) || 0;
      return Number(requestType) & 0x80 ? new Array(n).fill(0) : n;
    },
    setProtocol: (name) => {
      mock.calls.push({ method: "setProtocol", args: [name] });
    },
  };
  for (const [k, v] of Object.entries(usbDefaults)) {
    if (!(k in mock)) mock[k] = v;
  }
  // Not implemented in HueDeck device sessions yet (host.js `unimplemented`).
  for (const k of [
    "createFanControl", "removeFanControl", "setRPM",
    "getFanlevel", "getFanLevel", "getNormalizedFanlevel", "getNormalizedFanLevel", "getImageBuffer",
    "getMotherboardName", "createTemperatureSensor", "removeTemperatureSensor", "SetTemperature",
  ]) {
    mock[k] = () => {
      throw new HostApiMissing(`device.${k}`);
    };
  }
  const recordingKeys = new Set(["calls", "sent", "written", "logs", "pauses", "channels", "subdevices", "features", "battery", "setColor", "setFeatureReport", "setChannelColors"]);
  return new Proxy(mock, {
    get(target, key, receiver) {
      if (typeof key !== "string" || key in target || recordingKeys.has(key) || key === "then" || key === "toJSON") {
        return Reflect.get(target, key, receiver);
      }
      return () => {
        throw new HostApiMissing(`device.${key}`);
      };
    },
  });
}

/**
 * Install `globals` (name -> value) on globalThis; returns a function restoring what was there.
 * @param {Record<string, unknown>} globals
 */
export function installGlobals(globals) {
  const previous = new Map();
  for (const [name, value] of Object.entries(globals)) {
    previous.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    globalThis[name] = value;
  }
  return () => {
    for (const [name, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  };
}

/**
 * Import the plugin entry file as a fresh module instance with `@SignalRGB/*` / `@huedeck/*`
 * resolved. Install the globals first.
 */
export async function importPlugin(entryPath) {
  registerHostModules();
  return loadPlugin(entryPath);
}

/**
 * Bind ControllableParameters defaults as globals, coercing quoted numbers and booleans
 * (host.js `coerceDefault`).
 * @returns {Record<string, unknown>}
 */
export function parameterDefaults(params) {
  const out = {};
  for (const p of Array.isArray(params) ? params : []) {
    if (!p || typeof p.property !== "string" || p.property === "") continue;
    let v = p.default;
    const type = String(p.type ?? "").toLowerCase();
    if ((type === "number" || type === "hue") && typeof v === "string" && v.trim() !== "" && !Number.isNaN(Number(v))) {
      v = Number(v);
    } else if (type === "boolean" && typeof v === "string") {
      v = v === "true";
    }
    out[p.property] = v;
  }
  return out;
}
