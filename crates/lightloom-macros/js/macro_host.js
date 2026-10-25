// SPDX-License-Identifier: GPL-3.0-or-later
//
// Host of one macro script (crates/lightloom-macros/src/script.rs): the
// globals SignalRGB-shaped macro scripts use (docs/host-api/macros-system.md
// §1.4): `system`, `signalrgb`, `keys`, `mouse`, `block`, `console`. Every
// member calls a Rust native that answers JSON `{ok: value}` or
// `{err: [class, message]}`; errors are thrown as `TypeError`, `MacroRefused`
// (a call the policy refuses) or `MacroError`. There is no `window`,
// `setTimeout` or `Image`, as in SignalRGB's macro engine.
//
// `__macroHost` (non-writable) is what the Rust side calls: `load(namespace)`
// instantiates the script's `Action()` / `Input()` constructor, `setOptions`
// writes option values onto the instance, `execute()` and `handle(handler,
// event)` run it, `answer(id, text)` completes a `system.prompt`.
"use strict";
(() => {
  const native = globalThis.__macroNative;
  delete globalThis.__macroNative;
  const { freeze, defineProperty, keys: ownKeys } = Object;
  const { stringify, parse } = JSON;
  const isArray = Array.isArray;

  class MacroRefused extends Error {
    constructor(message) {
      super(message);
      this.name = "MacroRefused";
    }
  }
  class MacroError extends Error {
    constructor(message) {
      super(message);
      this.name = "MacroError";
    }
  }
  const classes = { MacroRefused, MacroError, TypeError, RangeError };

  /** A native's answer, or its error thrown. */
  const unwrap = (text) => {
    const r = parse(text);
    if (r.err) {
      const C = classes[r.err[0]] || MacroError;
      throw new C(r.err[1]);
    }
    return r.ok === null ? undefined : r.ok;
  };
  const json = (v) => {
    try {
      const s = stringify(v);
      return s === undefined ? "null" : s;
    } catch {
      return "null";
    }
  };
  const text = (v) => {
    if (typeof v === "string") return v;
    if (v instanceof Error) return `${v.name}: ${v.message}`;
    const s = json(v);
    return s === "null" && v !== null ? String(v) : s;
  };
  const line = (level) => (...args) => native.log(level, args.map(text).join(" "));

  const console = freeze({
    log: line("log"),
    info: line("info"),
    warn: line("warn"),
    error: line("error"),
    trace: line("trace"),
    debug: line("debug"),
  });

  const prompts = new Map();
  const field = (o, k) => (o[k] === undefined || o[k] === null ? "" : String(o[k]));

  const system = freeze({
    action: (path, _args) => {
      if (typeof path !== "string") {
        throw new TypeError("action(): Action parameter must be a string!");
      }
      unwrap(native.action(path));
    },
    openUrl: (url) => {
      if (typeof url !== "string") {
        throw new TypeError("openUrl(): Parameter must be a string!");
      }
      unwrap(native.openUrl(url));
    },
    openApplication: (path, args, startIn) => {
      if (typeof path !== "string") {
        throw new TypeError("openApplication(): Parameter must be a string!");
      }
      const list = isArray(args) ? args.map(String) : args === undefined || args === null ? [] : String(args);
      unwrap(native.openApplication(path, json(list), startIn === undefined || startIn === null ? "" : String(startIn)));
    },
    prompt: (options, callback) => {
      if (typeof callback !== "function") {
        throw new TypeError("prompt(): Second parameter must be a callback function!");
      }
      const o = options !== null && typeof options === "object" ? options : {};
      const id = unwrap(
        native.prompt(
          json({
            title: field(o, "title"),
            message: field(o, "message"),
            placeholder: field(o, "placeholder"),
            confirmText: field(o, "confirmText"),
            cancelText: field(o, "cancelText"),
          }),
        ),
      );
      if (typeof id !== "number" || id < 0) {
        // Nobody can answer (no dashboard open): cancelled.
        Promise.resolve().then(() => callback(null));
        return;
      }
      prompts.set(id, callback);
    },
    copyToClipboard: (value) => {
      if (typeof value !== "string") {
        throw new TypeError("copyToClipboard(): Parameter must be a string!");
      }
      unwrap(native.clipboard(value));
    },
    elevateWindow: (term) => {
      if (typeof term !== "string" || term === "") {
        throw new TypeError("elevateWindow(): Parameter must be a non-empty string!");
      }
      unwrap(native.unsupported("system.elevateWindow"));
    },
    cycleWindow: (term) => {
      if (typeof term !== "string" || term === "") {
        throw new TypeError("cycleWindow(): Parameter must be a non-empty string!");
      }
      unwrap(native.unsupported("system.cycleWindow"));
    },
    // No Explorer integration: "" is SignalRGB's answer when no Explorer window has the focus.
    getForegroundExplorerPath: () => "",
  });

  const SIGNALRGB = [
    "CurrentEffectName",
    "PreviousEffectName",
    "NextEffectName",
    "LastEffectName",
    "SelectNextEffect",
    "SelectPreviousEffect",
    "ApplyEffectPreset",
    "StartPlaylist",
    "StopPlaylist",
    "EffectShuffle",
    "SetEffectShuffle",
    "GetGlobalBrightness",
    "SetGlobalBrightness",
    "IncrementGlobalBrightness",
    "GetCanvasPauseState",
    "SetCanvasPauseState",
    "GetDeviceEnabled",
    "SetDeviceEnabled",
  ];
  const signalrgb = {};
  for (const name of SIGNALRGB) {
    signalrgb[name] = (...args) => unwrap(native.signalrgb(name, json(args)));
  }
  freeze(signalrgb);

  const keyArg = (k) => {
    if (typeof k !== "string" && typeof k !== "number") {
      throw new TypeError("(): Key Parameter must be a key name, or a virtual key code!");
    }
    return json(k);
  };
  const keys = freeze({
    press: (k) => unwrap(native.keys("press", keyArg(k))),
    hold: (k) => unwrap(native.keys("hold", keyArg(k))),
    release: (k) => unwrap(native.keys("release", keyArg(k))),
    paste: (t) => unwrap(native.keys("paste", json(String(t)))),
    sendUnicode: (t) => unwrap(native.keys("unicode", json(String(t)))),
  });

  const mouse = freeze({
    click: (b) => {
      if (typeof b !== "string" && typeof b !== "number") {
        throw new TypeError("click(): Button Parameter must be a button name or virtual button code!");
      }
      unwrap(native.click(json(b)));
    },
  });

  const block = freeze({
    trigger: () => unwrap(native.trigger()),
  });

  let instance = null;
  let kind = "";
  const HANDLERS = ["onExecute", "onApplication", "onKey", "onMouse", "onSensorUpdate"];
  const str = (v) => (typeof v === "string" ? v : undefined);

  const host = freeze({
    load(ns) {
      let ctor = null;
      if (typeof ns.Action === "function") {
        kind = "action";
        ctor = ns.Action;
      } else if (typeof ns.Input === "function") {
        kind = "input";
        ctor = ns.Input;
      } else {
        throw new MacroError("the script exports neither Action() nor Input()");
      }
      instance = new ctor();
      return host.describe();
    },
    describe() {
      let options = null;
      try {
        options = instance.Options === undefined ? null : parse(json(instance.Options));
      } catch {
        options = null;
      }
      return json({
        kind,
        name: str(instance.Name),
        description: str(instance.Description),
        options,
        handlers: HANDLERS.filter((h) => typeof instance[h] === "function"),
      });
    },
    setOptions(values) {
      const v = parse(values);
      for (const k of ownKeys(v)) {
        instance[k] = v[k];
      }
    },
    execute() {
      instance.onExecute();
    },
    handle(handler, event) {
      const f = instance[handler];
      if (typeof f !== "function") {
        return 0;
      }
      const e = parse(event);
      // Scripts read both spellings (§1.3): one object under both names.
      if (e.data !== undefined) {
        e.Data = e.data;
      }
      return f.call(instance, e) === true ? 1 : 0;
    },
    answer(id, value) {
      const cb = prompts.get(id);
      if (cb === undefined) {
        return;
      }
      prompts.delete(id);
      cb(value === null ? null : String(value));
    },
  });

  const constant = (name, value) =>
    defineProperty(globalThis, name, { value, writable: false, enumerable: false, configurable: false });
  constant("__macroHost", host);
  constant("system", system);
  constant("signalrgb", signalrgb);
  constant("keys", keys);
  constant("mouse", mouse);
  constant("block", block);
  constant("console", console);
})();
