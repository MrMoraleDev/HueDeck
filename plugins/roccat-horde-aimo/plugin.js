// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the six lighting zones of the Roccat Horde Aimo keyboard (1E7D:303E), per zone.
// Adapted from OpenRGB Controllers/RoccatController/RoccatHordeAimoController/
// RoccatHordeAimoController.cpp, RoccatHordeAimoController.h, RGBController_RoccatHordeAimo.cpp and
// Controllers/RoccatController/RoccatControllerDetect.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Morgan Guimard (morg)  24 Feb 2022" (RoccatHordeAimoController,
// RGBController_RoccatHordeAimo); RoccatControllerDetect.cpp names no author.
//
// Protocol notes: docs/hardware/roccat-horde-aimo.md. In short: interface 1, usage page 0x0B,
// usage 0. Start: feature report 13 08 01 00 00 00 00 00. Every frame: one 24-byte feature report
// 18 18 FF FF, R G B for the six LEDs, then the 16-bit sum of bytes 0..21 (low byte first).
// OpenRGB's GET_FEATURE after each frame reads into an uninitialised buffer, so its report id is
// undefined; HueDeck leaves it out. Shutdown: black (OpenRGB has no exit step and knows no
// hardware mode for this keyboard). Nothing else is ever sent.

/* global device */

const PID = 0x303E;
const LEDS = Array.from({ length: 6 }, (_, i) => ({ name: `LED ${i + 1}`, cell: [i, 0] }));
/** Render() resends unchanged colours only after this long. */
const KEEPALIVE_MS = 1000;

export function Name() { return "Roccat Horde Aimo"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/roccat-horde-aimo"; }
export function VendorId() { return 0x1E7D; }
export function ProductId() { return [PID]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
export function Size() { return [6, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LEDS.map((l) => l.name); }
export function LedPositions() { return LEDS.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// Roccat's / Turtle Beach's own software drives the lights while it runs; HueDeck reports it.
export function ConflictingProcesses() {
  return ["ROCCAT_Swarm.exe", "ROCCAT_Swarm_Monitor.exe", "Turtle Beach Swarm II.exe"];
}

let active = false;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IPU(..., 1, 0x0B, 0): interface 1, usage page 0x0B, usage 0.
export function Validate(endpoint) {
  return endpoint.interface === 1 && endpoint.usage_page === 0x000B && endpoint.usage === 0x0000;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  active = device.productId() === PID;
  if (!active) {
    device.log(`Roccat Horde Aimo: product id 0x${(device.productId() & 0xFFFF).toString(16)} is not one this plugin knows.`);
    return;
  }
  device.setName("Roccat Horde Aimo");
  device.setSize([6, 1]);
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // RoccatHordeAimoController::InitialPacket.
  device.send_report([0x13, 0x08, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00], 8);
}

export function Render() {
  if (!active) return;
  const colours = LEDS.map((l) => device.color(l.cell[0], l.cell[1]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (setColors(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

export function Shutdown(SystemSuspending) {
  if (!active) return;
  setColors(LEDS.map(() => [0, 0, 0]));
}

// RoccatHordeAimoController::SetColors.
function setColors(colours) {
  const buf = new Array(24).fill(0);
  buf[0x00] = 0x18;
  buf[0x01] = 24;
  buf[0x02] = 0xFF;
  buf[0x03] = 0xFF;
  colours.forEach(([r, g, b], i) => {
    buf[0x04 + i * 3] = r & 0xFF;
    buf[0x05 + i * 3] = g & 0xFF;
    buf[0x06 + i * 3] = b & 0xFF;
  });
  let crc = 0;
  for (let i = 0; i < 22; i++) crc += buf[i];
  buf[22] = crc & 0xFF;
  buf[23] = (crc >> 8) & 0xFF;
  return device.send_report(buf, 24) !== -1;
}

function sameColours(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) return false;
  }
  return true;
}
