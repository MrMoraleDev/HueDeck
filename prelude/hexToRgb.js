// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// hexToRgb / rgbToHex helpers for first-party plugins and the SDK.
//
// Corpus survey (2026-10-17, 1006 plugin .js files): the identifier hexToRgb
// appears 2868 times and is DEFINED locally in 871 files (regex
// /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i then parseInt of each pair; one
// variant of 4 files returns [0,0,0] on no match, the common one would throw
// on a missing match). Zero files use it without defining it, so the original
// host provides no global; Lightloom likewise does not inject one. rgbToHex is
// defined in 3 files. ControllableParameters of type "color" deliver
// "#rrggbb" strings, which is the only input shape plugins pass.
//
// Behaviour (own implementation):
//   hexToRgb("#1A2b3C") -> [26, 43, 60]; "1a2b3c" also accepted; "#abc"
//   shorthand expands to [170, 187, 204]; anything else -> [0, 0, 0]
//   (never throws, matching the defensive corpus variant).
//   rgbToHex(r, g, b) or rgbToHex([r, g, b]) -> "#rrggbb" lowercase, each
//   channel clamped to 0..255 and rounded.

const HEX6 = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i;
const HEX3 = /^#?([0-9a-f])([0-9a-f])([0-9a-f])$/i;

/** True when `value` is a 6-digit (or 3-digit shorthand) hex colour string. */
export function isHexColor(value) {
  if (typeof value !== "string") {
    return false;
  }
  const s = value.trim();
  return HEX6.test(s) || HEX3.test(s);
}

/** Clamp and round a channel value to an integer 0..255 (NaN -> 0). */
export function clampByte(n) {
  const v = Math.round(Number(n));
  if (!Number.isFinite(v)) {
    return 0;
  }
  return v < 0 ? 0 : v > 255 ? 255 : v;
}

/** "#rrggbb" | "rrggbb" | "#rgb" -> [r, g, b]; invalid input -> [0, 0, 0]. */
export function hexToRgb(hex) {
  if (typeof hex !== "string") {
    return [0, 0, 0];
  }
  const s = hex.trim();
  let m = HEX6.exec(s);
  if (m) {
    return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)];
  }
  m = HEX3.exec(s);
  if (m) {
    return [parseInt(m[1] + m[1], 16), parseInt(m[2] + m[2], 16), parseInt(m[3] + m[3], 16)];
  }
  return [0, 0, 0];
}

/** [r, g, b] | {r, g, b} | (r, g, b) -> "#rrggbb" (lowercase). */
export function rgbToHex(r, g, b) {
  let rr = r;
  let gg = g;
  let bb = b;
  if (Array.isArray(r) || ArrayBuffer.isView(r)) {
    [rr, gg, bb] = r;
  } else if (r && typeof r === "object") {
    rr = r.r;
    gg = r.g;
    bb = r.b;
  }
  const two = (n) => clampByte(n).toString(16).padStart(2, "0");
  return `#${two(rr)}${two(gg)}${two(bb)}`;
}

/** Any accepted hex form -> canonical "#rrggbb"; invalid -> "#000000". */
export function normalizeHex(hex) {
  return rgbToHex(hexToRgb(hex));
}

export default hexToRgb;
