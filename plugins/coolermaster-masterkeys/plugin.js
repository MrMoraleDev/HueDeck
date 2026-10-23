// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: Cooler Master MasterKeys (V1 protocol) keyboards, per key: MasterKeys Pro S,
// Pro L and Pro L White.
// Adapted from OpenRGB Controllers/CoolerMasterController/CoolerMasterControllerDetect.cpp,
// CMKeyboardController/CMKeyboardAbstractController.cpp and .h, CMKeyboardV1Controller.cpp and
// .h, CMKeyboardDevices.cpp and .h, RGBController_CMKeyboardController.cpp and .h, and
// KeyboardLayoutManager/KeyboardLayoutManager.cpp and .h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Tam D (too.manyhobbies)  30 Nov 2023" (CMKeyboardAbstractController,
// CMKeyboardV1Controller, CMKeyboardDevices, RGBController_CMKeyboardController);
// "Chris M (Dr_No)  04 Feb 2023" (KeyboardLayoutManager); CoolerMasterControllerDetect.cpp names
// no author ("This file is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/coolermaster-masterkeys.md. In short: interface 1 (usage page
// 0xFF00, usage 1), 65-byte output reports (report id 0x00) each answered by an input report.
// Start: the firmware query (41 00, then 01 02) whose answer picks the ANSI / ISO / JIS key
// table. Every frame: 41 02 (manual LED control), then eight C0 02 packets of 16 colours.
// Shutdown: 41 00, which hands the lights back to the keyboard's firmware.
// Never sent: the profile save (50 55), profile or effect writes (51 ...), anything else.
//
// The V2-protocol Cooler Master keyboards OpenRGB also drives (SK6xx, CK5xx, MK730, MK750) are
// held back: see the hardware notes.

/* global device */

import { LAYOUTS } from "./layouts.js";

const PRODUCTS = [
  { pid: 0x003C, name: "Cooler Master MasterKeys Pro S", table: "PRO_S" },
  { pid: 0x003B, name: "Cooler Master MasterKeys Pro L", table: "PRO_L" },
  // CMKeyboardDevices.cpp gives the Pro L White the Pro S (tenkeyless) key table.
  { pid: 0x0047, name: "Cooler Master MasterKeys Pro L White", table: "PRO_S" },
];

const WRITE_SIZE = 65;       // CM_KEYBOARD_WRITE_SIZE
// OpenRGB's SendCommand waits for the answer with a blocking hid_read; HueDeck waits at most
// CM_KEYBOARD_TIMEOUT (50 ms).
const READ_TIMEOUT_MS = 50;
const MODE_FIRMWARE = 0x00;  // cm_keyboard_control_mode
const MODE_MANUAL = 0x02;    // SetLEDControl(true) on V1
const DIRECT_SLOTS = 128;    // 8 packets x 16 colours
const KEEPALIVE_MS = 1000;

const DEFAULT = LAYOUTS.PRO_S.ANSI;

export function Name() { return "Cooler Master MasterKeys"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/coolermaster-masterkeys"; }
export function VendorId() { return 0x2516; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
// Placeholders (Pro S, ANSI); Initialize() sets the keyboard's own table.
export function Size() { return [DEFAULT.width, DEFAULT.height]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return DEFAULT.leds.map((l) => l[0]); }
export function LedPositions() { return DEFAULT.leds.map((l) => [l[1], l[2]]); }
export function ImageUrl() { return ""; }
// Cooler Master's own software drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["MasterPlus.exe", "MasterPlusApp.exe"]; }

let product = null;
let layout = null;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IPU(..., 1, 0xFF00, 1).
export function Validate(endpoint) {
  return endpoint.interface === 1 && endpoint.usage_page === 0xFF00 && endpoint.usage === 0x0001;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  layout = null;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (!product) {
    device.log(`Cooler Master MasterKeys: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  // CMKeyboardV1Controller::_GetFirmwareVersion (the constructor) and GetKeyboardLayout.
  setControlMode(MODE_FIRMWARE);
  const answer = sendCommand([0x01, 0x02]);
  const firmware = cString(answer, 4);
  const region = firmware.length === 0 ? "ANSI" : ({ 2: "ISO", 3: "JIS" }[firmware[0]] ?? "ANSI");
  layout = LAYOUTS[product.table][region];
  device.log(`Cooler Master MasterKeys: firmware "${firmware}", ${region} key table.`);
  device.setName(product.name);
  device.setSize([layout.width, layout.height]);
  device.setControllableLeds(layout.leds.map((l) => l[0]), layout.leds.map((l) => [l[1], l[2]]));
}

export function Render() {
  if (!layout) {
    return;
  }
  const colours = layout.leds.map((l) => device.color(l[1], l[2]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (setLeds(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step (CMKeyboardV1Controller::Shutdown is empty). HueDeck hands the
// lights back to the keyboard's firmware with SetControlMode(MODE_FIRMWARE), the same command
// OpenRGB's firmware query starts with; the keyboard then shows its own (stored) lighting.
export function Shutdown(SystemSuspending) {
  if (!layout) {
    return;
  }
  setControlMode(MODE_FIRMWARE);
}

// CMKeyboardV1Controller::SetLeds: manual control, then colours by LED value, 16 per packet.
function setLeds(colours) {
  let ok = setControlMode(MODE_MANUAL) !== null;
  const map = new Array(DIRECT_SLOTS * 3).fill(0);
  layout.leds.forEach((led, i) => {
    const slot = led[3];
    if (slot < DIRECT_SLOTS) {
      map[slot * 3] = colours[i][0] & 0xFF;
      map[slot * 3 + 1] = colours[i][1] & 0xFF;
      map[slot * 3 + 2] = colours[i][2] & 0xFF;
    }
  });
  for (let i = 0; i < 8; i++) {
    const data = [0xC0, 0x02, (i * 2) & 0xFF, 0x00, ...map.slice(i * 48, i * 48 + 48)];
    ok = sendCommand(data) !== null && ok;
  }
  return ok;
}

// CMKeyboardAbstractController::SetControlMode: 41 <mode>.
function setControlMode(mode) {
  return sendCommand([0x41, mode]);
}

// CMKeyboardAbstractController::SendCommand: report id 0x00, the command, zeros to 65 bytes;
// then read the keyboard's answer. Returns the answer without the report id (hidapi's buffer),
// or null when the write failed.
function sendCommand(command) {
  const packet = new Array(WRITE_SIZE).fill(0);
  command.forEach((b, i) => { packet[1 + i] = b & 0xFF; });
  if (device.write(packet, WRITE_SIZE) === -1) {
    return null;
  }
  const answer = device.read([0x00], WRITE_SIZE, READ_TIMEOUT_MS) ?? [];
  const size = device.getLastReadSize();
  return Array.from(answer).slice(1, Math.max(1, size));
}

// The NUL-terminated string at `offset` of an answer.
function cString(bytes, offset) {
  let s = "";
  for (let i = offset; i < bytes.length && bytes[i] !== 0; i++) {
    s += String.fromCharCode(bytes[i]);
  }
  return s;
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
