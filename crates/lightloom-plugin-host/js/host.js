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
//   (architecture-synthesis.md D6). An SMBus device session also gets the
//   `bus*` natives (src/bus.rs): the global `bus` is then the fixed-address bus.
// * "scan" (`Scan(bus)` of an SMBus plugin): only the `bus*`, `log` and
//   `pause` natives exist; `freeBus()` hands `Scan` the free-address bus.
// * "service" (the discovery side of a network plugin): `service.*` over the
//   `svc*` natives (src/service.rs), `discovery` = `new DiscoveryService()`.
//
// A network service or network device session also gets the `net*` / `http*`
// natives (src/net): real `@SignalRGB/udp` / `tcp` sockets, `XMLHttpRequest`,
// the `udp` feature; `controller` is then the announced controller data
// (docs/host-api/network-lightloom.md).
//
// A serial device session (`Type() == "serial"`) also gets the `serial*`
// natives (src/serial.rs): `@SignalRGB/serial` is then the real module, bound
// to the ports of the matched USB device (docs/host-api/hid-usb.md §3).
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
const native =
  cfg.mode === "device" || cfg.mode === "scan" || cfg.mode === "service" ? globalThis.__lightloomNative : undefined;
const deviceMode = cfg.mode === "device";
const serviceMode = cfg.mode === "service";
const netNative = native !== undefined && typeof native.netCreate === "function";
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
  // The `battery` feature: undefined until added; level -1 until set.
  battery: undefined,
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
  battery: () => batteryFeature(),
};

// ---------------------------------------------------------------- battery
// hid-usb.md §addFeature (battery gap): numeric states 0 unknown, 1 draining,
// 2 charging, 3 full (charging), 4 full, 5 wireless charging; the named
// constants carry the same numbers (`disabled` has none in the corpus: 0).
const BATTERY_STATES = ["Unknown", "Draining", "Charging", "Full (charging)", "Full", "Wireless charging"];
const DEFAULT_BATTERY_WARNING_LEVEL = 15;

/** A `setBatteryLevel` argument as 0..100 (rounded), or undefined when not a finite number. */
function batteryLevelArg(level) {
  const n = Number(level);
  return isFiniteNumber(n) ? Math.min(100, Math.max(0, Math.round(n))) : undefined;
}

/** A `setBatteryState` argument: 0..5, anything else is 0 (unknown). */
function batteryStateArg(value) {
  const n = Math.trunc(Number(value));
  return n >= 0 && n < BATTERY_STATES.length ? n : 0;
}

/**
 * The `battery` global. Device mode: the natives keep the level and state
 * (the daemon reads them) and report the user's low-battery warning level.
 * Stub mode: kept here and listed in the dry-run report.
 */
function batteryFeature() {
  let read;
  let setLevel;
  let setState;
  if (deviceMode) {
    native.batteryEnable();
    read = () => {
      const [level, st, warn] = native.batteryGet();
      return { level, state: st, warn };
    };
    setLevel = (n) => native.batterySetLevel(n);
    setState = (n) => native.batterySetState(n);
  } else {
    state.battery ??= { level: -1, state: 0 };
    read = () => ({ ...state.battery, warn: DEFAULT_BATTERY_WARNING_LEVEL });
    setLevel = (n) => {
      state.battery.level = n;
    };
    setState = (n) => {
      state.battery.state = n;
    };
  }
  return recording("battery", {
    disabled: 0,
    unknown: 0,
    draining: 1,
    charging: 2,
    fullCharging: 3,
    full: 4,
    wirelessCharging: 5,
    get batteryLevel() {
      return Math.max(0, read().level);
    },
    get batteryState() {
      return read().state;
    },
    get stateString() {
      return BATTERY_STATES[read().state] ?? "Unknown";
    },
    get lowBatteryWarningLevel() {
      return read().warn;
    },
    setBatteryLevel: (level) => {
      const n = batteryLevelArg(level);
      if (n !== undefined) {
        setLevel(n);
      }
    },
    setBatteryState: (value) => setState(batteryStateArg(value)),
  });
}

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
  getHidReportFields: () => null,
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
  control_transfer: (requestType, _rq, _v, _i, data, length) =>
    Number(requestType) & 0x80 ? zeros(length) : Number(length) || (data && data.length) || 0,
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

/**
 * The `keyboard` / `mouse` feature of a device session: events go to the macro
 * runtime (crates/lightloom-plugin-host/src/input.rs). Plain objects: members
 * SignalRGB lacks (`keyboard.sendKey`) stay `undefined` for plugins that test them.
 */
function inputFeature(kind) {
  const send = (what, data) => native.inputEvent(what, JSON.stringify(data));
  const opts = (o) => (o !== null && typeof o === "object" ? o : {});
  const hid = (what, vk, options, extra) => {
    if (typeof vk !== "number") {
      throw new TypeError("sendHid(): vkCode must be a number!");
    }
    const o = opts(options);
    if (o.released === undefined) {
      throw new TypeError("sendHid(): released option must be specified!");
    }
    send(what, { vk, released: Boolean(o.released), ...extra(o) });
  };
  if (kind === "keyboard") {
    return {
      sendEvent: (data) => {
        const d = opts(data);
        send("key", { key: d.key === undefined ? "" : String(d.key), keyCode: Number(d.keyCode) || 0, released: Boolean(d.released) });
      },
      sendHid: (vk, options) => hid("keyHid", vk, options, () => ({})),
    };
  }
  return {
    sendEvent: (data) => {
      const d = opts(data);
      send("button", {
        name: d.name === undefined ? "" : String(d.name),
        buttonCode: Number(d.buttonCode) || 0,
        released: Boolean(d.released),
      });
    },
    sendHid: (vk, options) =>
      hid("buttonHid", vk, options, (o) => (o.XButton === undefined ? {} : { XButton: Number(o.XButton) })),
  };
}

if (deviceMode) {
  const stubAddFeature = deviceTarget.addFeature;
  Object.assign(deviceTarget, {
    // Not implemented in device sessions yet: these throw instead of answering
    // with stub data (fans: M5, LCD: M6).
    ...unimplemented("device", [
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
    // Lightloom never drives fans (fan control is a future opt-in feature,
    // docs/sensors.md); plugins that check this skip fan control and their
    // temperature probes, and the fan API below stays a no-op.
    fanControlDisabled: () => true,
    // Network device sessions get the real `udp` feature; DTLS is not provided.
    addFeature: (name) => {
      const key = String(name);
      if (key === "udp" && netNative) {
        const current = globalThis.udp;
        if (current === undefined || current.__lightloomFeature !== "udp") {
          const feature = recording("udp", { ...udpFeatureMembers() });
          defineProperty(feature, "__lightloomFeature", { value: "udp" });
          globalThis.udp = feature;
        }
        return;
      }
      if (key === "udp" || key === "dtls") {
        unimplementedMember(`device.addFeature(${key})`)();
      }
      if (key === "keyboard" || key === "mouse") {
        const current = globalThis[key];
        if (current === undefined || current.__lightloomFeature !== key) {
          const feature = inputFeature(key);
          defineProperty(feature, "__lightloomFeature", { value: key });
          globalThis[key] = feature;
        }
        return;
      }
      return stubAddFeature(name);
    },
    // Raw USB (hid-usb.md §2): the direction comes from bit 7 of the endpoint /
    // bmRequestType. OUT returns the bytes sent or -1, IN the bytes received
    // (empty on timeout or failure, getLastReadSize() of them). Default timeout 1000 ms.
    bulk_transfer: (endpoint, data, length, timeout) => {
      const ep = toInt(endpoint) & 0xff;
      return ep & 0x80
        ? native.usbBulkIn(ep, lengthArg(data, length), timeoutArg(timeout, -1))
        : native.usbBulkOut(ep, bytesArg(data, length), timeoutArg(timeout, -1));
    },
    control_transfer: (requestType, request, value, index, data, length, timeout) => {
      const setup = [
        toInt(requestType) & 0xff,
        toInt(request) & 0xff,
        toInt(value) & 0xffff,
        toInt(index) & 0xffff,
        timeoutArg(timeout, -1),
      ];
      return setup[0] & 0x80
        ? native.usbControlIn(setup, Math.min(lengthArg(data, length), 0xffff))
        : native.usbControlOut(setup, bytesArg(data, length));
    },
    setProtocol: (name) => {
      native.setProtocol(strArg(name));
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
    // Lightloom extension: the open collection's reports and fields from its report
    // descriptor ({ usagePage, usage, reports, fields }), or null.
    getHidReportFields: () => JSON.parse(native.hidReportFields()),
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
    setControllableLeds: (names, positions) => {
      native.setLeds(flatPositions(positions));
      native.setLedNames(isArray(names) ? names.map(strArg) : []);
    },
    log: deviceLog,
    // Properties and messages (components-channels.md §2, hid-usb.md "notify"):
    // the natives keep the device's lists for the dashboard; the JS objects stay
    // the plugin's (`getProperty(name)`, mutate, `addProperty(obj)` updates).
    addProperty: (obj) => {
      if (obj === null || typeof obj !== "object") {
        native.propAdd("null");
        return;
      }
      if (typeof obj.property === "string" && obj.property !== "") {
        state.properties.set(obj.property, obj);
        if (!(obj.property in globalThis)) {
          globalThis[obj.property] = coerceDefault(obj);
        }
      }
      native.propAdd(stringify(plain(obj)));
    },
    removeProperty: (name) => {
      state.properties.delete(strArg(name));
      native.propRemove(strArg(name));
    },
    addMessage: (id, message, tooltip) => native.msgAdd(strArg(id), strArg(message), strArg(tooltip)),
    removeMessage: (id) => native.msgRemove(strArg(id)),
    notify: (title, text, priority) => {
      hostLog(`notify: ${strArg(title)}: ${strArg(text)}`);
      state.notifyId += 1;
      const id = `lightloom-alert-${state.notifyId}`;
      native.msgNotify(id, strArg(title), strArg(text), toInt(priority));
      return id;
    },
    denotify: (id) => native.msgRemove(strArg(id)),
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
    TransactionLock: () => true,
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
    SpdSlots: () => [],
    IsDryRun: () => true,
    log: logFn,
    pause: noop,
    toString: () => (fixed ? "I2CBusWrapperFixedAddress(stub)" : "I2CBusWrapper(stub)"),
  };
}

// ---------------------------------------------------------------- SMBus (native)
// smbus.md §3: the engine validates arguments and throws for bad ones;
// transfers never throw, they return a negative status. Lightloom extensions
// (docs/host-api/smbus.md, Lightloom section): WriteBlock clips data longer
// than `len` (logged once) and zero-pads shorter data.

function busNumber(method, what, v, max) {
  if (typeof v !== "number" || !isFiniteNumber(v) || v < 0 || v > max) {
    throw new RangeError(`${method}(): ${what} must be a number between 0 and ${max}`);
  }
  return Math.trunc(v);
}

let clipLogged = false;

function busData(method, len, data, max) {
  if (!isArray(data) && !isView(data)) {
    throw new TypeError(`${method}(): Data must be an array.`);
  }
  if (typeof len !== "number" || !isFiniteNumber(len) || len < 0 || len > 255) {
    throw new RangeError(`${method}(): Length must be a number between 0 and ${max}`);
  }
  const n = Math.trunc(len);
  if (n > max) {
    throw new RangeError(`${method}(): This operation has a max length of ${max} Bytes`);
  }
  if (data.length > n && !clipLogged) {
    clipLogged = true;
    hostLog(`bus: ${method}(): Array Size [${data.length}] is greater than length parameter [${n}]; sending ${n} bytes`);
  }
  const out = new Array(n);
  for (let i = 0; i < n; i++) {
    out[i] = i < data.length ? Number(data[i]) & 0xff : 0;
  }
  return out;
}

function splitBlock(raw) {
  return [raw[0], raw.slice(1)];
}

/** The `bus` object over the natives: fixed (bound address) or free (address first). */
function nativeBusMembers(fixed) {
  const info = parseJson(native.busInfo());
  const kind = info.kind;
  const bound = info.address ?? 0;
  const MAX_BLOCK = 32;
  // Free buses take the address first; fixed buses use the bound one.
  const call = (method, args, count, body) => {
    if (fixed) {
      return body(bound, ...args.slice(0, count));
    }
    const addr = busNumber(method, "Address", args[0], 255);
    return body(addr, ...args.slice(1, 1 + count));
  };
  const members = {
    WriteQuick: (...args) => call("WriteQuick", args, 0, (a) => native.busQuick(a)),
    ReadByte: (...args) =>
      call("ReadByte", args, 1, (a, r) => native.busReadByte(a, busNumber("ReadByte", "Register", r, 255))),
    WriteByte: (...args) =>
      call("WriteByte", args, 2, (a, r, v) =>
        native.busWriteByte(a, busNumber("WriteByte", "Register", r, 255), busNumber("WriteByte", "Byte", v, 255)),
      ),
    ReadWord: (...args) =>
      call("ReadWord", args, 1, (a, r) => native.busReadWord(a, busNumber("ReadWord", "Register", r, 255))),
    WriteWord: (...args) =>
      call("WriteWord", args, 2, (a, r, w) =>
        native.busWriteWord(a, busNumber("WriteWord", "Register", r, 255), busNumber("WriteWord", "Word", w, 65535)),
      ),
    WriteBlock: (...args) =>
      call("WriteBlock", args, 3, (a, r, len, data) =>
        native.busWriteBlock(a, busNumber("WriteBlock", "Register", r, 255), busData("WriteBlock", len, data, MAX_BLOCK)),
      ),
    ReadBlockBytes: (...args) =>
      call("ReadBlockBytes", args, 2, (a, r, len) =>
        splitBlock(
          native.busReadBlock(
            a,
            busNumber("ReadBlockBytes", "Register", r, 255),
            busNumber("ReadBlockBytes", "Length", len, MAX_BLOCK),
          ),
        ),
      ),
    WriteBlockWithoutRegister: (...args) =>
      call("WriteBlockWithoutRegister", args, 2, (a, len, data) =>
        native.busWriteNoReg(a, busData("WriteBlockWithoutRegister", len, data, MAX_BLOCK)),
      ),
    ReadBlockWithoutRegister: (...args) =>
      call("ReadBlockWithoutRegister", args, 1, (a, len) =>
        splitBlock(native.busReadNoReg(a, busNumber("ReadBlockWithoutRegister", "Length", len, MAX_BLOCK))),
      ),
    ReadByteWithoutRegister: (...args) => call("ReadByteWithoutRegister", args, 0, (a) => native.busReadByteNoReg(a)),
    // Lightloom answers whether the bus is held (SignalRGB returns nothing).
    TransactionLock: () => native.busLock(),
    // Lightloom: 0, or the status of the first write queued under SmbusBatchWrites() that failed.
    TransactionUnlock: () => native.busUnlock(),
    HasTransactionLock: () => native.busHasLock(),
    Vendor: () => info.vendor,
    SubVendor: () => info.subVendor,
    Product: () => info.device,
    SubDevice: () => info.subDevice,
    Name: () => info.name,
    Type: () => (fixed ? "Fixed" : "Free"),
    GetAddress: () => bound,
    IsSystemBus: () => kind === "system" || kind === "amd" || kind === "intel",
    IsAMDBus: () => kind === "amd",
    IsIntelBus: () => kind === "intel",
    IsNvidiaBus: () => kind === "nvidia",
    IsAMDGpuBus: () => kind === "amdgpu",
    IsNuvotonBus: () => kind === "nuvoton",
    FetchRamInfo: unimplementedMember("bus.FetchRamInfo"),
    // Lightloom extension (docs/host-api/smbus.md, SPD check): the DIMM slots whose SPD names
    // a manufacturer of the plugin's SpdManufacturers(), read by the host before Scan(bus).
    // Copies; reading them touches no hardware. Empty where no SPD check ran.
    SpdSlots: () => (isArray(info.spdSlots) ? info.spdSlots.map((s) => ({ ...s })) : []),
    // Lightloom extension: writes are recorded, never sent (--dry-run); reads still reach the bus.
    IsDryRun: () => info.dryRun === true,
    log: logFn,
    pause: (ms) => native.pause(Number(ms) || 0),
    toString: () =>
      fixed
        ? `I2CBusWrapperFixedAddress(${info.name} @0x${bound.toString(16).padStart(2, "0")})`
        : `I2CBusWrapper(${info.name})`,
  };
  if (fixed) {
    // The fixed bus has no free-only member (smbus.md §4); SpdSlots belongs to Scan(bus).
    delete members.WriteQuick;
    delete members.SpdSlots;
  }
  return members;
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

// ---------------------------------------------------------------- network (native)
// docs/host-api/network-lightloom.md: sockets and XMLHttpRequest over the `net*`
// / `http*` natives. What arrives is queued on the Rust side and delivered by
// `pumpNet()`, which the host calls between plugin calls.

const SOCKET_STATES = {
  UnconnectedState: 0,
  HostLookupState: 1,
  ConnectingState: 2,
  ConnectedState: 3,
  BoundState: 4,
  ListeningState: 5,
  ClosingState: 6,
};
const SOCKET_EVENTS = ["message", "close", "listening", "connection", "error"];
const MAX_PAYLOAD = 65536;

/** UTF-8 bytes of a string. */
function utf8(text) {
  const out = [];
  for (let i = 0; i < text.length; i++) {
    let cp = text.codePointAt(i);
    if (cp > 0xffff) {
      i++;
    }
    if (cp >= 0xd800 && cp <= 0xdfff) {
      cp = 0xfffd; // lone surrogate
    }
    if (cp < 0x80) {
      out.push(cp);
    } else if (cp < 0x800) {
      out.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
    } else if (cp < 0x10000) {
      out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
    } else {
      out.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 0x3f), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
    }
  }
  return out;
}

/** A send payload as bytes: a string (UTF-8), byte array, typed array or ArrayBuffer. */
function payloadBytes(data) {
  if (typeof data === "string") {
    return utf8(data);
  }
  if (data instanceof ArrayBuffer) {
    return arrayFrom(new Uint8Array(data));
  }
  if (isView(data)) {
    return arrayFrom(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
  }
  if (data != null && typeof data.length === "number") {
    const n = Math.min(data.length >>> 0, MAX_PAYLOAD);
    const out = new Array(n);
    for (let i = 0; i < n; i++) {
      out[i] = Number(data[i]) & 0xff;
    }
    return out;
  }
  hostLog(`Message type is not supported: [${typeof data}]`);
  return [];
}

const socketEntries = new Map();

/** A socket object over native socket `existing` (an accepted client) or a new one. */
function makeSocket(tcp, existing) {
  const id = existing ?? native.netCreate(tcp);
  const handlers = { message: [], close: [], listening: [], connection: [], error: [] };
  const target = {
    ...SOCKET_STATES,
    get state() {
      return id < 0 ? 0 : native.netState(id);
    },
    on(event, cb) {
      if (typeof cb !== "function") {
        throw new TypeError("on(): callback must be a function!");
      }
      const name = String(event);
      if (!SOCKET_EVENTS.includes(name)) {
        hostLog(`on(): unknown event type: ${name}`);
        return;
      }
      handlers[name].push(cb);
    },
    bind: (port) => id >= 0 && !tcp && native.netBind(id, toInt(port)),
    connect: (host, port) => id >= 0 && native.netConnect(id, strArg(host), toInt(port)),
    send: (data) => (id < 0 ? -1 : native.netSend(id, payloadBytes(data))),
    write: (data, host, port) =>
      id < 0 || tcp ? -1 : native.netSendTo(id, payloadBytes(data), strArg(host), toInt(port)),
    // Lightloom extension: listen on the LAN address facing `peer`, accepting `peer` only.
    listen: (port, peer) => {
      if (id < 0 || !tcp) {
        return null;
      }
      const at = native.netListen(id, toInt(port), strArg(peer));
      return at === "" ? null : parseJson(at);
    },
    close: () => {
      if (id >= 0) {
        native.netClose(id);
      }
    },
    disconnect: () => {
      if (id >= 0) {
        native.netDisconnect(id);
      }
    },
    address: () => parseJson(native.netAddress(id)),
    remoteAddress: () => parseJson(native.netRemote(id)),
  };
  if (id < 0) {
    hostLog("createSocket(): no socket (too many open)");
  }
  const socket = recording("socket", target);
  if (id >= 0) {
    socketEntries.set(id, { socket, handlers });
  }
  return socket;
}

/** `udp.send(host, port, data, endianness)` over one socket made on first use. */
function udpFeatureMembers() {
  let socket;
  return {
    send: (host, port, data) => {
      socket ??= makeSocket(false);
      return socket.write(data, host, port);
    },
  };
}

function callSafely(fn, self, args, what) {
  try {
    fn.apply(self, args);
  } catch (e) {
    const d = describeError(e);
    hostLog(`${what}: ${d.name}: ${d.message}`);
  }
}

const pendingXhr = new Map();
const xhrState = new WeakMap();

function finishXhr(x, r, guarded) {
  const st = xhrState.get(x);
  if (st === undefined || st.aborted) {
    return;
  }
  st.responseHeaders = isArray(r.headers) ? r.headers : [];
  x.status = Number(r.status) | 0;
  x.statusText = String(r.statusText ?? "");
  x.responseText = String(r.body ?? "");
  x.responseURL = st.url;
  if (x.responseType === "json") {
    try {
      x.response = parseJson(x.responseText);
    } catch {
      x.response = null;
    }
  } else {
    x.response = x.responseText;
  }
  x.readyState = 4;
  const fire = (name) => {
    const f = x[name];
    if (typeof f === "function") {
      const ev = { type: name.slice(2), target: x };
      if (guarded) {
        callSafely(f, x, [ev], `XMLHttpRequest.${name}`);
      } else {
        f.call(x, ev);
      }
    }
  };
  fire("onreadystatechange");
  if (x.status === 0) {
    fire(r.timedOut ? "ontimeout" : "onerror");
  } else {
    fire("onload");
  }
  fire("onloadend");
}

/** XMLHttpRequest over the host's HTTP client (http:// on the LAN; sync or async). */
class NetXMLHttpRequest {
  constructor() {
    this.readyState = 0;
    this.status = 0;
    this.statusText = "";
    this.response = "";
    this.responseText = "";
    this.responseType = "";
    this.responseURL = "";
    this.timeout = 0;
    this.withCredentials = false;
    this.onreadystatechange = null;
    this.onload = null;
    this.onerror = null;
    this.ontimeout = null;
    this.onloadend = null;
    this.onabort = null;
    xhrState.set(this, { method: "GET", url: "", async: true, headers: [], responseHeaders: [], aborted: false });
  }
  open(method, url, async = true) {
    const st = xhrState.get(this);
    st.method = String(method ?? "GET").toUpperCase();
    st.url = String(url ?? "");
    st.async = async !== false;
    st.headers = [];
    st.responseHeaders = [];
    st.aborted = false;
    this.readyState = 1;
    this.status = 0;
    this.responseText = "";
    this.response = "";
  }
  setRequestHeader(name, value) {
    xhrState.get(this).headers.push([String(name), String(value)]);
  }
  getResponseHeader(name) {
    const key = String(name).toLowerCase();
    const h = xhrState.get(this).responseHeaders.find(([n]) => n === key);
    return h ? h[1] : null;
  }
  getAllResponseHeaders() {
    return xhrState
      .get(this)
      .responseHeaders.map(([n, v]) => `${n}: ${v}\r\n`)
      .join("");
  }
  abort() {
    const st = xhrState.get(this);
    st.aborted = true;
    if (typeof this.onabort === "function") {
      callSafely(this.onabort, this, [{ type: "abort", target: this }], "XMLHttpRequest.onabort");
    }
  }
  send(body = null) {
    const st = xhrState.get(this);
    if (this.readyState !== 1) {
      throw new Error("XMLHttpRequest.send(): call open() first");
    }
    const t = Number(this.timeout);
    const req = stringify({
      method: st.method,
      url: st.url,
      headers: st.headers,
      body: body === null || body === undefined ? null : typeof body === "string" ? body : String(body),
      timeout: Number.isFinite(t) && t > 0 ? Math.trunc(t) : 0,
    });
    if (st.async) {
      this.readyState = 2;
      pendingXhr.set(native.httpAsync(req), this);
    } else {
      finishXhr(this, parseJson(native.httpSync(req)), false);
    }
  }
}
NetXMLHttpRequest.UNSENT = 0;
NetXMLHttpRequest.OPENED = 1;
NetXMLHttpRequest.HEADERS_RECEIVED = 2;
NetXMLHttpRequest.LOADING = 3;
NetXMLHttpRequest.DONE = 4;

const udpModule = netNative ? freeze({ createSocket: () => makeSocket(false), ...udpFeatureMembers() }) : undefined;
const tcpModule = netNative ? freeze({ createSocket: () => makeSocket(true) }) : undefined;

// ---------------------------------------------------------------- network service (native)
// network-services.md §1.4 over the `svc*` natives; dashboard conventions in
// network-lightloom.md.

const controllerWrappers = [];
const wrapperSet = new WeakSet();
let controllersDirty = false;
let controllerWarning = false;
let discoveryInstance;
const missingWarned = new Set();

const controllerOf = (obj) => (obj !== null && typeof obj === "object" && wrapperSet.has(obj) ? obj.obj : obj);

function settingPart(what, v) {
  const text = String(v);
  if (text.includes("/") || text.includes("..")) {
    throw new Error(`${what} cannot contain a '/' or '..'`);
  }
  return text;
}

const nativeService = {
  name: "",
  icon: "",
  ui: "",
  view: null,
  pluginDir: "",
  controllersReady: true,
  get controllers() {
    return controllerWrappers;
  },
  devices: [],
  console: consoleStub,
  log: (msg, options) => {
    if (typeof msg === "string") {
      hostLog(msg);
    } else {
      try {
        hostLog(options && options.pretty ? stringify(msg, null, 2) : stringify(msg));
      } catch {
        hostLog(String(msg));
      }
    }
  },
  addController: (obj) => {
    const target = controllerOf(obj);
    if (target === null || typeof target !== "object") {
      throw new TypeError("addController(): instance must be an object!");
    }
    if (target.id === undefined) {
      throw new TypeError("addController(): instance must have an 'id' property!");
    }
    controllersDirty = true;
    if (controllerWrappers.some((w) => w.obj === target || w.id === target.id)) {
      return;
    }
    const wrapper = { id: target.id, obj: target };
    wrapperSet.add(wrapper);
    controllerWrappers.push(wrapper);
    if (controllerWrappers.length > 50 && !controllerWarning) {
      controllerWarning = true;
      hostLog("Warning: Controller list size exceeds 50! There may be a memory leak in the plugin");
    }
  },
  removeController: (obj) => {
    const target = controllerOf(obj);
    const id = target !== null && typeof target === "object" ? target.id : undefined;
    const i = controllerWrappers.findIndex((w) => w.obj === target || (id !== undefined && w.id === id));
    if (i >= 0) {
      controllerWrappers.splice(i, 1);
      controllersDirty = true;
    }
  },
  updateController: () => {
    controllersDirty = true;
  },
  hasController: (id) => controllerWrappers.some((w) => w.id === id),
  getController: (id) => {
    const w = controllerWrappers.find((x) => x.id === id);
    return w ? w.obj : undefined;
  },
  announceController: (obj) => {
    const target = controllerOf(obj);
    if (target === null || typeof target !== "object" || target.id === undefined) {
      throw new TypeError("announceController(): instance must have an 'id' property!");
    }
    controllersDirty = true;
    return native.svcAnnounce(String(target.id), stringify(plain(target) ?? {}));
  },
  suppressController: (obj) => {
    const target = controllerOf(obj);
    const id = target !== null && typeof target === "object" ? target.id : target;
    if (id !== undefined && id !== null) {
      native.svcSuppress(String(id));
    }
  },
  getSetting: (id, key) =>
    native.svcGetSetting(settingPart("getSetting(): id", id), settingPart("getSetting(): key", key)) ?? undefined,
  saveSetting: (id, key, value) => {
    const i = settingPart("saveSetting(): id", id);
    const k = settingPart("saveSetting(): key", key);
    const text = value === undefined || value === null ? undefined : typeof value === "string" ? value : stringify(value);
    native.svcSaveSetting(i, k, text);
  },
  removeSetting: (id, key) =>
    native.svcSaveSetting(settingPart("removeSetting(): id", id), settingPart("removeSetting(): key", key), undefined),
  broadcast: (payload) => native.netBroadcast(payloadBytes(payload)),
  resolve: (hostname, cb) => {
    const ips = native.netResolve(strArg(hostname));
    if (typeof cb === "function") {
      for (const ip of ips) {
        cb({ protocol: ip.includes(":") ? "IPV6" : "IPV4", ip });
      }
    }
  },
};

function publishControllers(force) {
  if (!serviceMode || (!controllersDirty && !force)) {
    return;
  }
  controllersDirty = false;
  const list = controllerWrappers.map((w) => ({
    id: String(w.id),
    data: plain(w.obj) ?? {},
    canPair: w.obj !== null && typeof w.obj === "object" && typeof w.obj.startLink === "function",
  }));
  native.svcControllers(stringify(list));
}

function discoveryCall(name, args) {
  const d = discoveryInstance;
  const f = d === undefined ? undefined : d[name];
  if (typeof f !== "function") {
    if (!missingWarned.has(name)) {
      missingWarned.add(name);
      hostLog(`${name}() not implemented!`);
    }
    return undefined;
  }
  return f.apply(d, args);
}

/** Deliver what arrived on the sockets and the finished asynchronous requests. */
function pumpNet() {
  if (!netNative) {
    return;
  }
  const events = parseJson(native.netPoll());
  for (const ev of events) {
    if (ev.kind === "http") {
      const x = pendingXhr.get(ev.socket);
      if (x !== undefined) {
        pendingXhr.delete(ev.socket);
        finishXhr(x, ev.http ?? {}, true);
      }
      continue;
    }
    if (ev.discovery && ev.kind === "message") {
      const value = { id: ev.address, ip: ev.address, port: ev.port, response: ev.text ?? "", buffer: ev.data ?? [] };
      callSafely(discoveryCall, undefined, ["Discovered", [value]], "Discovered");
      continue;
    }
    const entry = socketEntries.get(ev.socket);
    if (entry === undefined) {
      continue;
    }
    const { socket, handlers } = entry;
    switch (ev.kind) {
      case "message": {
        const text = ev.text ?? "";
        const msg = { data: text, utf16: text, buffer: ev.data ?? [], address: ev.address ?? "", port: ev.port ?? 0, timestamp: Date.now() };
        for (const cb of handlers.message) {
          callSafely(cb, socket, [msg], "socket 'message' callback");
        }
        break;
      }
      case "connection": {
        const args = ev.client === undefined ? [] : [makeSocket(true, ev.client)];
        for (const cb of handlers.connection) {
          callSafely(cb, socket, args, "socket 'connection' callback");
        }
        break;
      }
      case "error":
        for (const cb of handlers.error) {
          callSafely(cb, socket, [-1, ev.message ?? ""], "socket 'error' callback");
        }
        break;
      case "close":
        socketEntries.delete(ev.socket);
        for (const cb of handlers.close) {
          callSafely(cb, socket, [], "socket 'close' callback");
        }
        break;
      default:
        for (const cb of handlers.listening) {
          callSafely(cb, socket, [], "socket 'listening' callback");
        }
    }
  }
  publishControllers(false);
}

/** Service: `discovery = new DiscoveryService()`; returns what the host reads from it (JSON). */
function startService() {
  const ns = state.namespace;
  const Ctor = ns === undefined ? undefined : ns.DiscoveryService;
  if (typeof Ctor !== "function") {
    throw new TypeError("DiscoveryService is not exported as a function or class");
  }
  const inst = new Ctor();
  discoveryInstance = inst;
  globalThis.discovery = inst;
  try {
    nativeService.name = String(typeof ns.Name === "function" ? ns.Name() : (ns.Name ?? ""));
  } catch {
    // Name() was read by the catalog already; the service works without it.
  }
  const port = (v) => {
    const n = Number(v);
    return Number.isInteger(n) && n > 0 && n < 65536 ? n : 0;
  };
  const mdns = isArray(inst.MDns) ? inst.MDns.filter((t) => typeof t === "string") : [];
  if (inst.UdpBroadcastAddress !== undefined && typeof inst.UdpBroadcastAddress !== "string") {
    hostLog("UdpBroadcastAddress must be a string!");
  }
  return stringify({
    iconUrl: typeof inst.IconUrl === "string" ? inst.IconUrl : "",
    mdns,
    broadcastAddress: typeof inst.UdpBroadcastAddress === "string" ? inst.UdpBroadcastAddress : "255.255.255.255",
    broadcastPort: port(inst.UdpBroadcastPort),
    listenPort: port(inst.UdpListenPort),
    canForget: typeof inst.forgetController === "function",
  });
}

/** Service: call a `discovery` member (`{ name, args }` as JSON). */
function callDiscovery(json) {
  const { name, args } = parseJson(json);
  try {
    discoveryCall(String(name), isArray(args) ? args : []);
  } finally {
    publishControllers(name === "Update");
  }
}

/** Service, dashboard "Add by IP". */
function addByIp(ip) {
  try {
    if (discoveryInstance !== undefined && typeof discoveryInstance.forceDiscover === "function") {
      discoveryInstance.forceDiscover(ip);
    } else {
      discoveryCall("Discovered", [{ id: ip, ip, port: 0, response: "", manual: true }]);
    }
  } finally {
    publishControllers(true);
  }
}

function wrapperById(id) {
  return controllerWrappers.find((w) => String(w.id) === String(id));
}

/** Service, dashboard "Pair": the controller's `startLink()`. */
function pairController(id) {
  try {
    const w = wrapperById(id);
    if (w !== undefined && w.obj !== null && typeof w.obj.startLink === "function") {
      w.obj.startLink();
    } else {
      hostLog(`controller ${id} cannot be paired`);
    }
  } finally {
    publishControllers(true);
  }
}

/** Service, dashboard "Forget": `discovery.forgetController(id)`. */
function forgetController(id) {
  try {
    const w = wrapperById(id);
    discoveryCall("forgetController", [w === undefined ? id : w.id]);
  } finally {
    publishControllers(true);
  }
}

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

// `@SignalRGB/serial` (hid-usb.md §3) over the `serial*` natives. The Rust
// side only opens the ports of the device's own USB parent; here the options
// are checked and mapped. Settings the backends cannot do (1.5 stop bits,
// mark / space parity) make `connect` return false with a log line.
const serialNative = native !== undefined && typeof native.serialConnect === "function";
const STOP_BITS = { one: 1, two: 2 };
const PARITIES = { none: 0, odd: 1, even: 2 };
const MAX_SERIAL_WRITE = 1 << 20;

/** Bytes of a serial write: a byte array, typed array, ArrayBuffer or string (UTF-8). */
function serialBytes(data) {
  if (typeof data === "string") {
    return utf8(data);
  }
  if (data instanceof ArrayBuffer) {
    return arrayFrom(new Uint8Array(data, 0, Math.min(data.byteLength, MAX_SERIAL_WRITE)));
  }
  if (isView(data)) {
    return arrayFrom(new Uint8Array(data.buffer, data.byteOffset, Math.min(data.byteLength, MAX_SERIAL_WRITE)));
  }
  if (data != null && typeof data.length === "number") {
    const n = Math.min(data.length >>> 0, MAX_SERIAL_WRITE);
    const out = new Array(n);
    for (let i = 0; i < n; i++) {
      out[i] = Number(data[i]) & 0xff;
    }
    return out;
  }
  return [];
}

function serialOption(value, table, name) {
  const key = typeof value === "number" ? { 1: "one", 2: "two" }[value] : String(value).toLowerCase();
  if (key === undefined || !hasOwn(table, key)) {
    hostLog(`serial.connect: ${name} ${stringify(value)} is not supported by this host`);
    return undefined;
  }
  return table[key];
}

function serialJson(text) {
  return text === "" ? undefined : parseJson(text);
}

const serialModule = serialNative
  ? freeze({
      connect(options) {
        const o = options ?? {};
        const stop = serialOption(o.stopBits ?? "One", STOP_BITS, "stopBits");
        const parity = serialOption(o.parity ?? "None", PARITIES, "parity");
        if (stop === undefined || parity === undefined) {
          return false;
        }
        const port = o.portName === undefined || o.portName === null ? "" : String(o.portName);
        const baud = Number(o.baudRate ?? 115200);
        const dataBits = Number(o.dataBits ?? 8);
        return native.serialConnect(port, isFiniteNumber(baud) ? baud : 0, dataBits | 0, stop, parity);
      },
      disconnect: () => {
        native.serialDisconnect();
      },
      isConnected: () => native.serialIsConnected(),
      write: (data) => native.serialWrite(serialBytes(data)),
      read: (maxBytes, timeoutMs = 1000) => {
        const max = maxBytes === undefined || maxBytes === null ? -1 : Math.max(0, Number(maxBytes) | 0);
        return native.serialRead(max, Number(timeoutMs) || 0);
      },
      readAll: () => native.serialRead(-1, 0),
      availablePorts: () => parseJson(native.serialPorts()),
      getDeviceInfo: (portName) => serialJson(native.serialInfo(portName == null ? "" : String(portName))),
      getPortName: () => native.serialPortName(),
      getBaudRate: () => native.serialBaudRate(),
    })
  : undefined;

// `@SignalRGB/lcd` (effects-engine.md §4, docs/host-api/lcd-lightloom.md) over the
// `lcd*` natives (src/lcd.rs): frames of the device's LCD face, encoded in Rust. A
// native answers bytes as a Uint8Array (handed to the plugin as a plain array: plugins
// use `.length`, `.slice` and `concat`), or `{code, message}`, thrown as an `LcdError`.
const lcdNative = native !== undefined && typeof native.lcdGetFrame === "function";

class LcdError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "LcdError";
    this.code = code;
  }
}

function lcdBytes(r) {
  if (isView(r)) {
    return arrayFrom(r);
  }
  throw new LcdError(String(r?.code ?? "LCD_ERROR"), String(r?.message ?? "LCD call failed"));
}

function lcdFormat(o, member) {
  const f = o?.format;
  if (typeof f !== "string") {
    throw new LcdError(
      "INVALID_FORMAT",
      `LCD.${member}(): Invalid image format: ${stringify(f)}. Valid formats are: JPEG, PNG, BMP, RGBA, RGB, RGB565, NZXT::RGB565, NZXT::Q565`,
    );
  }
  return f;
}

function lcdQuality(o) {
  const q = Number(o?.quality);
  return isFiniteNumber(q) ? q : -1;
}

const lcdModule = lcdNative
  ? freeze({
      initialize(settings) {
        if (settings === null || typeof settings !== "object") {
          throw new LcdError("INVALID_SETTINGS", "Settings must be an object");
        }
        if (typeof settings.width !== "number") {
          throw new LcdError("INVALID_SETTINGS", "Width must be provided and must be a number");
        }
        if (typeof settings.height !== "number") {
          throw new LcdError("INVALID_SETTINGS", "Height must be provided and must be a number");
        }
        // Both spellings are accepted (hid-usb.md Gap resolution).
        const circular = settings.circular ?? settings.circularPreview;
        if (circular !== undefined && typeof circular !== "boolean") {
          throw new LcdError("INVALID_SETTINGS", "Circular must be a boolean");
        }
        const r = native.lcdInitialize(settings.width, settings.height, circular === true);
        if (r !== "") {
          throw new LcdError(String(r.code), String(r.message));
        }
      },
      get initialized() {
        return native.lcdInitialized();
      },
      getFrame(options) {
        return lcdBytes(native.lcdGetFrame(lcdFormat(options, "getFrame"), lcdQuality(options)));
      },
      getSlicedFrame(options) {
        const o = options ?? {};
        const columns = Number(o.columns ?? o.cols);
        const rows = Number(o.rows);
        const r = native.lcdGetSlicedFrame(columns, rows, lcdFormat(o, "getSlicedFrame"), lcdQuality(o));
        if (isArray(r)) {
          return r.map(lcdBytes);
        }
        return lcdBytes(r);
      },
      getImageBuffer(x, y, width, height, options) {
        const o = options ?? {};
        const out = (v) => (v === undefined || v === null ? NaN : Number(v));
        const format = o.format === undefined ? "RGB" : lcdFormat(o, "getImageBuffer");
        return lcdBytes(
          native.lcdGetImageBuffer(
            [Number(x), Number(y), Number(width), Number(height), out(o.outputWidth), out(o.outputHeight)],
            format,
          ),
        );
      },
    })
  : undefined;

const netModules = netNative ? { udp: udpModule, tcp: tcpModule } : {};
const serialModules = serialModule === undefined ? {} : { serial: serialModule };
const lcdModules = lcdModule === undefined ? {} : { lcd: lcdModule };
const nativeModules = deviceMode
  ? { permissions: permissionsModule, systeminfo: systeminfoModule, ...netModules, ...serialModules, ...lcdModules }
  : { ...netModules };

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
// A device session has the fixed-address bus when it drives an SMBus device;
// for a HID device every `bus.*` member throws.
const nativeBus = native !== undefined && typeof native.busInfo === "function";
if (deviceMode) {
  g.bus = nativeBus ? recording("bus", nativeBusMembers(true)) : recording("bus", {});
} else if (native !== undefined) {
  // Scan mode: "bus doesn't exist in the global scope" during Scan (smbus.md §1).
  g.bus = recording("bus", {});
} else {
  g.bus = recording("bus", busMembers(true));
}
g.service = recording("service", serviceMode && netNative ? nativeService : serviceTarget);
// Plain-data snapshot of the plugin's own controller object / the plugin's
// DiscoveryService instance: unknown fields are plugin data, not host API.
g.controller =
  typeof cfg.controller === "string" ? parseJson(cfg.controller) : recording("controller", controllerTarget, { data: true });
g.discovery = recording("discovery", {}, { data: true });
g.XmlHttp = recording("XmlHttp", xmlHttpTarget);
g.XMLHttpRequest = netNative ? NetXMLHttpRequest : XMLHttpRequestStub;
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
  "HidUsages",
  "SmbusAddresses",
  "SmbusBuses",
  "SmbusDeniedRegisters",
  "SpdManufacturers",
  "SmbusBatchWrites",
  "SaveCapabilities",
  "ShutdownModes",
];
const LIFECYCLE = ["Validate", "Initialize", "Render", "Shutdown", "Scan", "DiscoveryService", "SaveToDevice"];

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
      // `getProperty` / `removeProperty` / `getPropertyNames` see the exports too.
      state.properties.set(p.property, p);
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

/** Host-side `Scan(bus)` argument: the native free-address bus in scan mode, else a stub. */
function freeBus() {
  return recording("bus", nativeBus ? nativeBusMembers(false) : busMembers(false));
}

function report() {
  return stringify({
    gaps: fromEntries(state.gaps),
    fields: fromEntries(state.fields),
    calls: fromEntries(state.calls),
    consoleLines: state.consoleLines,
    logTail: state.logTail,
    ...(state.battery === undefined
      ? {}
      : { battery: { level: state.battery.level < 0 ? null : state.battery.level, state: state.battery.state } }),
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
    pumpNet,
    startService,
    callDiscovery,
    addByIp,
    pairController,
    forgetController,
    describeError,
    modules: freeze(modules),
    nativeModules: freeze(nativeModules),
  }),
  writable: false,
  enumerable: false,
  configurable: false,
});
