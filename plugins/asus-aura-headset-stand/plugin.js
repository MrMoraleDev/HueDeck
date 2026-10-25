// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the ASUS ROG Throne, Throne Qi and Throne Qi Gundam headset stands, per LED
// (17 underglow LEDs and the logo).
// Adapted from OpenRGB Controllers/AsusAuraUSBController/AsusAuraUSBControllerDetect.cpp,
// AsusAuraHeadsetStandController/AsusAuraHeadsetStandController.cpp and .h,
// AsusAuraHeadsetStandController/RGBController_AsusAuraHeadsetStand.cpp and .h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Mola19  06 Apr 2021" (AsusAuraHeadsetStandController,
// RGBController_AsusAuraHeadsetStand); AsusAuraUSBControllerDetect.cpp names no author ("This file
// is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/asus-aura-headset-stand.md. In short: interface 0; per frame two
// 65-byte output reports, 00 C0 81 00 00 + 15 x (00 R G B) and 00 C0 81 01 00 + 3 x (00 R G B).
// Never sent: the save command (00 50 55) and hardware effects (00 51 28).

/* global device */

const VENDOR_ID = 0x0B05;
const REPORT_LENGTH = 65;
const KEEPALIVE_MS = 1000;

// AsusAuraUSBControllerDetect.cpp: REGISTER_HID_DETECTOR_I(..., 0).
const PRODUCTS = [
  { pid: 0x18D9, name: "ASUS ROG Throne" },
  { pid: 0x18C5, name: "ASUS ROG Throne QI" },
  { pid: 0x1994, name: "ASUS ROG Throne QI GUNDAM" },
];

// RGBController_AuraHeadsetStand::SetupZones: zone "Underglow" (17 LEDs, "Underglow LED 0" .. 16)
// on row 0, zone "Logo" ("Logo LED") on row 1; LED order = wire order.
const LED_NAMES = [...Array.from({ length: 17 }, (_, i) => `Underglow LED ${i}`), "Logo LED"];
const LED_CELLS = [...Array.from({ length: 17 }, (_, i) => [i, 0]), [8, 1]];

export function Name() { return "ASUS Aura Headset Stand"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/asus-aura-headset-stand"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
export function Size() { return [17, 2]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LED_NAMES.slice(); }
export function LedPositions() { return LED_CELLS.map((c) => c.slice()); }
export function ImageUrl() { return ""; }
// ASUS's own software drives the stand while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() {
  return ["ArmouryCrate.exe", "ArmouryCrate.Service.exe", "LightingService.exe"];
}

let product = null;
let lastFrame = null;
let lastFrameAt = 0;

// REGISTER_HID_DETECTOR_I(..., 0): any collection of interface 0.
export function Validate(endpoint) {
  return endpoint != null && endpoint.interface === 0;
}

export function Initialize() {
  lastFrame = null;
  lastFrameAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (product === null) {
    device.log(`ASUS Aura Headset Stand: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  // OpenRGB's start is GetVersion() (00 12 00 + read), for its device information only: not sent.
  // Direct (OpenRGB's first mode) needs no mode command: its DeviceUpdateMode just sends the LEDs.
}

export function Render() {
  if (product === null) {
    return;
  }
  const reports = updateLeds(LED_CELLS.map(([x, y]) => device.color(x, y)));
  const now = Date.now();
  if (lastFrame !== null && same(reports, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
    return;
  }
  let ok = true;
  for (const r of reports) {
    ok = device.write(r, REPORT_LENGTH) !== -1 && ok;
  }
  lastFrame = ok ? reports : null;
  lastFrameAt = now;
}

// OpenRGB has no exit step; its other modes are hardware effects (00 51 28), and the stand's own
// saved effect cannot be selected without one, so the LEDs are set black, on exit and suspend.
export function Shutdown(SystemSuspending) {
  if (product === null) {
    return;
  }
  for (const r of updateLeds(LED_CELLS.map(() => [0, 0, 0]))) {
    device.write(r, REPORT_LENGTH);
  }
  lastFrame = null;
}

// AuraHeadsetStandController::UpdateLeds: LEDs 0..14, then 15..17.
function updateLeds(colours) {
  const first = new Array(REPORT_LENGTH).fill(0);
  first[0x00] = 0x00;
  first[0x01] = 0xC0;
  first[0x02] = 0x81;
  first[0x03] = 0x00;
  first[0x04] = 0x00;
  for (let i = 0; i < 60; i += 4) {
    const c = colours[i / 4];
    first[6 + i] = c[0] & 0xFF;
    first[7 + i] = c[1] & 0xFF;
    first[8 + i] = c[2] & 0xFF;
  }
  const second = new Array(REPORT_LENGTH).fill(0);
  second[0x00] = 0x00;
  second[0x01] = 0xC0;
  second[0x02] = 0x81;
  second[0x03] = 0x01;
  second[0x04] = 0x00;
  for (let i = 0; i < 12; i += 4) {
    const c = colours[i / 4 + 15];
    second[6 + i] = c[0] & 0xFF;
    second[7 + i] = c[1] & 0xFF;
    second[8 + i] = c[2] & 0xFF;
  }
  return [first, second];
}

function same(a, b) {
  for (let r = 0; r < a.length; r++) {
    for (let i = 0; i < REPORT_LENGTH; i++) {
      if (a[r][i] !== b[r][i]) {
        return false;
      }
    }
  }
  return true;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
