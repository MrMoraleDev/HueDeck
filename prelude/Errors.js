// SPDX-License-Identifier: GPL-3.0-or-later
//
// "@SignalRGB/Errors.js" (also resolvable as "@SignalRGB/Errors" and
// "@huedeck/Errors"): Assert helpers, ContextError and globalContext.
//
// Corpus survey (2026-10-17, 1006 plugin .js files). Import forms:
//   import { Assert } from "@SignalRGB/Errors.js"                            35
//   import {ContextError, globalContext, Assert} from "@SignalRGB/Errors.js"  6
//   import { Assert, globalContext } from "@SignalRGB/Errors.js"              3
//   import { Assert } from "@SignalRGB/Errors"   (no extension)               3
// Member call counts:
//   Assert.softIsDefined(value, message)                   29  (never throws)
//   Assert.isOk(value, message[, context])                 18  e.g. isOk(channel, "Channel not found", {id, ChannelArray})
//   Assert.fail(message)                                   14
//   Assert.unreachable(message[, context])                  4  e.g. unreachable("Invalid Led Count", {leds: count})
//   Assert.isEqual(a, b, message[, context])                2  e.g. isEqual(positions.length, leds.length, "...", this.Config)
//   globalContext.set("Motherboard Config", configuration)  3  (Gigabyte motherboard controllers)
//   new ContextError(...)                                   0  (imported 6x, never constructed by plugins)
// No plugin inspects the return value of any Assert member.
//
// Behaviour (own design, consistent with every call shape above and with
// docs/host-api/hid-usb.md "Errors.js" resolution):
//   * isOk / isEqual / unreachable / fail throw a ContextError whose `context`
//     merges the call-site context over a snapshot of globalContext. A thrown
//     error aborts the current Initialize()/Render() and the host reports it
//     in the plugin console.
//   * softIsDefined only logs (device.log / service.log / console.warn) when
//     the value is null or undefined, and returns a boolean.
//   * globalContext is per JS context (one per plugin thread), never shared.
//
// Dependency-free on purpose.

function logWarning(message) {
  const h = globalThis.__huedeck;
  if (h && typeof h.log === "function") {
    h.log("warn", message);
    return;
  }
  const dev = globalThis.device;
  if (dev && typeof dev.log === "function") {
    dev.log(message);
    return;
  }
  const svc = globalThis.service;
  if (svc && typeof svc.log === "function") {
    svc.log(message);
    return;
  }
  const c = globalThis.console;
  if (c && typeof c.warn === "function") {
    c.warn(message);
  }
}

function safeStringify(value) {
  try {
    const seen = new WeakSet();
    return JSON.stringify(value, (key, v) => {
      if (typeof v === "bigint") {
        return `${v}n`;
      }
      if (typeof v === "function") {
        return `[Function ${v.name || "anonymous"}]`;
      }
      if (v && typeof v === "object") {
        if (seen.has(v)) {
          return "[Circular]";
        }
        seen.add(v);
      }
      return v;
    });
  } catch {
    return String(value);
  }
}

const store = new Map();

/** Per-context key/value bag whose snapshot is attached to every ContextError. */
export const globalContext = Object.freeze({
  set(key, value) {
    store.set(String(key), value);
    return globalContext;
  },
  get(key) {
    return store.get(String(key));
  },
  has(key) {
    return store.has(String(key));
  },
  delete(key) {
    return store.delete(String(key));
  },
  clear() {
    store.clear();
  },
  keys() {
    return [...store.keys()];
  },
  /** Plain object copy of the current entries. */
  snapshot() {
    const out = {};
    for (const [k, v] of store) {
      out[k] = v;
    }
    return out;
  },
});

function mergeContext(context, extra) {
  const merged = globalContext.snapshot();
  if (extra) {
    Object.assign(merged, extra);
  }
  if (context !== null && typeof context === "object") {
    Object.assign(merged, context);
  } else if (context !== undefined) {
    merged.value = context;
  }
  return merged;
}

/** Error subclass carrying a `context` object (call-site context over globalContext). */
export class ContextError extends Error {
  constructor(message, context = undefined) {
    super(message === undefined ? "ContextError" : String(message));
    this.name = "ContextError";
    this.context = mergeContext(context);
  }

  /** Message followed by the serialized context when it is not empty. */
  toString() {
    const keys = Object.keys(this.context);
    if (keys.length === 0) {
      return `${this.name}: ${this.message}`;
    }
    return `${this.name}: ${this.message} ${safeStringify(this.context)}`;
  }
}

function raise(message, context, extra) {
  const err = new ContextError(message);
  err.context = mergeContext(context, extra);
  throw err;
}

export const Assert = Object.freeze({
  /** Throws ContextError when `value` is falsy. Returns `value` otherwise. */
  isOk(value, message = "Assertion failed: expected a truthy value", context) {
    if (!value) {
      raise(message, context);
    }
    return value;
  },

  /** Throws ContextError when `actual !== expected` (strict equality). */
  isEqual(actual, expected, message = "Assertion failed: values are not equal", context) {
    if (actual !== expected) {
      raise(message, context, { actual, expected });
    }
    return true;
  },

  /** Always throws; marks a code path the author considers impossible. */
  unreachable(message = "Unreachable code reached", context) {
    raise(message, context);
  },

  /** Always throws. */
  fail(message = "Assertion failed", context) {
    raise(message, context);
  },

  /**
   * Logs a warning (never throws) when `value` is null or undefined.
   * Returns true when the value is defined.
   */
  softIsDefined(value, message = "Expected a defined value", context) {
    if (value === undefined || value === null) {
      let text = `Assert.softIsDefined: ${message}`;
      if (context !== undefined) {
        text += ` ${safeStringify(context)}`;
      }
      logWarning(text);
      return false;
    }
    return true;
  },
});

export default { Assert, ContextError, globalContext };
