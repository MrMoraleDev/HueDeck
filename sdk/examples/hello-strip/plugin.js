// SPDX-License-Identifier: MIT
//
// Hello Strip: the example plugin of sdk/README.md. A made-up USB HID LED strip with 8 LEDs in
// a row (pid.codes test id 1209:0001). Its protocol, also made up: one 65-byte output report,
// report id 0, then command 0x01 "set colours", the LED count, and R, G, B per LED.

/// <reference path="../../lightloom.d.ts" />
/* global device, shutdownColor, LightingMode, forcedColor */

const LEDS = 8;

export function Name() { return "Hello Strip"; }
export function Publisher() { return "Lightloom SDK example"; }
export function Version() { return "0.1.0"; }
export function VendorId() { return 0x1209; }
export function ProductId() { return [0x0001]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [LEDS, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return Array.from({ length: LEDS }, (_, i) => `LED ${i + 1}`); }
export function LedPositions() { return Array.from({ length: LEDS }, (_, i) => [i, 0]); }

export function ControllableParameters() {
  return [
    { property: "shutdownColor", group: "lighting", label: "Shutdown Color", type: "color", default: "#000000" },
    { property: "LightingMode", group: "lighting", label: "Lighting Mode", type: "combobox", values: ["Canvas", "Forced"], default: "Canvas" },
    { property: "forcedColor", group: "lighting", label: "Forced Color", type: "color", default: "#009bde" },
  ];
}

// The strip has one HID interface; accept its collection only.
/** @param {Lightloom.HidEndpoint} endpoint */
export function Validate(endpoint) {
  return endpoint.interface === 0 || endpoint.interface === -1;
}

export function Initialize() {
  device.setName("Hello Strip");
}

export function Render() {
  sendColors();
}

/** @param {boolean} [systemSuspending] */
export function Shutdown(systemSuspending) {
  sendColors(systemSuspending ? "#000000" : shutdownColor);
}

/** @param {string} [overrideColor] */
function sendColors(overrideColor) {
  const packet = [0x00, 0x01, LEDS];
  for (let i = 0; i < LEDS; i++) {
    /** @type {Lightloom.RGB} */
    let color;
    if (overrideColor) {
      color = hexToRgb(overrideColor);
    } else if (LightingMode === "Forced") {
      color = hexToRgb(forcedColor);
    } else {
      color = device.color(i, 0);
    }
    packet.push(color[0], color[1], color[2]);
  }
  device.write(packet, 65);
}

/**
 * @param {string} hex
 * @returns {Lightloom.RGB}
 */
function hexToRgb(hex) {
  const n = parseInt(String(hex).replace("#", ""), 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}
