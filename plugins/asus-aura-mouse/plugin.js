// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the ASUS ROG mice with a native direct lighting mode over USB (ROG Gladius III
// Wireless AimPoint, Chakram X, Spatha X, Strix Impact III, Keris Wireless AimPoint), per zone.
// Adapted from OpenRGB Controllers/AsusAuraUSBController/AsusAuraUSBControllerDetect.cpp,
// AsusAuraMouseController/AsusAuraMouseController.cpp and .h, AsusAuraMouseController/
// AsusAuraMouseDevices.h, AsusAuraMouseController/RGBController_AsusAuraMouse.cpp and .h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  23 Oct 2020" (AsusAuraMouseController);
// "Adam Honse (CalcProgrammer1)  25 Oct 2020" (RGBController_AsusAuraMouse); "Chris M (Dr_No)
// 11 Jul 2021" (AsusAuraMouseDevices); AsusAuraUSBControllerDetect.cpp names no author ("This file
// is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/asus-aura-mouse.md. In short: interface 0, usage page 0xFF01,
// 65-byte output reports with report id 0x00. Per frame: 00 51 29 <count> 00 <offset> R G B ...,
// up to 5 LEDs per report, each zone's colour on its LEDs. Never sent: the save command (00 50 03),
// hardware modes (00 51 28), DPI, polling rate or any other setting. The older mice without a
// direct mode (OpenRGB drives them through their Static hardware mode) are not claimed.

/* global device */

const VENDOR_ID = 0x0B05;
const REPORT_LENGTH = 65;     // ASUS_AURA_MOUSE_PACKET_SIZE
const LEDS_PER_PACKET = 5;
const KEEPALIVE_MS = 1000;    // an unchanged frame is sent again only after this long

// AsusAuraMouseDevices.h: zone names (aura_mouse_zone_names) and the LEDs each zone colours
// (aura_mouse_led_maps), for the devices whose mouse_type has `direct = true`.
const LOGO = { name: "Logo", cell: [0, 0] };
const SCROLL = { name: "Scroll Wheel", cell: [1, 0] };
const UNDERGLOW = { name: "Underglow", cell: [2, 0] };
const ONE_LOGO = { ledAmount: 1, zones: [{ ...LOGO, leds: [0] }] };
const SPATHA_X = { ledAmount: 5, zones: [{ ...LOGO, leds: [0] }, { ...SCROLL, leds: [1] }, { ...UNDERGLOW, leds: [2, 3, 4] }] };
const CHAKRAM_X = { ledAmount: 9, zones: [{ ...LOGO, leds: [7] }, { ...SCROLL, leds: [8] }, { ...UNDERGLOW, leds: [0, 1, 2, 3, 4, 5, 6] }] };
const IMPACT_III = { ledAmount: 2, zones: [{ ...LOGO, leds: [0] }, { ...SCROLL, leds: [1] }] };

// AsusAuraUSBControllerDetect.cpp order; every one REGISTER_HID_DETECTOR_IP(..., 0, 0xFF01).
const PRODUCTS = [
  { pid: 0x1A70, name: "ASUS ROG Gladius III Wireless AimPoint USB", map: ONE_LOGO },
  { pid: 0x1A72, name: "ASUS ROG Gladius III Wireless AimPoint 2.4Ghz", map: ONE_LOGO },
  { pid: 0x1A18, name: "Asus ROG Chakram X USB", map: CHAKRAM_X },
  { pid: 0x1A1A, name: "Asus ROG Chakram X 2.4GHz", map: CHAKRAM_X },
  { pid: 0x1977, name: "Asus ROG Spatha X USB", map: SPATHA_X },
  { pid: 0x1979, name: "Asus ROG Spatha X 2.4GHz", map: SPATHA_X },
  { pid: 0x1A88, name: "ASUS ROG Strix Impact III", map: IMPACT_III },
  { pid: 0x1A66, name: "ASUS ROG Keris Wireless AimPoint USB", map: ONE_LOGO },
  { pid: 0x1A68, name: "ASUS ROG Keris Wireless AimPoint 2.4Ghz", map: ONE_LOGO },
];

export function Name() { return "ASUS Aura Mouse"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/asus-aura-mouse"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
// Placeholders; Initialize() sets the product's zones.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return [LOGO.name]; }
export function LedPositions() { return [LOGO.cell.slice()]; }
export function ImageUrl() { return ""; }
// ASUS's own software drives the mouse while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() {
  return ["ArmouryCrate.exe", "ArmouryCrate.Service.exe", "LightingService.exe"];
}

let product = null;
let lastFrame = null;
let lastFrameAt = 0;

// REGISTER_HID_DETECTOR_IP(..., 0, 0xFF01).
export function Validate(endpoint) {
  return endpoint != null && endpoint.interface === 0 && Number(endpoint.usage_page) === 0xFF01;
}

export function Initialize() {
  lastFrame = null;
  lastFrameAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (product === null) {
    device.log(`ASUS Aura Mouse: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  const zones = product.map.zones;
  device.setName(product.name);
  device.setSize([zones.length, 1]);
  device.setControllableLeds(zones.map((z) => z.name), zones.map((z) => z.cell.slice()));
  // OpenRGB's start for these mice is GetVersion() (00 12 + read; information only, not sent
  // here); Direct needs no mode command (DeviceUpdateMode returns for it).
}

export function Render() {
  if (product === null) {
    return;
  }
  const frame = sendDirect(product.map.zones.map((z) => device.color(z.cell[0], z.cell[1])));
  const now = Date.now();
  if (lastFrame !== null && sameReports(frame, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
    return;
  }
  let ok = true;
  for (const report of frame) {
    ok = device.write(report, REPORT_LENGTH) !== -1 && ok;
  }
  lastFrame = ok ? frame : null;
  lastFrameAt = now;
}

// OpenRGB has no exit step; its other modes are hardware effects (00 51 28) and the mouse's own
// saved effect cannot be selected without them, so every zone is set black, on exit and suspend.
export function Shutdown(SystemSuspending) {
  if (product === null) {
    return;
  }
  for (const report of sendDirect(product.map.zones.map(() => [0, 0, 0]))) {
    device.write(report, REPORT_LENGTH);
  }
  lastFrame = null;
}

// AuraMouseController::SendDirect: zone colours onto the LED list, then 5 LEDs per report.
function sendDirect(zoneColours) {
  const map = product.map;
  const colours = Array.from({ length: map.ledAmount }, () => [0, 0, 0]);
  map.zones.forEach((zone, i) => {
    for (const led of zone.leds) {
      colours[led] = zoneColours[i];
    }
  });
  const reports = [];
  for (let led = 0; led < map.ledAmount; led += LEDS_PER_PACKET) {
    const count = map.ledAmount >= led + LEDS_PER_PACKET ? LEDS_PER_PACKET : map.ledAmount - led;
    const buf = new Array(REPORT_LENGTH).fill(0);
    buf[0x00] = 0x00;
    buf[0x01] = 0x51;
    buf[0x02] = 0x29;
    buf[0x03] = count;
    buf[0x04] = 0x00;
    buf[0x05] = led;
    for (let c = 0; c < count; c++) {
      buf[0x06 + c * 3] = colours[led + c][0] & 0xFF;
      buf[0x07 + c * 3] = colours[led + c][1] & 0xFF;
      buf[0x08 + c * 3] = colours[led + c][2] & 0xFF;
    }
    reports.push(buf);
  }
  return reports;
}

function sameReports(a, b) {
  if (a.length !== b.length) {
    return false;
  }
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
