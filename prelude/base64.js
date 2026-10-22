// SPDX-License-Identifier: GPL-3.0-or-later
//
// "@SignalRGB/base64" (also "@huedeck/base64") and the `base64` feature global.
//
// Corpus survey (2026-10-17, 1006 plugin .js files):
//   import {encode, decode} from "@SignalRGB/base64"   1 file (Twinkly addon)
//     encode(Array.from({length: 32}, () => randomByte))   2 calls (service context)
//   device.addFeature("base64")                         2 files (Govee, Twinkly)
//     base64.Encode(packet /* number[] */)              2 calls -> string
//     base64.Decode(token  /* string   */)              1 call  -> number[]
// Exe facts: module class PluginBase64 with method `encode` taking a second
// boolean parameter named `abUrlEncoding`; the module exports are lowercase,
// the feature global members are capitalised. Both surfaces must exist.
//
// Behaviour (own implementation):
//   encode(bytes, urlSafe = false) -> string
//     `bytes` may be a number[] (each value masked to 0..255), a Uint8Array /
//     ArrayBuffer / any array-like, or a string (encoded as UTF-8).
//     urlSafe swaps "+/" for "-_" and keeps the "=" padding (the Qt
//     Base64UrlEncoding default the original host is built on).
//   decode(text) -> number[]
//     Accepts both alphabets, with or without padding, ignores whitespace and
//     any character outside the alphabet (lenient, like QByteArray::fromBase64)
//     and stops at the first "=". Returns a plain Array so plugins can .concat().
//
// Dependency-free: no TextEncoder/Buffer/atob required (QuickJS-safe).

const STD = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const PAD = 61; // "="

const LOOKUP = new Int16Array(256).fill(-1);
for (let i = 0; i < 64; i++) {
  LOOKUP[STD.charCodeAt(i)] = i;
  LOOKUP[URL.charCodeAt(i)] = i;
}

function utf8Bytes(text) {
  const out = [];
  for (let i = 0; i < text.length; i++) {
    const cp = text.codePointAt(i);
    if (cp > 0xffff) {
      i++; // surrogate pair consumed
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

function toByteList(input) {
  if (input == null) {
    return [];
  }
  if (typeof input === "string") {
    return utf8Bytes(input);
  }
  if (input instanceof ArrayBuffer) {
    return Array.from(new Uint8Array(input));
  }
  if (ArrayBuffer.isView(input)) {
    return Array.from(new Uint8Array(input.buffer, input.byteOffset, input.byteLength));
  }
  if (typeof input.length === "number") {
    const out = new Array(input.length);
    for (let i = 0; i < input.length; i++) {
      const v = input[i];
      out[i] = (typeof v === "number" ? v : Number(v) || 0) & 0xff;
    }
    return out;
  }
  if (typeof input[Symbol.iterator] === "function") {
    return toByteList(Array.from(input));
  }
  throw new TypeError("base64.encode: expected a byte array, typed array, ArrayBuffer or string");
}

/** Encode bytes (or a UTF-8 string) to base64. */
export function encode(bytes, urlSafe = false) {
  const data = toByteList(bytes);
  const alphabet = urlSafe ? URL : STD;
  let out = "";
  let i = 0;
  for (; i + 2 < data.length; i += 3) {
    const n = (data[i] << 16) | (data[i + 1] << 8) | data[i + 2];
    out += alphabet[(n >> 18) & 63] + alphabet[(n >> 12) & 63] + alphabet[(n >> 6) & 63] + alphabet[n & 63];
  }
  const rest = data.length - i;
  if (rest === 1) {
    const n = data[i] << 16;
    out += alphabet[(n >> 18) & 63] + alphabet[(n >> 12) & 63] + "==";
  } else if (rest === 2) {
    const n = (data[i] << 16) | (data[i + 1] << 8);
    out += alphabet[(n >> 18) & 63] + alphabet[(n >> 12) & 63] + alphabet[(n >> 6) & 63] + "=";
  }
  return out;
}

/** Decode a base64 string (either alphabet, padding optional) to a number[]. */
export function decode(text) {
  if (text == null) {
    return [];
  }
  const s = String(text);
  const out = [];
  let acc = 0;
  let bits = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === PAD) {
      break;
    }
    const v = c < 256 ? LOOKUP[c] : -1;
    if (v < 0) {
      continue; // whitespace or foreign character: skip
    }
    acc = (acc << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push((acc >> bits) & 0xff);
      acc &= (1 << bits) - 1;
    }
  }
  return out;
}

/** Decode to a UTF-8 string (convenience for token/JSON payloads). */
export function decodeToString(text) {
  const bytes = decode(text);
  let s = "";
  for (let i = 0; i < bytes.length; ) {
    const b0 = bytes[i++];
    let cp;
    if (b0 < 0x80) {
      cp = b0;
    } else if ((b0 & 0xe0) === 0xc0) {
      cp = ((b0 & 0x1f) << 6) | (bytes[i++] & 0x3f);
    } else if ((b0 & 0xf0) === 0xe0) {
      cp = ((b0 & 0x0f) << 12) | ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f);
    } else {
      cp = ((b0 & 0x07) << 18) | ((bytes[i++] & 0x3f) << 12) | ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f);
    }
    s += String.fromCodePoint(Number.isFinite(cp) && cp <= 0x10ffff ? cp : 0xfffd);
  }
  return s;
}

// Feature-global shape: after device.addFeature("base64") the host installs
// globalThis.base64 = createFeatureGlobal().
export const Encode = encode;
export const Decode = decode;

/** Object the host binds to `globalThis.base64` for addFeature("base64"). */
export function createFeatureGlobal() {
  return Object.freeze({ Encode: encode, Decode: decode, encode, decode });
}

export default { encode, decode, Encode, Decode, decodeToString, createFeatureGlobal };
