// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the lights of the Razer Hanbo Chroma liquid cooler (pump cap and three fans).
// Adapted from OpenRGB Controllers/RazerController/RazerHanboController/RazerHanboController.cpp
// and .h, RazerHanboController/RGBController_RazerHanbo.cpp and .h, RazerControllerDetect.cpp,
// RazerDevices.cpp and .h (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Joseph East (dripsnek)  12 Apr 2025" (RazerHanboController,
// RGBController_RazerHanbo); "Adam Honse (CalcProgrammer1)  22 Jan 2021"
// (RazerControllerDetect.cpp); "Adam Honse (CalcProgrammer1)  04 Sep 2021" (RazerDevices).
//
// Protocol notes: docs/hardware/razer-hanbo.md. Interface 0, collection 0xFF00/0x0001, 65-byte
// unnumbered output reports (report id byte 0x00, then a command byte and 63 argument bytes).
// Direct mode is a four-command sequence (0x82, 0x80, 0x80, 0x82), each answered by the cooler;
// per frame one colour report per zone (0x32 pump, 0x40 fans), colours in G, R, B order. Pump and
// fan speeds are never touched.

/* global device */

import { DETECTORS, DEVICES, VENDOR_ID } from "./devices.js";

const PRODUCT_IDS = [...new Set(DETECTORS.map((d) => d[1]))];

/** sizeof(razer_hanbo_report): header[2] (dummy report id, command) and 63 argument bytes. */
const REPORT_LENGTH = 65;
const READ_TIMEOUT_MS = 2;
const RESEND_MS = 1000;
/** RazerHanboController.h zone order: PUMP, FAN1, FAN2, FAN3. */
const PUMP = 0;

export function Name() { return "Razer Hanbo Chroma"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/razer-hanbo"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCT_IDS.slice(); }
export function Type() { return "Hid"; }
export function DeviceType() { return "aio"; }
export function Size() { return [geometry().width, geometry().height]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return geometry().names; }
export function LedPositions() { return geometry().cells.map((c) => c.slice()); }
export function ImageUrl() { return ""; }
// Razer's own software drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["RazerAppEngine.exe", "RazerCentralService.exe", "Razer Synapse 3.exe", "Razer Synapse.exe"]; }

let zones = null;     // [{ name, cells: [[x, y], ...] }] in PUMP, FAN1, FAN2, FAN3 order
let lastColours = null;
let lastSentAt = 0;

/** REGISTER_HID_DETECTOR_IPU("Razer Hanbo Chroma", ..., 0x00, 0xFF00, 0x01). */
export function Validate(endpoint) {
  const pid = device.productId();
  return DETECTORS.some(([, p, , iface, page, usage]) => p === pid
    && endpoint.interface === iface && endpoint.usage_page === page && endpoint.usage === usage);
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  const pid = device.productId();
  const row = DETECTORS.find((d) => d[1] === pid);
  if (!row || !DEVICES[pid]) {
    zones = null;
    device.log(`Razer Hanbo: product id 0x${(pid & 0xFFFF).toString(16)} is not one this plugin knows.`);
    return;
  }
  const g = geometry(pid);
  zones = g.zones;
  device.setName(row[0]);
  device.setSize([g.width, g.height]);
  device.setControllableLeds(g.names, g.cells.map((c) => c.slice()));
  // RazerHanboController::RazerHanboController() -> SetDirectMode(). (Its GetFirmware() request
  // only fills OpenRGB's version and serial fields and is left out.)
  setDirectMode();
}

export function Render() {
  if (!zones) {
    return;
  }
  const colours = zones.map((z) => z.cells.map(([x, y]) => {
    const c = device.color(x, y);
    return [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF];
  }));
  const now = Date.now();
  if (lastColours && JSON.stringify(colours) === JSON.stringify(lastColours) && now - lastSentAt < RESEND_MS) {
    return;
  }
  // RGBController_RazerHanbo::DeviceUpdateLEDs(): PUMP, FAN1, FAN2, FAN3.
  colours.forEach((zoneColours, index) => setZoneLeds(index, zoneColours));
  lastColours = colours;
  lastSentAt = now;
}

/** OpenRGB has no exit step; HueDeck hands the cooler its hardware spectrum cycle. */
export function Shutdown(_systemSuspending) {
  if (!zones) {
    return;
  }
  setModeSpectrumCycle();
}

/** The table entry's zones (Pump 16, Fan 1..3 18 LEDs each), stacked top to bottom. */
function geometry(pid = PRODUCT_IDS[0]) {
  const entry = DEVICES[pid];
  const out = { width: 1, height: 0, cells: [], names: [], zones: [] };
  for (const zone of entry.zones) {
    const z = { name: zone.name, cells: [] };
    for (let r = 0; r < zone.rows; r++) {
      for (let c = 0; c < zone.cols; c++) {
        const cell = [c, out.height + r];
        z.cells.push(cell);
        out.cells.push(cell);
        out.names.push(`${zone.name} LED ${r * zone.cols + c + 1}`);
      }
    }
    out.width = Math.max(out.width, zone.cols);
    out.height += zone.rows;
    out.zones.push(z);
  }
  return out;
}

/** razer_hanbo_create_report(command): byte 0 is the dummy report id 0x00. */
function report(command, args = []) {
  const bytes = new Array(REPORT_LENGTH).fill(0);
  bytes[1] = command & 0xFF;
  for (let i = 0; i < args.length && i < 63; i++) {
    bytes[2 + i] = args[i] & 0xFF;
  }
  return bytes;
}

/** UsbSend() then UsbReceive() (hid_read_timeout, 2 ms; the answer is not checked). */
function transact(bytes) {
  device.write(bytes, REPORT_LENGTH);
  device.read([0x00], REPORT_LENGTH, READ_TIMEOUT_MS);
}

/** SetDirectMode(). */
function setDirectMode() {
  transact(report(0x82, [0x01]));
  transact(report(0x80, [0x01, 0x00, 0x01]));
  transact(report(0x80, [0x01, 0x01, 0x01]));
  transact(report(0x82, [0x01]));
}

/** SetModeSpectrumCycle(). */
function setModeSpectrumCycle() {
  transact(report(0x82, [0x01]));
  transact(report(0x80, [0x01]));
  transact(report(0x80, [0x01, 0x01]));
}

/**
 * SetZoneLeds(): command 0x32 for the pump, 0x40 for a fan; arguments 01 07 00 00 00 00 <zone>
 * (the hex string "0107000000000" + zone index), then G, R, B per LED; no answer, 2 ms gap.
 */
function setZoneLeds(zoneIndex, colours) {
  const args = [0x01, 0x07, 0x00, 0x00, 0x00, 0x00, zoneIndex];
  for (const [r, g, b] of colours) {
    args.push(g, r, b);
  }
  device.write(report(zoneIndex > PUMP ? 0x40 : 0x32, args), REPORT_LENGTH);
  device.pause(2);
}
