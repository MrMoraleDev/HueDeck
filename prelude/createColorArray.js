// SPDX-License-Identifier: GPL-3.0-or-later
//
// device.createColorArray(color, count, layout[, colorOrder]) reference
// implementation. The Rust host may bind this function directly onto the
// `device` object (it is pure) or reimplement it natively; the SDK simulator
// and conformance tests use this file as the oracle.
//
// Corpus survey (2026-10-17, 1006 plugin .js files; spec total 447 call
// sites in 136 files). Call shapes:
//   createColorArray(c, ChannelLedCount, "Inline")                 ~200
//   createColorArray(c, n, "Inline", "GRB")                          86
//   createColorArray(c, n, "Inline", "RBG")                          52
//   createColorArray(c, n, "Inline", RGBconfig | actualRGBOrder)     26  (user combobox: RGB RBG BGR BRG GBR GRB)
//   createColorArray(c, n, "Separate")                               36
//   createColorArray(c, n, "Seperate")                                8  (docs' spelling, must be accepted)
//   createColorArray(c, n, "Inline", "RGB")                           6
//   createColorArray(c, n, "Inline", "BGR")                           6
//   createColorArray("#000000", n, "Inline", order)                   2
//   createColorArray(c, 1, "Inline")                                 20
// `c` is always a "#RRGGBB" string (ControllableParameters type "color" or
// device.getChannelPulseColor()). Results are consumed as:
//   Inline   -> flat number[] of length count*3 (plugins do .length/3, splice(0, n*3))
//   Separate -> [plane0[], plane1[], plane2[]] (plugins index ColorData[0..2][i])
// Exe ComponentChannel table lists the closed order set RGB|RBG|BGR|BRG|GBR|GRB.
//
// Behaviour (own implementation):
//   * invalid/missing hex -> treated as "#000000" (never throws);
//     [r,g,b] arrays are also accepted as `color` for SDK convenience.
//   * count is coerced to a non-negative integer (NaN/negative -> 0).
//   * layout is matched case-insensitively; "Separate" and "Seperate" are
//     synonyms; anything else falls back to "Inline".
//   * colorOrder is matched case-insensitively against the closed set;
//     anything else falls back to "RGB". The order permutes the channel
//     positions: order "GRB" yields [g, r, b, g, r, b, ...] for Inline and
//     [[g...], [r...], [b...]] for Separate.
//
// Dependency-free (hex parsing is inlined rather than imported from
// hexToRgb.js so the host can load this file on its own).

export const LAYOUTS = Object.freeze(["Inline", "Separate", "Seperate"]);
export const COLOR_ORDERS = Object.freeze(["RGB", "RBG", "BGR", "BRG", "GBR", "GRB"]);

const HEX6 = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i;
const CHANNEL_INDEX = { R: 0, G: 1, B: 2 };

function clampByte(n) {
  const v = Math.round(Number(n));
  return Number.isFinite(v) ? (v < 0 ? 0 : v > 255 ? 255 : v) : 0;
}

function parseColor(color) {
  if (typeof color === "string") {
    const m = HEX6.exec(color.trim());
    if (m) {
      return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)];
    }
    return [0, 0, 0];
  }
  if (color && typeof color.length === "number" && color.length >= 3) {
    return [clampByte(color[0]), clampByte(color[1]), clampByte(color[2])];
  }
  return [0, 0, 0];
}

/** Canonical order string from any casing; unknown -> "RGB". */
export function normalizeColorOrder(order) {
  if (typeof order !== "string") {
    return "RGB";
  }
  const up = order.trim().toUpperCase();
  return COLOR_ORDERS.includes(up) ? up : "RGB";
}

/** Canonical layout from any casing/spelling; unknown -> "Inline". */
export function normalizeLayout(layout) {
  if (typeof layout !== "string") {
    return "Inline";
  }
  const low = layout.trim().toLowerCase();
  if (low === "separate" || low === "seperate") {
    return "Separate";
  }
  return "Inline";
}

/** Permute an [r, g, b] triple (or hex string) into the requested channel order. */
export function reorderColor(rgb, order = "RGB") {
  const o = normalizeColorOrder(order);
  const src = parseColor(rgb);
  return [src[CHANNEL_INDEX[o[0]]], src[CHANNEL_INDEX[o[1]]], src[CHANNEL_INDEX[o[2]]]];
}

function toCount(count) {
  const n = Math.floor(Number(count));
  return Number.isFinite(n) && n > 0 ? n : 0;
}

/**
 * Build `count` copies of `color` in the requested layout and channel order.
 * Inline -> number[] (count*3); Separate -> [number[], number[], number[]].
 */
export function createColorArray(color, count, layout = "Inline", colorOrder = "RGB") {
  const n = toCount(count);
  const [c0, c1, c2] = reorderColor(color, colorOrder);
  if (normalizeLayout(layout) === "Separate") {
    return [new Array(n).fill(c0), new Array(n).fill(c1), new Array(n).fill(c2)];
  }
  const out = new Array(n * 3);
  for (let i = 0; i < n; i++) {
    const k = i * 3;
    out[k] = c0;
    out[k + 1] = c1;
    out[k + 2] = c2;
  }
  return out;
}

/**
 * Lay out an existing list of [r,g,b] triples (e.g. sampled LED colours) the
 * same way channel.getColors(layout, order) does. Used by the SDK simulator.
 */
export function layoutColors(triples, layout = "Inline", colorOrder = "RGB") {
  const list = Array.isArray(triples) ? triples : [];
  const ordered = list.map((t) => reorderColor(t, colorOrder));
  if (normalizeLayout(layout) === "Separate") {
    return [ordered.map((t) => t[0]), ordered.map((t) => t[1]), ordered.map((t) => t[2])];
  }
  const out = new Array(ordered.length * 3);
  for (let i = 0; i < ordered.length; i++) {
    out[i * 3] = ordered[i][0];
    out[i * 3 + 1] = ordered[i][1];
    out[i * 3 + 2] = ordered[i][2];
  }
  return out;
}

/**
 * channel.getColors(layout, order) over a flat [r, g, b, r, g, b, ...] stream
 * (what the host samples): a trailing partial triple is dropped and every
 * value is clamped to a byte. Always returns fresh arrays.
 */
export function layoutFlatColors(flat, layout = "Inline", colorOrder = "RGB") {
  const src = flat != null && typeof flat.length === "number" ? flat : [];
  const n = Math.floor(src.length / 3);
  const o = normalizeColorOrder(colorOrder);
  const i0 = CHANNEL_INDEX[o[0]];
  const i1 = CHANNEL_INDEX[o[1]];
  const i2 = CHANNEL_INDEX[o[2]];
  if (normalizeLayout(layout) === "Separate") {
    const p0 = new Array(n);
    const p1 = new Array(n);
    const p2 = new Array(n);
    for (let i = 0; i < n; i++) {
      const k = i * 3;
      p0[i] = clampByte(src[k + i0]);
      p1[i] = clampByte(src[k + i1]);
      p2[i] = clampByte(src[k + i2]);
    }
    return [p0, p1, p2];
  }
  const out = new Array(n * 3);
  for (let i = 0; i < n; i++) {
    const k = i * 3;
    out[k] = clampByte(src[k + i0]);
    out[k + 1] = clampByte(src[k + i1]);
    out[k + 2] = clampByte(src[k + i2]);
  }
  return out;
}

export default createColorArray;
