// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the two lights (bottom and top) of the SteelSeries QCK Prism Cloth mouse mats.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesQCKMatController/
// SteelSeriesQCKMatController.cpp, SteelSeriesQCKMatController.h,
// RGBController_SteelSeriesQCKMat.cpp, RGBController_SteelSeriesQCKMat.h, and
// Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Edbgon  22 May 2021" (SteelSeriesQCKMatController,
// RGBController_SteelSeriesQCKMat); SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h name
// no author.
//
// Protocol notes: docs/hardware/steelseries-qck.md. In short: interface 0; per update one
// 525-byte feature report (report id 0x00, command 0x0E) with both colours, then a 65-byte output
// report with command 0x0D, exactly as OpenRGB's SetColors does. OpenRGB does not say what 0x0D
// does: it may only apply the colours, or it may also store them in the mat's memory. So that an
// animated effect cannot turn it into a memory write on every frame, Lightloom sends an update only
// when a colour changes, at most once per MIN_INTERVAL_MS (the latest colours win), and never as
// a keepalive.

/* global device */

const PRODUCTS = [
  { pid: 0x150A, name: "SteelSeries QCK Prism Cloth Medium" },
  { pid: 0x150D, name: "SteelSeries QCK Prism Cloth XL" },
  { pid: 0x151E, name: "SteelSeries QCK Prism Cloth XL Destiny Ed." },
  { pid: 0x1520, name: "SteelSeries QCK Prism Cloth XL Destiny 2 Lightfall Ed." },
  { pid: 0x1514, name: "SteelSeries QCK Prism Cloth XL CS:GO Neon Rider Ed." },
  { pid: 0x151C, name: "SteelSeries QCK Prism Cloth XL CS:GO Neo Noir Ed." },
  { pid: 0x1516, name: "SteelSeries QCK Prism Cloth 3XL" },
  { pid: 0x1518, name: "SteelSeries QCK Prism Cloth 4XL" },
  { pid: 0x151A, name: "SteelSeries QCK Prism Cloth 5XL" },
];

// RGBController_SteelSeriesQCKMat::SetupZones: one zone "Mousemat" with two LEDs, colours[0] the
// bottom one and colours[1] the top one (one grid row, OpenRGB order).
const LEDS = [
  { name: "Mat Bottom LED", cell: [0, 0] },
  { name: "Mat Top LED", cell: [1, 0] },
];
const SIZE = [2, 1];
const COLOUR_REPORT_LENGTH = 525;
const TRAILER_REPORT_LENGTH = 65;
// At most one 0x0E + 0x0D pair per this many milliseconds (at most 3600 an hour under a running
// effect, instead of about 108000 at 30 frames a second).
const MIN_INTERVAL_MS = 1000;

export function Name() { return "SteelSeries QCK Prism"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-qck"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mousepad"; }
export function Size() { return SIZE.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LEDS.map((l) => l.name); }
export function LedPositions() { return LEDS.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let enabled = false;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_I(..., 0) for every product: any collection of interface 0.
export function Validate(endpoint) {
  return endpoint.interface === 0;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  enabled = Boolean(product);
  if (!product) {
    device.log(`SteelSeries QCK: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(SIZE.slice());
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // OpenRGB's only mode, Direct, has no mode command (DeviceUpdateMode -> DeviceUpdateLEDs): the
  // first Render() sends the colours.
}

// Sent only when a colour changes, at most once per MIN_INTERVAL_MS (see the header): no
// keepalive. A change inside the interval waits; the next Render() after it sends the colours of
// that moment.
export function Render() {
  if (!enabled) {
    return;
  }
  const colours = LEDS.map((l) => {
    const [r, g, b] = device.color(l.cell[0], l.cell[1]);
    return [r & 0xFF, g & 0xFF, b & 0xFF];
  });
  if (lastColours !== null && sameColours(colours, lastColours)) {
    return;
  }
  const now = Date.now();
  if (lastColours !== null && now - lastSentAt < MIN_INTERVAL_MS) {
    return;
  }
  if (setColors(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step and no other mode for the mat: the lights are switched off (black),
// one 0x0E + 0x0D pair. If 0x0D stores the colours, the mat keeps black after Lightloom exits (the
// docs say so).
export function Shutdown(SystemSuspending) {
  if (!enabled) {
    return;
  }
  setColors(LEDS.map(() => [0, 0, 0]));
}

// SteelSeriesQCKMatController::SetColors.
function setColors(colours) {
  const buf = new Array(COLOUR_REPORT_LENGTH).fill(0);
  buf[0x00] = 0x00;
  buf[0x01] = 0x0E;
  buf[0x03] = 0x02;
  buf[0x08] = 0xFF;
  buf[0x09] = 0x32;
  buf[0x0A] = 0xC8;
  buf[0x0E] = 0x01;
  buf[0x14] = 0xFF;
  buf[0x15] = 0x32;
  buf[0x16] = 0xC8;
  buf[0x19] = 0x01;
  buf[0x1A] = 0x01;
  buf[0x1C] = 0x01;
  buf[0x05] = colours[0][0];
  buf[0x06] = colours[0][1];
  buf[0x07] = colours[0][2];
  buf[0x11] = colours[1][0];
  buf[0x12] = colours[1][1];
  buf[0x13] = colours[1][2];
  let ok = device.send_report(buf, COLOUR_REPORT_LENGTH) !== -1;
  const cbuf = new Array(TRAILER_REPORT_LENGTH).fill(0);
  cbuf[0x01] = 0x0D;
  ok = (device.write(cbuf, TRAILER_REPORT_LENGTH) !== -1) && ok;
  return ok;
}

function sameColours(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) {
      return false;
    }
  }
  return true;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
