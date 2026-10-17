// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// HostApiMissing: the typed error every Lightloom prelude facade throws when a
// plugin reaches a host member that this build does not implement yet
// (architecture-synthesis.md D6: "log API_GAP once, then throw a typed error").
//
// The host (crates/lightloom-plugin-host) is expected to set
//   globalThis.__lightloom = { plugin: "<plugin file or Name()>", log(level, msg) }
// before evaluating the plugin. Everything here degrades gracefully when it is
// absent (plain Node, the SDK simulator, unit tests).
//
// This file has no imports. prelude/modules/*.js import it; nothing else does.

const seen = new Set();
let logger = null;

/** Returns the plugin name the host registered for this context, if any. */
export function currentPluginName() {
  const h = globalThis.__lightloom;
  if (h && typeof h.plugin === "string" && h.plugin.length > 0) {
    return h.plugin;
  }
  return undefined;
}

/**
 * Error raised for an unimplemented host member.
 * Fields: name "HostApiMissing", code "API_GAP", module, member, plugin, hint.
 */
export class HostApiMissing extends Error {
  constructor(member, { plugin, module, hint } = {}) {
    const resolvedPlugin = plugin === undefined ? currentPluginName() : plugin;
    const qualified = module ? `${module}.${member}` : String(member);
    let text = `API_GAP: ${qualified} is not implemented by this host`;
    if (resolvedPlugin) {
      text += ` (plugin: ${resolvedPlugin})`;
    }
    if (hint) {
      text += ` - ${hint}`;
    }
    super(text);
    this.name = "HostApiMissing";
    this.code = "API_GAP";
    this.member = String(member);
    this.module = module === undefined ? undefined : String(module);
    this.plugin = resolvedPlugin;
    this.hint = hint;
  }

  /** Stable key used by the log-once helper. */
  get gapKey() {
    return `${this.plugin ?? "?"}:${this.module ?? ""}:${this.member}`;
  }
}

/**
 * Replace the sink used by logOnce (tests, or a host that prefers its own
 * channel). Pass null to restore automatic detection.
 */
export function setLogger(fn) {
  logger = typeof fn === "function" ? fn : null;
}

function sink() {
  if (logger) {
    return logger;
  }
  const h = globalThis.__lightloom;
  if (h && typeof h.log === "function") {
    return (msg) => h.log("warn", msg);
  }
  const dev = globalThis.device;
  if (dev && typeof dev.log === "function") {
    return (msg) => dev.log(msg);
  }
  const svc = globalThis.service;
  if (svc && typeof svc.log === "function") {
    return (msg) => svc.log(msg);
  }
  const c = globalThis.console;
  if (c && typeof c.warn === "function") {
    return (msg) => c.warn(msg);
  }
  return () => {};
}

/**
 * Logs `message` the first time `key` is seen in this context; later calls are
 * no-ops. Returns true when the message was emitted.
 */
export function logOnce(key, message) {
  const k = String(key);
  if (seen.has(k)) {
    return false;
  }
  seen.add(k);
  try {
    sink()(message);
  } catch {
    // Logging must never take the plugin down.
  }
  return true;
}

/** Forget every log-once key (test helper). */
export function resetLogOnce() {
  seen.clear();
}

/** Number of distinct gaps logged so far (diagnostics). */
export function loggedGapCount() {
  return seen.size;
}

/**
 * Build, log (once per plugin/module/member) and return a HostApiMissing.
 * Callers `throw missing(...)`.
 */
export function missing(module, member, extra = {}) {
  const err = new HostApiMissing(member, { module, ...extra });
  logOnce(err.gapKey, err.message);
  return err;
}

/** Same as missing() but throws immediately. */
export function throwMissing(module, member, extra = {}) {
  throw missing(module, member, extra);
}

const PASSTHROUGH_KEYS = new Set([
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
  "__lightloomFacade",
]);

/**
 * Create a facade object for a Rust-backed `@SignalRGB/<name>` module.
 *
 * Every member in `members` becomes a function that throws HostApiMissing
 * naming the member. Any other string property is also answered with such a
 * function (or, with `accessThrows: true`, the access itself throws), so an
 * API gap can never pass silently. A few keys are left undefined so that
 * Promise/JSON/console probing keeps working.
 */
export function createFacade(moduleName, members = [], { accessThrows = false, hint } = {}) {
  // Ordinary prototype so String()/template literals/console printing work
  // through Object.prototype.toString + Symbol.toStringTag.
  const base = {};
  const makeThrower = (member) => {
    const fn = function lightloomFacadeMember() {
      throw missing(moduleName, member, { hint });
    };
    Object.defineProperty(fn, "name", { value: `${moduleName}.${member}` });
    fn.__lightloomFacadeMember = member;
    return fn;
  };
  for (const member of members) {
    base[member] = makeThrower(member);
  }
  base.__lightloomFacade = moduleName;
  base.__lightloomMembers = Object.freeze([...members]);
  Object.defineProperty(base, Symbol.toStringTag, { value: `HostApiFacade(${moduleName})` });

  return new Proxy(base, {
    get(target, key, receiver) {
      if (typeof key !== "string" || PASSTHROUGH_KEYS.has(key)) {
        return Reflect.get(target, key, receiver);
      }
      if (key in target) {
        return Reflect.get(target, key, receiver);
      }
      if (accessThrows) {
        throw missing(moduleName, key, { hint });
      }
      return makeThrower(key);
    },
    has(target, key) {
      return typeof key === "string" && !PASSTHROUGH_KEYS.has(key) ? true : Reflect.has(target, key);
    },
    set(target, key, value) {
      // Plugins occasionally monkey-patch module objects; allow it so the
      // override takes effect instead of silently vanishing.
      target[key] = value;
      return true;
    },
  });
}

export default HostApiMissing;
