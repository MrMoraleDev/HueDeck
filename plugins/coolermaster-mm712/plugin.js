// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the logo light of the Cooler Master MM712 mouse.
// Adapted from OpenRGB Controllers/CoolerMasterController/CoolerMasterControllerDetect.cpp,
// CMMM712Controller/CMMM712Controller.cpp and .h, CMMM712Controller/
// RGBController_CMMM712Controller.cpp and .h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Chris M (Dr_No)  14 Feb 2021" and "Frans Meulenbroeks  08 Dec 2024"
// (CMMM712Controller, RGBController_CMMM712Controller); CoolerMasterControllerDetect.cpp names no
// author ("This file is part of the OpenRGB project"). The protocol text file next to those
// sources (MM712protocol.txt) carries no license header and was not used.
//
// Protocol notes: docs/hardware/coolermaster-mm712.md. In short: interface 3 (usage page 0xFF0A,
// usage 2), 65-byte output reports (report id 0x00). Start: 44 81 02 (+ answer), then direct
// mode on (5A 81 02). Every frame: 5A 81 02 and 5A 81 03 R G B. Shutdown: direct mode off
// (5A 81 01), which gives the light back to the mouse's own mode. Never sent: the save command
// (54 81 01), mode or profile writes (4C 81 04 / 08, 44 81 01), DPI or any other setting.

/* global device */

const PRODUCT = { pid: 0x0169, name: "Cooler Master MM712" };
const PACKET_SIZE = 65;       // CM_MM712_PACKET_SIZE
const READ_TIMEOUT_MS = 250;  // CM_MM712_INTERRUPT_TIMEOUT
const KEEPALIVE_MS = 1000;
const LED_NAMES = ["Logo LED"];
const LED_POSITIONS = [[0, 0]];

export function Name() { return PRODUCT.name; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/coolermaster-mm712"; }
export function VendorId() { return 0x2516; }
export function ProductId() { return [PRODUCT.pid]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LED_NAMES.slice(); }
export function LedPositions() { return LED_POSITIONS.map((p) => p.slice()); }
export function ImageUrl() { return ""; }
export function ConflictingProcesses() { return ["MasterPlus.exe", "MasterPlusApp.exe"]; }

let active = false;
let lastColour = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IPU(..., 3, 0xFF0A, 2).
export function Validate(endpoint) {
  return endpoint.interface === 3 && endpoint.usage_page === 0xFF0A && endpoint.usage === 0x0002;
}

export function Initialize() {
  lastColour = null;
  lastSentAt = 0;
  active = device.productId() === PRODUCT.pid;
  if (!active) {
    device.log(`Cooler Master MM712: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(PRODUCT.name);
  // Constructor: SendInitPacket (44 81 02, then the answer). Its mode / colour status round trip
  // (4C 81 07, 4C 81 08 <same mode>, 4C 81 03) only fills OpenRGB's user interface and is left out.
  write([0x00, 0x44, 0x81, 0x02]);
  device.read([0x00], PACKET_SIZE, READ_TIMEOUT_MS);
  // OpenRGB's switch to Direct (DeviceUpdateMode): SetDirectMode(true).
  setDirectMode(true);
}

export function Render() {
  if (!active) {
    return;
  }
  const colour = device.color(LED_POSITIONS[0][0], LED_POSITIONS[0][1]);
  const now = Date.now();
  if (lastColour !== null && colour[0] === lastColour[0] && colour[1] === lastColour[1] &&
      colour[2] === lastColour[2] && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (setLedsDirect(colour)) {
    lastColour = colour;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step; leaving its Direct mode for any other mode starts with
// SetDirectMode(false). Lightloom sends only that: the mouse goes back to its own (stored) mode.
export function Shutdown(SystemSuspending) {
  if (!active) {
    return;
  }
  setDirectMode(false);
}

// CMMM712Controller::SetLedsDirect. OpenRGB's current_mode never becomes "direct" (only the
// device's own modes are read into it), so every update turns direct mode on again first.
// RGBController_CMMM712Controller forces the brightness to 255, which leaves the colour as is.
function setLedsDirect(colour) {
  const ok = setDirectMode(true);
  return write([0x00, 0x5A, 0x81, 0x03, colour[0] & 0xFF, colour[1] & 0xFF, colour[2] & 0xFF]) && ok;
}

// SetDirectMode: 5A 81 02 (on) / 5A 81 01 (off), no answer read.
function setDirectMode(on) {
  return write([0x00, 0x5A, 0x81, on ? 0x02 : 0x01]);
}

function write(bytes) {
  const buf = new Array(PACKET_SIZE).fill(0);
  bytes.forEach((b, i) => { buf[i] = b & 0xFF; });
  return device.write(buf, PACKET_SIZE) !== -1;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
