// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the light of the Logitech (Saitek) X56 Rhino HOTAS joystick and throttle.
// Adapted from OpenRGB Controllers/LogitechController/LogitechControllerDetect.cpp,
// LogitechX56Controller/LogitechX56Controller.cpp and .h, LogitechX56Controller/
// RGBController_LogitechX56.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Edbgon  11 Jun 2021" (LogitechX56Controller, RGBController_LogitechX56);
// LogitechControllerDetect.cpp names no author ("This file is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/logitech-x56.md. In short: per colour two 64-byte feature reports
// on interface 2, 09 00 02 <brightness> and 09 00 03 R G B. OpenRGB's save command (01 01, a
// manual save) is never sent.

/* global device */

const VENDOR_ID = 0x0738;          // LOGITECH_X56_VID
const PRODUCTS = [
  { pid: 0x2221, name: "Logitech X56 Rhino Hotas Joystick" },
  { pid: 0xA221, name: "Logitech X56 Rhino Hotas Throttle" },
];
const PACKET_SIZE = 64;            // X56_CONTROLLER_PACKET_SIZE
// RGBController_LogitechX56: Direct.brightness = brightness_max = 0x64. Lightloom already applies its
// own brightness to the colours, so the device's stays at its maximum.
const BRIGHTNESS = 0x64;
const KEEPALIVE_MS = 1000;

export function Name() { return "Logitech X56"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/logitech-x56"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["X56"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// Logitech G HUB and the older X56 software (Logitech Gaming Software) drive the light while they
// run; Lightloom reports them, never closes them.
export function ConflictingProcesses() { return ["lghub.exe", "lghub_agent.exe", "lghub_updater.exe", "LCore.exe"]; }

let product = null;
let lastColour = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IPU(..., 2, 0xFF00, 1) and (..., 2, 0xFF00, 3) for both ids.
export function Validate(endpoint) {
  return PRODUCTS.some((p) => p.pid === device.productId())
    && endpoint.interface === 2
    && endpoint.usage_page === 0xFF00
    && (endpoint.usage === 0x0001 || endpoint.usage === 0x0003);
}

export function Initialize() {
  lastColour = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (!product) {
    device.log(`Logitech X56: product id 0x${(device.productId() & 0xFFFF).toString(16)} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  // DeviceUpdateMode() only calls DeviceUpdateLEDs(): the first Render() sends it.
}

export function Render() {
  if (!product) {
    return;
  }
  const colour = device.color(0, 0);
  const now = Date.now();
  if (lastColour !== null && colour.every((v, i) => v === lastColour[i]) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (setColor(colour)) {
    lastColour = colour;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step and no hardware effect for the X56: the light goes black.
export function Shutdown(SystemSuspending) {
  if (!product) {
    return;
  }
  setColor([0, 0, 0]);
}

// LogitechX56Controller::SetColor.
function setColor([r, g, b]) {
  const init = new Array(PACKET_SIZE).fill(0);
  init[0] = 0x09;
  init[2] = 0x02;
  init[3] = BRIGHTNESS;
  const colour = new Array(PACKET_SIZE).fill(0);
  colour[0] = 0x09;
  colour[2] = 0x03;
  colour[3] = r & 0xFF;
  colour[4] = g & 0xFF;
  colour[5] = b & 0xFF;
  const a = device.send_report(init, PACKET_SIZE) !== -1;
  const c = device.send_report(colour, PACKET_SIZE) !== -1;
  return a && c;
}
