// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: Razer Kraken 7.1 family (Kraken 7.1, 7.1 Chroma, 7.1 V2, Kitty Black Edition V2,
// Ultimate) and Kraken V3 family (Kraken V3, V3 X, V3 HyperSense, Kitty V2 Pro) headsets.
// Adapted from OpenRGB Controllers/RazerController/RazerKrakenController/RazerKrakenController.cpp
// and .h, RazerKrakenController/RGBController_RazerKraken.cpp and .h, RazerKrakenV3Controller/
// RazerKrakenV3Controller.cpp and .h, RazerKrakenV3Controller/RGBController_RazerKrakenV3.cpp and
// .h, RazerControllerDetect.cpp, RazerDevices.cpp and .h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  28 Feb 2021" (RazerKrakenController,
// RGBController_RazerKraken); "Greg Sandstrom (superstrom)  1 Nov 2025" (RazerKrakenV3Controller,
// RGBController_RazerKrakenV3); "Adam Honse (CalcProgrammer1)  22 Jan 2021"
// (RazerControllerDetect.cpp); "Adam Honse (CalcProgrammer1)  04 Sep 2021" (RazerDevices).
// The product table in devices.js is generated from OpenRGB's tables; the protocol is hand-written.
//
// Protocol notes: docs/hardware/razer-kraken.md. Both families use interface 3, collection
// 0x000C/0x0001, output reports. Kraken 7.1: 37-byte report id 0x04 writes of a colour (address
// 0x1189) and of the LED effect byte (0x1008 or 0x172D), destination 0x40. Kraken V3: 15-byte
// report id 0x40 commands (0x01 mode, 0x02 brightness, 0x03 colours).

/* global device */

import { DETECTORS, DEVICES, KRAKEN_ADDRESSES, KRAKEN_CUSTOM_RGB, VENDOR_ID } from "./devices.js";

const PRODUCT_IDS = [...new Set(DETECTORS.map((d) => d[1]))];

/** sizeof(razer_kraken_request_report): report id, destination, length, address (2), 32 arguments. */
const KRAKEN_REPORT_LENGTH = 37;
const KRAKEN_REPORT_ID = 0x04;
const KRAKEN_DESTINATION_WRITE = 0x40;
/** razer_kraken_effect_byte bits (first bit field = least significant bit). */
const EFFECT_ON = 0x01;
const EFFECT_SPECTRUM = 0x04;

/** sizeof(razer_kraken_v3_request_report): report id, command id, 13 arguments. */
const V3_REPORT_LENGTH = 15;
const V3_REPORT_ID = 0x40;
const V3_SET_MODE = 0x01;
const V3_SET_BRIGHTNESS = 0x02;
const V3_SET_COLOR = 0x03;
const V3_MODE_DIRECT = 0x08;
const V3_MODE_WAVE = 0x03;

const RESEND_MS = 1000;
// Kraken 7.1: every frame writes device memory (destination 0x40, not documented as RAM), so
// changed colours go out at most every KRAKEN71_MIN_INTERVAL_MS (a few per second, not 30).
const KRAKEN71_MIN_INTERVAL_MS = 250;

export function Name() { return "Razer Kraken"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/razer-kraken"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCT_IDS.slice(); }
export function Type() { return "Hid"; }
export function DeviceType() { return "headphones"; }
// Placeholders; Initialize() sets the product's own.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Headset"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// Razer's own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["RazerAppEngine.exe", "RazerCentralService.exe", "Razer Synapse 3.exe", "Razer Synapse.exe"]; }

let product = null;   // { pid, protocol, cells, cols }
let lastColours = null;
let lastSentAt = 0;

/** REGISTER_HID_DETECTOR_IPU(..., 0x03, 0x0C, 0x01) for every product. */
export function Validate(endpoint) {
  const pid = device.productId();
  return DETECTORS.some(([, p, , iface, page, usage]) => p === pid
    && endpoint.interface === iface && endpoint.usage_page === page && endpoint.usage === usage);
}

export function Initialize() {
  product = null;
  lastColours = null;
  lastSentAt = 0;
  const pid = device.productId();
  const row = DETECTORS.find((d) => d[1] === pid);
  if (!row) {
    device.log(`Razer Kraken: product id 0x${hex4(pid)} is not one this plugin knows.`);
    return;
  }
  product = { pid, protocol: row[2], ...geometry(pid) };
  device.setName(row[0]);
  device.setSize([product.width, product.height]);
  device.setControllableLeds(product.names, product.cells.map((c) => c.slice()));
  if (product.protocol === "kraken-v3") {
    // The headset starts in its wave effect; switching to Direct sends SetModeDirect() and
    // SetBrightness(brightness), the maximum by default (Lightloom's colours carry the brightness).
    v3SetModeDirect();
    v3SetBrightness(255);
  }
  // Kraken 7.1: switching to Direct sends nothing; every frame sets the custom colour.
}

export function Render() {
  if (!product) {
    return;
  }
  const colours = product.cells.map(([x, y]) => {
    const c = device.color(x, y);
    return [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF];
  });
  const now = Date.now();
  if (lastColours && same(colours, lastColours) && now - lastSentAt < RESEND_MS) {
    return;
  }
  if (product.protocol !== "kraken-v3" && lastColours && now - lastSentAt < KRAKEN71_MIN_INTERVAL_MS) {
    return;
  }
  if (product.protocol === "kraken-v3") {
    v3SetDirect(colours);
  } else {
    krakenSetModeCustom(colours[0]);
  }
  lastColours = colours;
  lastSentAt = now;
}

// Exit modes (Lightloom extension, docs/host-api/save-shutdown.md).
export function ShutdownModes() { return ["restore", "keep", "off"]; }

/**
 * OpenRGB has no exit step. Lightloom gives the headset a hardware effect back: the spectrum cycle
 * on the Kraken 7.1 family (SetModeSpectrumCycle()), the wave at full brightness on the V3
 * family (SetModeWave(), the effect the V3 starts with). That is "restore", SignalRGB's call
 * without a mode, and suspend. Lightloom exit modes, nothing stored (OpenRGB has no save command):
 * "keep" sends the last frame once more as a frame does (Kraken 7.1: the custom colour and the
 * effect byte "on"; V3: the Direct colours), "off" a black V3 frame or, on the Kraken 7.1, OpenRGB's
 * SetModeOff() (effect byte 0). OpenRGB sends no keepalive in either mode, so the headset is
 * expected to hold them while it has power (unverified on hardware).
 */
export function Shutdown(systemSuspending, mode) {
  if (!product) {
    return;
  }
  if (!systemSuspending && (mode === "keep" || mode === "off")) {
    const colours = product.cells.map(([x, y]) => {
      const c = mode === "keep" ? device.color(x, y) : [0, 0, 0];
      return [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF];
    });
    if (product.protocol === "kraken-v3") {
      v3SetDirect(colours);
    } else if (mode === "keep") {
      krakenSetModeCustom(colours[0]);
    } else {
      krakenSetModeNone();
    }
    return;
  }
  if (product.protocol === "kraken-v3") {
    v3SetModeWave();
    v3SetBrightness(255);
  } else {
    krakenSetModeSpectrumCycle();
  }
}

/**
 * LEDs from the product's table entry, zones stacked top to bottom. Kraken 7.1: OpenRGB sends
 * only colours[0] (one colour for the whole headset), so Lightloom offers that one LED, "Headset"
 * (0x0501 and 0x0506 have no table entry at all; OpenRGB falls back to device_list[0]).
 */
function geometry(pid) {
  const entry = DEVICES[pid];
  if (!entry || DETECTORS.find((d) => d[1] === pid)[2] === "kraken") {
    return { width: 1, height: 1, cells: [[0, 0]], names: ["Headset"], cols: 1 };
  }
  const cells = [];
  const names = [];
  let width = 1;
  let y = 0;
  for (const zone of entry.zones) {
    width = Math.max(width, zone.cols);
    for (let r = 0; r < zone.rows; r++) {
      for (let c = 0; c < zone.cols; c++) {
        cells.push([c, y + r]);
        const count = zone.rows * zone.cols;
        names.push(count === 1 ? zone.name : `${zone.name} LED ${r * zone.cols + c + 1}`);
      }
    }
    y += zone.rows;
  }
  return { width, height: Math.max(1, y), cells, names, cols: entry.cols };
}

// --- Kraken 7.1 (RazerKrakenController) ----------------------------------------------------------

/** razer_kraken_create_report(0x04, destination, length, address) with its arguments. */
function krakenReport(destination, length, address, args) {
  const bytes = new Array(KRAKEN_REPORT_LENGTH).fill(0);
  bytes[0] = KRAKEN_REPORT_ID;
  bytes[1] = destination;
  bytes[2] = length;
  bytes[3] = (address >> 8) & 0xFF;
  bytes[4] = address & 0xFF;
  for (let i = 0; i < args.length; i++) {
    bytes[5 + i] = args[i] & 0xFF;
  }
  return bytes;
}

/** razer_set_mode_custom(): the colour (only on products with RGB) then the effect byte "on". */
function krakenSetModeCustom([r, g, b]) {
  const addresses = KRAKEN_ADDRESSES[product.pid];
  if (KRAKEN_CUSTOM_RGB.includes(product.pid)) {
    device.write(krakenReport(KRAKEN_DESTINATION_WRITE, 3, addresses.custom, [r, g, b]), KRAKEN_REPORT_LENGTH);
  }
  device.write(krakenReport(KRAKEN_DESTINATION_WRITE, 0x01, addresses.ledMode, [EFFECT_ON]), KRAKEN_REPORT_LENGTH);
}

/** razer_set_mode_none() (SetModeOff()): effect byte 0, the lights off. */
function krakenSetModeNone() {
  const addresses = KRAKEN_ADDRESSES[product.pid];
  device.write(krakenReport(KRAKEN_DESTINATION_WRITE, 0x01, addresses.ledMode, [0x00]), KRAKEN_REPORT_LENGTH);
}

/** razer_set_mode_spectrum_cycle(): effect byte "on" + "spectrum cycling". */
function krakenSetModeSpectrumCycle() {
  const addresses = KRAKEN_ADDRESSES[product.pid];
  device.write(krakenReport(KRAKEN_DESTINATION_WRITE, 0x01, addresses.ledMode, [EFFECT_ON | EFFECT_SPECTRUM]), KRAKEN_REPORT_LENGTH);
}

// --- Kraken V3 (RazerKrakenV3Controller) -----------------------------------------------------------

function v3Report(commandId, args) {
  const bytes = new Array(V3_REPORT_LENGTH).fill(0);
  bytes[0] = V3_REPORT_ID;
  bytes[1] = commandId;
  for (let i = 0; i < args.length && i < 13; i++) {
    bytes[2 + i] = args[i] & 0xFF;
  }
  return bytes;
}

/** SetModeDirect(): mode command, arguments [0x00, 0x0F, 0x08]. */
function v3SetModeDirect() {
  device.write(v3Report(V3_SET_MODE, [0x00, 0x0F, V3_MODE_DIRECT]), V3_REPORT_LENGTH);
}

/** SetModeWave(): mode command, arguments [0x00, 0x0F, 0x03]. */
function v3SetModeWave() {
  device.write(v3Report(V3_SET_MODE, [0x00, 0x0F, V3_MODE_WAVE]), V3_REPORT_LENGTH);
}

/** SetBrightness(): arguments [0x00, 0x00, brightness]. */
function v3SetBrightness(brightness) {
  device.write(v3Report(V3_SET_BRIGHTNESS, [0x00, 0x00, brightness]), V3_REPORT_LENGTH);
}

/** SetDirect(): arguments [0x00, r, g, b, ...] for the entry's `cols` LEDs. */
function v3SetDirect(colours) {
  const args = [0x00];
  for (let i = 0; i < product.cols; i++) {
    const c = colours[i] ?? [0, 0, 0];
    args.push(c[0], c[1], c[2]);
  }
  device.write(v3Report(V3_SET_COLOR, args), V3_REPORT_LENGTH);
}

function same(a, b) {
  return a.length === b.length && a.every((c, i) => c[0] === b[i][0] && c[1] === b[i][1] && c[2] === b[i][2]);
}

function hex4(v) {
  return (Number(v) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
