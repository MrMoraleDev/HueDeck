// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom compat host.
//
// Evaluated as an ES module in every plugin context *before* the plugin
// module. It installs the compat-v1 host globals (`device`, `bus`, `console`,
// `service`, `controller`, `discovery`, `XmlHttp`, `XMLHttpRequest`,
// `BinaryUtils`) plus `device.addFeature()` feature globals. Member lists
// follow docs/host-api/*.md. Every call of a listed member is counted
// (`<object>.<member>`); every access to a member that is not listed is
// recorded as `API_GAP:<object>.<member>`. `controller` and `discovery` hold
// plugin-defined data (network-services.md §1.1), so reads of fields the host
// lacks are recorded as stub fields, not API gaps, and yield `undefined`.
//
// Two modes, chosen by the Rust side through `__lightloomConfig.mode`:
//
// * "stub" (default; `lightloom compat dry-run`): every member is a recording
//   stub that returns sane defaults and never touches hardware; an API gap is
//   answered with a no-op function, so the dry-run measures how far the
//   corpus gets against the documented surface.
// * "device" (a real device session): the HID, canvas, subdevice, channel and
//   logging members of `device` call the natives on `__lightloomNative`
//   (crates/lightloom-plugin-host src/device.rs and src/children.rs), which this
//   module captures and removes from the global object before plugin code
//   runs. An API gap is logged once and throws `HostApiMissing` when called
//   (architecture-synthesis.md D6).
//
// The Rust side reads state back through `globalThis.__lightloomHost`
// (non-writable, non-configurable: plugin code cannot replace it).

import { createColorArray, layoutColors, layoutFlatColors } from "lightloom:prelude/createColorArray.js";
import { BinaryUtils } from "lightloom:prelude/BinaryUtils.js";
import { createFeatureGlobal as createBase64Feature } from "lightloom:prelude/base64.js";
import { HostApiMissing } from "lightloom:prelude/HostApiMissing.js";

// Builtins the host-side helpers rely on, captured before plugin code runs
// (a plugin replacing `JSON.stringify` must not falsify the report).
const stringify = JSON.stringify;
const parseJson = JSON.parse;
const objectKeys = Object.keys;
const hasOwn = Object.hasOwn;
const fromEntries = Object.fromEntries;
const isArray = Array.isArray;
const arrayFrom = Array.from;
const isView = ArrayBuffer.isView;
const isFiniteNumber = Number.isFinite;
const defineProperty = Object.defineProperty;
const freeze = Object.freeze;

const cfg = globalThis.__lightloomConfig ?? {};

// Device mode: capture the natives and take them out of plugin reach.
const native = cfg.mode === "device" ? globalThis.__lightloomNative : undefined;
delete globalThis.__lightloomNative;
const strictGaps = native !== undefined;

const state = {
  gaps: new Map(),
  fields: new Map(),
  calls: new Map(),
  consoleLines: 0,
  logTail: [],
  productId: 0,
  vendorId: 0,
  properties: new Map(),
  channels: new Map(),
  subdevices: [],
  notifyId: 0,
  namespace: undefined,
  gapsLogged: new Set(),
};

// Keys that probing code (Promise resolution, JSON, printing, typeof checks
// done by engines) touches without meaning to use a host member.
const PASSTHROUGH = new Set([
  "then",
  "catch",
  "finally",
  "toJSON",
  "constructor",
  "prototype",
  "valueOf",
  "toString",
  "toLocaleString",
  "inspect",
  "nodeType",
  "$$typeof",
  "__esModule",
  "length",
  "name",
]);

function bump(map, key) {
  map.set(key, (map.get(key) ?? 0) + 1);
}

function gap(key) {
  bump(state.gaps, key);
}

function fmt(args) {
  return args
    .map((a) => {
      if (typeof a === "string") {
        return a;
      }
      try {
        return stringify(a);
      } catch {
        return String(a);
      }
    })
    .join(" ");
}

function line(text) {
  state.consoleLines += 1;
  if (state.logTail.length < 20) {
    state.logTail.push(String(text).slice(0, 300));
  }
}

/** A plugin-visible log line: counted, and forwarded to the session log in device mode. */
function hostLog(text) {
  line(text);
  if (native !== undefined) {
    native.log(String(text));
  }
}

/**
 * Device mode: a documented member Lightloom does not implement yet. Recorded as
 * an API gap, logged once, and throws HostApiMissing when called, so a plugin
 * never acts on stub data in a real session.
 */
function unimplementedMember(member) {
  return function lightloomUnimplemented() {
    gap(member);
    if (!state.gapsLogged.has(member)) {
      state.gapsLogged.add(member);
      hostLog(`API_GAP: ${member} is not implemented by this host`);
    }
    throw new HostApiMissing(member);
  };
}

function unimplemented(objectName, names) {
  return fromEntries(names.map((n) => [n, unimplementedMember(`${objectName}.${n}`)]));
}

/** Device mode: an unimplemented member logs once, then throws a typed error when called. */
function gapThrower(member) {
  return function lightloomApiGap() {
    if (!state.gapsLogged.has(member)) {
      state.gapsLogged.add(member);
      hostLog(`API_GAP: ${member} is not implemented by this host`);
    }
    throw new HostApiMissing(member);
  };
}

/**
 * Wrap `target` as a recording host object: calls of its function members are
 * counted as `<objectName>.<member>`; unknown string members are recorded as
 * API gaps and answered with a no-op function (`data: true`: recorded as stub
 * fields and answered with `undefined`, for plugin-defined plain data).
 */
function recording(objectName, target, { data = false } = {}) {
  const wrappers = new Map();
  return new Proxy(target, {
    get(t, key, receiver) {
      if (typeof key !== "string" || PASSTHROUGH.has(key)) {
        return Reflect.get(t, key, receiver);
      }
      if (key in t) {
        const value = Reflect.get(t, key, receiver);
        // Inherited builtins (`hasOwnProperty`, ...) are not host members.
        if (typeof value !== "function" || !hasOwn(t, key)) {
          return value;
        }
        let w = wrappers.get(key);
        if (w === undefined || w.fn !== value) {
          const name = `${objectName}.${key}`;
          const wrapper = function lightloomRecorded(...args) {
            bump(state.calls, name);
            return Reflect.apply(value, this, args);
          };
          w = { fn: value, wrapper };
          wrappers.set(key, w);
        }
        return w.wrapper;
      }
      if (data) {
        bump(state.fields, `${objectName}.${key}`);
        return undefined;
      }
      gap(`${objectName}.${key}`);
      if (strictGaps) {
        return gapThrower(`${objectName}.${key}`);
      }
      return function lightloomApiGap() {
        return undefined;
      };
    },
  });
}

const zeros = (n) => {
  const len = Number.isFinite(Number(n)) && Number(n) > 0 ? Math.min(Number(n) | 0, 1 << 20) : 0;
  return new Array(len).fill(0);
};
const noop = () => undefined;
const logFn = (...args) => hostLog(fmt(args));

// ---------------------------------------------------------------- console
const consoleStub = recording("console", {
  log: logFn,
  warn: logFn,
  error: logFn,
  info: logFn,
  debug: logFn,
  trace: logFn,
  assert: (cond, ...args) => {
    if (!cond) {
      line(fmt(args));
    }
  },
  count: noop,
  time: noop,
  timeEnd: noop,
});

// ---------------------------------------------------------------- channels
// components-channels.md §1.6. No components are assigned, so every colour
// query is empty in the requested layout: "Inline" -> [], "Separate" /
// "Seperate" -> [[], [], []].
function makeChannel(name, ledLimit = 0) {
  const ch = {
    name,
    ledCount: 0,
    ledLimit,
    overrideColors: false,
    overrideCount: 0,
    forceColor: false,
    identifying: false,
    LedCount: () => 0,
    LedLimit: () => ch.ledLimit,
    SetLedLimit: (n) => {
      ch.ledLimit = Number(n) || 0;
    },
    SetLedCount: noop,
    Name: () => ch.name,
    SetName: (n) => {
      ch.name = String(n);
    },
    getColors: (layout, order) => layoutColors([], layout, order),
    shouldPulseColors: () => false,
    onlyHasDefaultComponents: () => true,
    getComponentNames: () => [],
    getComponents: () => [],
    getComponent: () => undefined,
    getComponentColors: (_id, layout, order) => layoutColors([], layout, order),
    getComponentData: () => ({ LedCount: 0 }),
    getChannelColor: () => "#000000",
    getPulse: (count, layout, order) => createColorArray("#009EDB", count, layout, order),
  };
  return recording("channel", ch);
}

// ---------------------------------------------------------------- features
function socketStub() {
  return recording("socket", {
    UnconnectedState: 0,
    HostLookupState: 1,
    ConnectingState: 2,
    ConnectedState: 3,
    BoundState: 4,
    ClosingState: 6,
    ListeningState: 5,
    state: 0,
    on: noop,
    bind: noop,
    close: noop,
    connect: noop,
    disconnect: noop,
    send: (data) => (data && data.length) || 0,
    write: (data) => (data && data.length) || 0,
    address: () => ({ address: "0.0.0.0", port: 0, protocol: "IPV4" }),
    remoteAddress: () => ({ address: "0.0.0.0", port: 0, protocol: "IPV4" }),
  });
}

const featureFactories = {
  base64: () => recording("base64", { ...createBase64Feature() }),
  udp: () =>
    recording("udp", {
      send: (_ip, _port, data) => (data && data.length) || 0,
      createSocket: socketStub,
    }),
  dtls: () =>
    recording("dtls", {
      createConnection: noop,
      send: (data) => (data && data.length) || 0,
      hasEncryptedConnection: () => false,
      CloseConnection: noop,
      onConnectionEstablished: noop,
      onConnectionClosed: noop,
      onConnectionError: noop,
      ignoreSslErrors: noop,
    }),
  keyboard: () => recording("keyboard", { sendHid: noop, sendKey: noop, sendEvent: noop }),
  mouse: () => recording("mouse", { sendHid: noop, sendEvent: noop }),
  battery: () =>
    recording("battery", {
      disabled: 0,
      unknown: 0,
      draining: 1,
      charging: 2,
      fullCharging: 3,
      full: 4,
      wirelessCharging: 5,
      batteryLevel: 0,
      batteryState: 0,
      stateString: "Unknown",
      lowBatteryWarningLevel: 15,
      setBatteryLevel: noop,
      setBatteryState: noop,
    }),
};

// ---------------------------------------------------------------- device
const deviceTarget = {
  // I/O (hid-usb.md §2). Never touches hardware.
  write: (data, length) => Number(length) || (data && data.length) || 0,
  send_report: (data, length) => Number(length) || (data && data.length) || 0,
  read: (_id, length) => zeros(length),
  get_report: (_id, length) => zeros(length),
  input_report: (_id, length) => zeros(length),
  getLastReadSize: () => 0,
  clearReadBuffer: noop,
  flush: noop,
  set_endpoint: noop,
  getHidEndpoints: () => [],
  getHidInfo: () => ({ readLength: 64, writeLength: 64 }),
  getDeviceInfo: () => ({
    product: "",
    manufacturer: "",
    serialNumber: "",
    vendorId: state.vendorId,
    productId: state.productId,
  }),
  productId: () => state.productId,
  bulk_transfer: (endpoint, data, length) =>
    Number(endpoint) >= 0x80 ? zeros(length) : Number(length) || (data && data.length) || 0,
  control_transfer: (_rt, _rq, _v, _i, _data, length) => zeros(length),
  setProtocol: noop,
  pause: noop,
  setFrameRateTarget: noop,
  setRefreshRate: noop,
  setFrameDelay: noop,
  resetFrameRateTarget: noop,
  // Canvas, shape, subdevices (components-channels.md §1-2).
  color: () => [0, 0, 0],
  getBrightness: () => 100,
  subdeviceColor: () => [0, 0, 0],
  createSubdevice: (id) => {
    if (!state.subdevices.includes(id)) {
      state.subdevices.push(id);
    }
  },
  removeSubdevice: (id) => {
    const i = state.subdevices.indexOf(id);
    if (i >= 0) {
      state.subdevices.splice(i, 1);
    }
  },
  getCurrentSubdevices: () => [...state.subdevices],
  setSubdeviceName: noop,
  setSubdeviceSize: noop,
  setSubdeviceLeds: noop,
  setSubdeviceLedMap: noop,
  setSubdeviceImageUrl: noop,
  setSubdeviceImage: noop,
  SetIsSubdeviceController: noop,
  setName: noop,
  setSize: noop,
  setControllableLeds: noop,
  setImageFromUrl: noop,
  setImageFromBase64: noop,
  repollName: noop,
  repollLeds: noop,
  repollSize: noop,
  SetLedLimit: noop,
  addChannel: (name, ledLimit) => {
    state.channels.set(String(name), makeChannel(String(name), Number(ledLimit) || 0));
  },
  removeChannel: (name) => {
    state.channels.delete(String(name));
  },
  getChannelNames: () => [...state.channels.keys()],
  channel: (name) => {
    const key = String(name);
    if (!state.channels.has(key)) {
      state.channels.set(key, makeChannel(key));
    }
    return state.channels.get(key);
  },
  getLedCount: () => 0,
  getChannelPulseColor: () => "#009EDB",
  createColorArray: (color, count, layout, order) => createColorArray(color, count, layout, order),
  // Properties (components-channels.md §2): each value is a global named `property`.
  addProperty: (obj) => {
    if (obj && typeof obj === "object" && typeof obj.property === "string") {
      state.properties.set(obj.property, obj);
      if (!(obj.property in globalThis)) {
        globalThis[obj.property] = coerceDefault(obj);
      }
    }
  },
  removeProperty: (name) => {
    state.properties.delete(String(name));
  },
  getProperty: (name) => state.properties.get(String(name)),
  hasProperty: (name) => state.properties.has(String(name)),
  getPropertyNames: () => [...state.properties.keys()],
  setUserProperty: (name, value) => {
    const key = String(name);
    if (state.properties.has(key) || key in globalThis) {
      globalThis[key] = value;
    }
  },
  // Features, notifications, messages, logging.
  addFeature: (name) => {
    const key = String(name);
    const make = featureFactories[key];
    if (make === undefined) {
      gap(`device.addFeature(${key})`);
      return;
    }
    if (globalThis[key] === undefined || globalThis[key].__lightloomFeature !== key) {
      const value = make();
      try {
        defineProperty(value, "__lightloomFeature", { value: key });
      } catch {
        // Proxies over frozen objects: the marker is optional.
      }
      globalThis[key] = value;
    }
  },
  notify: () => {
    state.notifyId += 1;
    return `lightloom-alert-${state.notifyId}`;
  },
  denotify: noop,
  addMessage: noop,
  removeMessage: noop,
  log: logFn,
  // Fan + temperature API (components-channels.md §5), exact casing plus docs aliases.
  fanControlDisabled: () => false,
  createFanControl: noop,
  removeFanControl: noop,
  setRPM: noop,
  getFanlevel: () => 0,
  getFanLevel: () => 0,
  getNormalizedFanlevel: () => 0,
  getNormalizedFanLevel: () => 0,
  createTemperatureSensor: noop,
  removeTemperatureSensor: noop,
  SetTemperature: noop,
  // Misc documented members.
  getImageBuffer: () => [],
  getMotherboardName: () => "",
};

// ---------------------------------------------------------------- device (device mode)
// Argument normalisation follows hid-usb.md §2: a byte array is any array-like
// of numbers, each taken `& 0xFF`; an explicit length pads with zeros or
// truncates; reads return `length` bytes, report id first.
const MAX_REPORT = 1 << 16;
const toInt = (v) => Math.trunc(Number(v)) | 0;

function lengthArg(data, length) {
  const n = Number(length);
  if (Number.isFinite(n) && n > 0) {
    return Math.min(Math.trunc(n), MAX_REPORT);
  }
  const l = data != null && typeof data.length === "number" ? data.length : 0;
  return Math.min(l >>> 0, MAX_REPORT);
}

function bytesArg(data, length) {
  const n = lengthArg(data, length);
  const src = data != null && typeof data.length === "number" ? data : [];
  const out = new Array(n);
  for (let i = 0; i < n; i++) {
    out[i] = i < src.length ? Number(src[i]) & 0xff : 0;
  }
  return out;
}

function reportIdArg(data) {
  if (typeof data === "number") {
    return data & 0xff;
  }
  if (data != null && typeof data.length === "number" && data.length > 0) {
    return Number(data[0]) & 0xff;
  }
  return 0;
}

// Reads may block at most 5 s, so a stop request (Ctrl-C, window close) still
// leaves the session time to run Shutdown() before the host gives up on it.
const MAX_READ_TIMEOUT_MS = 5000;

function timeoutArg(timeout, fallback) {
  const t = Number(timeout);
  return Number.isFinite(t) && t >= 0 ? Math.min(Math.trunc(t), MAX_READ_TIMEOUT_MS) : fallback;
}

/** `[[x, y], ...]` -> `[x0, y0, x1, y1, ...]` (missing coordinates are 0). */
function flatPositions(positions) {
  const flat = [];
  if (isArray(positions)) {
    for (const p of positions) {
      flat.push(toInt(p?.[0]), toInt(p?.[1]));
    }
  }
  return flat;
}

const strArg = (v) => (v === undefined || v === null ? "" : String(v));

// Identify colour of channels (SignalRGB's default accent) until the UI can pulse them.
const PULSE_COLOR = "#009EDB";

/**
 * Device mode: the channel object of `device.channel(name)`. Stateless (every
 * member asks the host by name), so one object per name is reused.
 */
function deviceChannel(name) {
  const data = (id) => {
    const text = native.chComponentData(name, id);
    return text === "" ? undefined : parseJson(text);
  };
  const component = (id) => {
    const componentData = data(id);
    return componentData === undefined ? undefined : { componentID: id, componentData };
  };
  return recording("channel", {
    get ledCount() {
      return native.chLedCount(name);
    },
    get ledLimit() {
      return native.chLedLimit(name);
    },
    get name() {
      return name;
    },
    overrideColors: false,
    overrideCount: 0,
    forceColor: false,
    identifying: false,
    LedCount: () => native.chLedCount(name),
    LedLimit: () => native.chLedLimit(name),
    SetLedLimit: (n) => native.chSetLimit(name, toInt(n)),
    SetLedCount: noop,
    Name: () => name,
    SetName: noop,
    getColors: (layout, order) => layoutFlatColors(native.chColors(name), layout, order),
    // Nobody can ask to identify a channel yet (no UI): never pulsing.
    shouldPulseColors: () => false,
    onlyHasDefaultComponents: () => native.chOnlyDefaults(name),
    getComponentNames: () => native.chComponents(name),
    getComponents: () => native.chComponents(name).map(component),
    getComponent: (id) => component(strArg(id)),
    getComponentColors: (id, layout, order) =>
      layoutFlatColors(native.chComponentColors(name, strArg(id)), layout, order),
    getComponentData: (id) => data(strArg(id)),
    getChannelColor: () => "#000000",
    getPulse: (count, layout, order) => createColorArray(PULSE_COLOR, count, layout, order),
  });
}

/** `device.log(value[, { toFile, hex }])`; `hex` prints a byte array as hex. */
function deviceLog(value, ...rest) {
  const opts = rest.length === 1 && rest[0] && typeof rest[0] === "object" && !isArray(rest[0]) ? rest[0] : undefined;
  if (opts && opts.hex && value != null && typeof value !== "string" && typeof value.length === "number") {
    hostLog(arrayFrom(value, (b) => (Number(b) & 0xff).toString(16).padStart(2, "0")).join(" "));
    return;
  }
  hostLog(fmt(opts === undefined ? [value, ...rest] : [value]));
}

const channelObjects = new Map();

if (native !== undefined) {
  const stubAddFeature = deviceTarget.addFeature;
  Object.assign(deviceTarget, {
    // Not implemented in device sessions yet: these throw instead of answering
    // with stub data (libusb transfers: M2, fans: M5, LCD: M6).
    ...unimplemented("device", [
      "bulk_transfer",
      "control_transfer",
      "setProtocol",
      "createFanControl",
      "removeFanControl",
      "setRPM",
      "getFanlevel",
      "getFanLevel",
      "getNormalizedFanlevel",
      "getNormalizedFanLevel",
      "getImageBuffer",
      "getMotherboardName",
    ]),
    // Subdevices (components-channels.md §2).
    createSubdevice: (id) => native.subCreate(strArg(id)),
    removeSubdevice: (id) => native.subRemove(strArg(id)),
    getCurrentSubdevices: () => native.subList(),
    setSubdeviceName: (id, name) => native.subName(strArg(id), strArg(name)),
    setSubdeviceSize: (id, w, h) => native.subSize(strArg(id), toInt(w), toInt(h)),
    setSubdeviceLeds: (id, names, positions) =>
      native.subLeds(strArg(id), isArray(names) ? names.map(strArg) : [], flatPositions(positions)),
    setSubdeviceLedMap: (id, map) => native.subLedMap(strArg(id), isArray(map) ? map.map(toInt) : []),
    setSubdeviceImageUrl: (id, url) => native.subImage(strArg(id), strArg(url)),
    setSubdeviceImage: (id, image) => native.subImage(strArg(id), strArg(image)),
    subdeviceColor: (id, x, y) => native.subColor(strArg(id), toInt(x), toInt(y)),
    SetIsSubdeviceController: (flag) => native.setSubdeviceController(Boolean(flag)),
    // Channels (components-channels.md §1): components are assigned by the host.
    SetLedLimit: (n) => native.setLedLimit(toInt(n)),
    addChannel: (name, ledLimit, defaultCount) =>
      native.chAdd(strArg(name), toInt(ledLimit), defaultCount === undefined ? -1 : toInt(defaultCount)),
    removeChannel: (name) => native.chRemove(strArg(name)),
    getChannelNames: () => native.chNames(),
    channel: (name) => {
      const key = strArg(name);
      if (!native.chExists(key)) {
        return undefined;
      }
      let ch = channelObjects.get(key);
      if (ch === undefined) {
        ch = deviceChannel(key);
        channelObjects.set(key, ch);
      }
      return ch;
    },
    getLedCount: () => native.ledCount(),
    getChannelPulseColor: () => PULSE_COLOR,
    // Lightloom does not drive fans yet; plugins that check this skip fan control.
    fanControlDisabled: () => true,
    // Network features need sockets Lightloom does not provide to device plugins yet.
    addFeature: (name) => {
      const key = String(name);
      if (key === "udp" || key === "dtls") {
        unimplementedMember(`device.addFeature(${key})`)();
      }
      return stubAddFeature(name);
    },
    write: (data, length) => native.hidWrite(bytesArg(data, length)),
    send_report: (data, length) => native.hidSendFeature(bytesArg(data, length)),
    read: (data, length, timeout) => native.hidRead(lengthArg(data, length), timeoutArg(timeout, 100)),
    get_report: (data, length) => native.hidGetFeature(reportIdArg(data), lengthArg(data, length)),
    input_report: (data, length) => native.hidGetInput(reportIdArg(data), lengthArg(data, length)),
    getLastReadSize: () => native.lastReadSize(),
    clearReadBuffer: () => native.clearReadBuffer(),
    flush: () => native.clearReadBuffer(),
    set_endpoint: (iface, usage, usagePage, collection) =>
      native.setEndpoint(toInt(iface), toInt(usage), toInt(usagePage), collection === undefined ? -1 : toInt(collection)),
    getHidEndpoints: () =>
      native.endpoints().map(([iface, usage, usagePage, collection]) => ({
        interface: iface,
        usage,
        usage_page: usagePage,
        collection,
      })),
    getHidInfo: () => {
      const [readLength, writeLength] = native.hidInfo();
      return { readLength, writeLength };
    },
    getDeviceInfo: () => {
      const [manufacturer, product, serialNumber] = native.deviceInfo();
      return { product, manufacturer, serialNumber, vendorId: state.vendorId, productId: state.productId };
    },
    pause: (ms) => native.pause(Number(ms) || 0),
    color: (x, y) => native.color(toInt(x), toInt(y)),
    getBrightness: () => native.brightness(),
    setName: (name) => native.setName(String(name)),
    setSize: (size) => {
      if (size != null && typeof size.length === "number" && size.length >= 2) {
        native.setSize(toInt(size[0]), toInt(size[1]));
      }
    },
    setControllableLeds: (_names, positions) => native.setLeds(flatPositions(positions)),
    log: deviceLog,
    notify: (title, text) => {
      hostLog(`notify: ${String(title ?? "")}: ${String(text ?? "")}`);
      state.notifyId += 1;
      return String(state.notifyId);
    },
  });
}

// ---------------------------------------------------------------- SMBus
function busMembers(fixed) {
  const a = fixed ? 0 : 1; // argument offset: free buses take the address first
  return {
    WriteQuick: () => 0,
    ReadByte: () => 0,
    WriteByte: () => 0,
    ReadWord: () => 0,
    WriteWord: () => 0,
    WriteBlock: () => 0,
    ReadBlockBytes: (...args) => [0, zeros(args[1 + a])],
    WriteBlockWithoutRegister: () => 0,
    ReadBlockWithoutRegister: (...args) => [0, zeros(args[a])],
    ReadByteWithoutRegister: () => 0,
    TransactionLock: noop,
    TransactionUnlock: noop,
    HasTransactionLock: () => false,
    Vendor: () => 0,
    SubVendor: () => 0,
    Product: () => 0,
    SubDevice: () => 0,
    Name: () => "Lightloom stub bus",
    Type: () => (fixed ? "Fixed" : "Free"),
    GetAddress: () => 0,
    IsSystemBus: () => false,
    IsAMDBus: () => false,
    IsIntelBus: () => false,
    IsNvidiaBus: () => false,
    IsAMDGpuBus: () => false,
    IsNuvotonBus: () => false,
    FetchRamInfo: () => "",
    log: logFn,
    pause: noop,
    toString: () => (fixed ? "I2CBusWrapperFixedAddress(stub)" : "I2CBusWrapper(stub)"),
  };
}

// ---------------------------------------------------------------- network
// network-services.md §1.4 (moc slots + properties of the host `service`).
const serviceTarget = {
  name: "",
  icon: "",
  ui: "",
  view: null,
  pluginDir: "",
  controllersReady: true,
  controllers: [],
  devices: [],
  console: consoleStub,
  log: logFn,
  ignoreSslErrors: noop,
  addController: (obj) => {
    serviceTarget.controllers.push({ id: obj && obj.id, obj });
  },
  removeController: (obj) => {
    const i = serviceTarget.controllers.findIndex((c) => c.obj === obj || c.id === (obj && obj.id));
    if (i >= 0) {
      serviceTarget.controllers.splice(i, 1);
    }
  },
  updateController: noop,
  hasController: (id) => serviceTarget.controllers.some((c) => c.id === id),
  getController: (id) => {
    const c = serviceTarget.controllers.find((x) => x.id === id);
    return c ? c.obj : undefined;
  },
  getSetting: () => undefined,
  saveSetting: noop,
  removeSetting: noop,
  announceController: noop,
  suppressController: noop,
  broadcast: noop,
  callDiscoveryFunction: noop,
  callControllerFunction: noop,
  resolve: noop,
  resolveMacAddress: () => "",
  saveSnapshot: noop,
  showSettings: noop,
  hideSettings: noop,
};

const controllerTarget = {
  id: "00:00:00:00:00:00",
  ip: "127.0.0.1",
  port: 0,
  name: "Lightloom stub controller",
  hostname: "stub.local",
  mac: "00:00:00:00:00:00",
};

// HTTP: plugins define their own `class XmlHttp` over XMLHttpRequest; the
// global is provided for the few that do not. Requests never leave the host:
// every request completes synchronously with status 0 ("unreachable").
class XMLHttpRequestStub {
  constructor() {
    this.readyState = 0;
    this.status = 0;
    this.statusText = "";
    this.response = "";
    this.responseText = "";
    this.responseType = "";
    this.timeout = 0;
    this.onreadystatechange = null;
    this.onload = null;
    this.onerror = null;
    this.ontimeout = null;
  }
  open() {
    this.readyState = 1;
  }
  setRequestHeader() {}
  getResponseHeader() {
    return null;
  }
  getAllResponseHeaders() {
    return "";
  }
  abort() {}
  send() {
    this.readyState = 4;
    this.status = 0;
    if (typeof this.onreadystatechange === "function") {
      this.onreadystatechange();
    }
    if (typeof this.onerror === "function") {
      this.onerror();
    }
  }
}
XMLHttpRequestStub.UNSENT = 0;
XMLHttpRequestStub.OPENED = 1;
XMLHttpRequestStub.HEADERS_RECEIVED = 2;
XMLHttpRequestStub.LOADING = 3;
XMLHttpRequestStub.DONE = 4;

const xmlHttpTarget = {
  Get: noop,
  Post: noop,
  Put: noop,
  Delete: noop,
  GetWithAuth: noop,
  PostWithAuth: noop,
  PutWithAuth: noop,
};

// ---------------------------------------------------------------- @SignalRGB/* stub modules
// Used instead of the throwing prelude facades when the host runs with
// `module_stubs` (the dry-run default): the facade policy is still exercised
// by the prelude tests, the dry-run wants to know how far plugins get.
const modules = {
  udp: recording("@SignalRGB/udp", { createSocket: socketStub, send: (_ip, _port, d) => (d && d.length) || 0 }),
  tcp: recording("@SignalRGB/tcp", { createSocket: socketStub }),
  serial: recording("@SignalRGB/serial", {
    connect: () => true,
    disconnect: noop,
    isConnected: () => true,
    write: (d) => (d && d.length) || 0,
    read: () => [],
    readAll: () => [],
    getDeviceInfo: () => ({ deviceId: "", vendorId: state.vendorId, productId: state.productId, portName: "" }),
    getPortName: () => "",
    getBaudRate: () => 115200,
    availablePorts: () => [],
  }),
  lcd: recording("@SignalRGB/lcd", {
    initialize: noop,
    getFrame: () => [],
    getSlicedFrame: () => [],
    getImageBuffer: () => [],
  }),
  // `{ [partner]: { fans, lighting, macros } }` (hid-usb.md §3); every partner the corpus checks, all granted.
  permissions: recording("@SignalRGB/permissions", {
    permissions: () => ({
      "Lian-Li": { fans: true, lighting: true, macros: true },
      MSI: { fans: true, lighting: true, macros: true },
    }),
    setCallback: noop,
  }),
  systeminfo: recording("@SignalRGB/systeminfo", {
    GetMotherboardInfo: () => ({ manufacturer: "", model: "" }),
    GetBiosInfo: () => ({ releaseDate: "" }),
    GetRamInfo: () => [],
  }),
  DeviceDiscovery: recording("@SignalRGB/DeviceDiscovery", { foundVirtualDevice: noop }),
  performance: recording("@SignalRGB/performance", {
    StartTimer: noop,
    EndTimer: noop,
    LogReport: noop,
    GetTimers: () => ({}),
  }),
  appInfo: recording("@SignalRGB/appInfo", { version: "0.0.1", compat: "signalrgb-2.5", name: "Lightloom" }),
};

// ---------------------------------------------------------------- device-session modules
// Real `@SignalRGB/*` modules of a device session (the loader falls back to the
// throwing facade for the others).

// Partner permissions (MSI, Lian-Li): SignalRGB fetches them from its servers
// and plugins treat a missing entry as granted. Lightloom has no partner gate,
// the user controls their own hardware, so everything is granted and never changes.
const PARTNERS = ["MSI", "Lian-Li"];
const permissionsModule = freeze({
  permissions: () => fromEntries(PARTNERS.map((p) => [p, { fans: true, lighting: true, macros: true }])),
  setCallback: noop,
});

// Motherboard / BIOS / memory facts (SMBIOS, the source of SignalRGB's WMI queries).
function systemInfo() {
  const text = native.systemInfo();
  return text === "" ? undefined : parseJson(text);
}
const systeminfoModule = freeze({
  GetMotherboardInfo: () => {
    const info = systemInfo();
    return info && { manufacturer: info.motherboard.manufacturer, model: info.motherboard.model };
  },
  GetBiosInfo: () => {
    const info = systemInfo();
    return info && { vendor: info.bios.vendor, version: info.bios.version, releaseDate: info.bios.release_date };
  },
  GetRamInfo: () => {
    const info = systemInfo();
    return info
      ? info.ram.map((m) => ({
          manufacturer: m.manufacturer,
          partNumber: m.part_number,
          serialNumber: m.serial_number,
          locator: m.locator,
          capacity: m.size_mb * 1024 * 1024,
          speed: m.speed_mts,
        }))
      : [];
  },
});

const nativeModules = native === undefined ? {} : { permissions: permissionsModule, systeminfo: systeminfoModule };

// ---------------------------------------------------------------- install
const g = globalThis;
g.__lightloom = {
  plugin: cfg.plugin ?? "",
  log(_level, msg) {
    const text = String(msg);
    const m = /^API_GAP: (\S+) is not implemented/.exec(text);
    if (m) {
      gap(m[1]);
    }
    hostLog(text);
  },
};
g.console = consoleStub;
g.device = recording("device", deviceTarget);
// SMBus is not available to device sessions yet (M5): every `bus.*` member throws there.
g.bus = native !== undefined ? recording("bus", {}) : recording("bus", busMembers(true));
g.service = recording("service", serviceTarget);
// Plain-data snapshot of the plugin's own controller object / the plugin's
// DiscoveryService instance: unknown fields are plugin data, not host API.
g.controller = recording("controller", controllerTarget, { data: true });
g.discovery = recording("discovery", {}, { data: true });
g.XmlHttp = recording("XmlHttp", xmlHttpTarget);
g.XMLHttpRequest = XMLHttpRequestStub;
g.BinaryUtils = BinaryUtils;

// ---------------------------------------------------------------- host-side helpers (called from Rust)
const METADATA = [
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
];
const LIFECYCLE = ["Validate", "Initialize", "Render", "Shutdown", "Scan", "DiscoveryService"];

function describeError(e) {
  if (e && typeof e === "object") {
    return { name: String(e.name ?? "Error"), message: String(e.message ?? e) };
  }
  return { name: "Throw", message: String(e) };
}

// JSON-safe copy (functions dropped, cycles cut).
function plain(value, depth = 0) {
  if (value === null || value === undefined) {
    return null;
  }
  const t = typeof value;
  if (t === "number") {
    return isFiniteNumber(value) ? value : null;
  }
  if (t === "string" || t === "boolean") {
    return value;
  }
  if (t === "bigint") {
    return Number(value);
  }
  if (t !== "object" || depth > 8) {
    return null;
  }
  if (isArray(value) || isView(value)) {
    return arrayFrom(value, (v) => plain(v, depth + 1));
  }
  const out = {};
  for (const k of objectKeys(value)) {
    const v = plain(value[k], depth + 1);
    if (v !== null || value[k] === null) {
      out[k] = v;
    }
  }
  return out;
}

/** Apply the call-if-function rule to every metadata export; returns a JSON string. */
function readMeta(ns) {
  const exports = {};
  const errors = {};
  for (const name of METADATA) {
    if (!(name in ns)) {
      continue;
    }
    try {
      let v = ns[name];
      if (typeof v === "function") {
        v = v();
      }
      if (name === "ProductId" && v && typeof v === "object" && !isArray(v)) {
        v = objectKeys(v);
      }
      exports[name] = plain(v);
    } catch (e) {
      const d = describeError(e);
      errors[name] = `${d.name}: ${d.message}`;
    }
  }
  const lifecycle = {};
  for (const name of LIFECYCLE) {
    lifecycle[name] = typeof ns[name] === "function";
  }
  const names = objectKeys(ns);
  const hooks = names.filter((k) => /^on.+Changed$/.test(k) && typeof ns[k] === "function");
  return stringify({ exports, errors, lifecycle, hooks, names });
}

/**
 * The global value of a ControllableParameters / addProperty entry: its
 * `default`, with the numeric and boolean strings of the corpus coerced
 * (`"800"` -> 800 for number/hue, `"true"` -> true for boolean; hid-usb.md
 * critic pass).
 */
function coerceDefault(p) {
  let v = p.default;
  const type = String(p.type ?? "").toLowerCase();
  if ((type === "number" || type === "hue") && typeof v === "string" && v.trim() !== "" && !Number.isNaN(Number(v))) {
    v = Number(v);
  } else if (type === "boolean" && typeof v === "string") {
    v = v === "true";
  }
  return v;
}

/** Bind ControllableParameters defaults as globals (before Initialize). */
function applyParameters(params) {
  if (!isArray(params)) {
    return 0;
  }
  let n = 0;
  for (const p of params) {
    if (p && typeof p.property === "string" && p.property.length > 0) {
      g[p.property] = coerceDefault(p);
      n += 1;
    }
  }
  return n;
}

/** Override parameter globals from `{ name: value }` (CLI `--param`, UI changes). */
function setGlobals(values) {
  if (values === null || typeof values !== "object") {
    return 0;
  }
  let n = 0;
  for (const k of objectKeys(values)) {
    g[k] = values[k];
    n += 1;
  }
  return n;
}

function setIds(vendorId, productId) {
  state.vendorId = Number(vendorId) || 0;
  state.productId = Number(productId) || 0;
}

/** Keep the evaluated plugin's namespace out of reach of plugin code. */
function setNamespace(ns) {
  state.namespace = ns;
}

/** The plugin namespace recorded by `setNamespace` (undefined before). */
function namespace() {
  return state.namespace;
}

/** Host-side `Scan(bus)` argument. */
function freeBus() {
  return recording("bus", busMembers(false));
}

function report() {
  return stringify({
    gaps: fromEntries(state.gaps),
    fields: fromEntries(state.fields),
    calls: fromEntries(state.calls),
    consoleLines: state.consoleLines,
    logTail: state.logTail,
  });
}

defineProperty(g, "__lightloomHost", {
  value: freeze({
    readMeta,
    applyParameters,
    setGlobals,
    setIds,
    setNamespace,
    namespace,
    freeBus,
    report,
    describeError,
    modules: freeze(modules),
    nativeModules: freeze(nativeModules),
  }),
  writable: false,
  enumerable: false,
  configurable: false,
});
