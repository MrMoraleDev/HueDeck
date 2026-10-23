// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the ASUS ROG Balteus and Balteus Qi mousemats, per LED (15 LEDs).
// Adapted from OpenRGB Controllers/AsusAuraUSBController/AsusAuraUSBControllerDetect.cpp,
// AsusAuraMousematController/AsusAuraMousematController.cpp and .h, AsusAuraMousematController/
// RGBController_AsusAuraMousemat.cpp and .h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  10 Jan 2022" (AsusAuraMousematController,
// RGBController_AsusAuraMousemat.cpp); "Mola19  06 Apr 2021" (RGBController_AsusAuraMousemat.h);
// AsusAuraUSBControllerDetect.cpp names no author ("This file is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/asus-aura-mousemat.md. In short: usage page 0xFF06, usage 1;
// per frame one 65-byte output report EE C0 81 00 00 + 15 x (00 R G B). Never sent: the save
// command (EE 50 03) and hardware effects (EE 51 28).

/* global device */

const VENDOR_ID = 0x0B05;
const REPORT_LENGTH = 65;
const LED_COUNT = 15;
const KEEPALIVE_MS = 1000;

// AsusAuraUSBControllerDetect.cpp: REGISTER_HID_DETECTOR_PU(..., 0xFF06, 1).
const PRODUCTS = [
  { pid: 0x1891, name: "ASUS ROG Balteus" },
  { pid: 0x1890, name: "ASUS ROG Balteus Qi" },
];

// RGBController_AuraMousemat::SetupZones: one linear zone "Mousemat", "Mousemat LED 0" .. 14.
const LED_NAMES = Array.from({ length: LED_COUNT }, (_, i) => `Mousemat LED ${i}`);
const LED_CELLS = Array.from({ length: LED_COUNT }, (_, i) => [i, 0]);

export function Name() { return "ASUS Aura Mousemat"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/asus-aura-mousemat"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mousepad"; }
export function Size() { return [LED_COUNT, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LED_NAMES.slice(); }
export function LedPositions() { return LED_CELLS.map((c) => c.slice()); }
export function ImageUrl() { return ""; }
// ASUS's own software drives the mousemat while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() {
  return ["ArmouryCrate.exe", "ArmouryCrate.Service.exe", "LightingService.exe"];
}

let product = null;
let lastFrame = null;
let lastFrameAt = 0;

// REGISTER_HID_DETECTOR_PU(..., 0xFF06, 1): any interface.
export function Validate(endpoint) {
  return endpoint != null && Number(endpoint.usage_page) === 0xFF06 && Number(endpoint.usage) === 0x0001;
}

export function Initialize() {
  lastFrame = null;
  lastFrameAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (product === null) {
    device.log(`ASUS Aura Mousemat: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  // OpenRGB's start is GetVersion() (EE 12 00 + read), for its device information only: not sent.
  // Direct (OpenRGB's first mode) needs no mode command: its DeviceUpdateMode just sends the LEDs.
}

export function Render() {
  if (product === null) {
    return;
  }
  const report = updateLeds(LED_CELLS.map(([x, y]) => device.color(x, y)));
  const now = Date.now();
  if (lastFrame !== null && same(report, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
    return;
  }
  lastFrame = device.write(report, REPORT_LENGTH) !== -1 ? report : null;
  lastFrameAt = now;
}

// OpenRGB has no exit step; its other modes are hardware effects (EE 51 28), and the mat's own
// saved effect cannot be selected without one, so the LEDs are set black, on exit and suspend.
export function Shutdown(SystemSuspending) {
  if (product === null) {
    return;
  }
  device.write(updateLeds(LED_CELLS.map(() => [0, 0, 0])), REPORT_LENGTH);
  lastFrame = null;
}

// AuraMousematController::UpdateLeds.
function updateLeds(colours) {
  const buf = new Array(REPORT_LENGTH).fill(0);
  buf[0x00] = 0xEE;
  buf[0x01] = 0xC0;
  buf[0x02] = 0x81;
  buf[0x03] = 0x00;
  buf[0x04] = 0x00;
  for (let i = 0; i < 60; i += 4) {
    const c = colours[i / 4];
    buf[5 + i] = 0x00;
    buf[6 + i] = c[0] & 0xFF;
    buf[7 + i] = c[1] & 0xFF;
    buf[8 + i] = c[2] & 0xFF;
  }
  return buf;
}

function same(a, b) {
  for (let i = 0; i < REPORT_LENGTH; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
