// SPDX-License-Identifier: GPL-3.0-or-later
// Lint fixture: every export problem the lint knows, once.

/* global device */

export function Name() { return "Bad Exports"; }
export function VendorId() { return 0x1209; }
export function ProductId() { return [0x0002, 0x0004]; }
export function DeviceType() { return "mouse"; }
export function Size() { return [2, 1]; }
export function LedNames() { return ["A", "B", "C"]; }
export function LedPositions() { return [[0, 0], [0, 0]]; }
export function Version() { throw new Error("no version"); }
export function ControllableParameters() {
  return [
    { property: "mode", type: "combobox", default: "Canvas" },
    { property: "mode", type: "number", default: "5" },
    { property: "device", type: "boolean", default: false },
    { property: "speed", type: "slider" },
  ];
}
export function Initialize() {
  // window.alert is only in a comment, "document" only in a string: neither counts.
  const name = "document";
  setTimeout(() => device.log(name), 10);
}
export function Render() {
  if (typeof window !== "undefined") return;
  const store = window.localStorage;
  store.x = 1;
}
