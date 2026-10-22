// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the five lighting zones (QWERTY, ten-key, function keys, MX keys, logo) of the
// original SteelSeries Apex / Apex Fnatic and the Apex 350 keyboards.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesOldApexController/
// SteelSeriesOldApexController.cpp, SteelSeriesOldApexController.h,
// RGBController_SteelSeriesOldApex.cpp, RGBController_SteelSeriesOldApex.h,
// Controllers/SteelSeriesController/color32.h, SteelSeriesControllerDetect.cpp and
// SteelSeriesDevices.h (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Based on findings in ApexCtl by Audrius/tuxmark5, et. al,
// https://github.com/tuxmark5/ApexCtl", "David Lee (RAMChYLD)  15 Nov 2020" and "Based on work by
// B Horn (bahorn)  13 May 2020" (SteelSeriesOldApexController, RGBController_SteelSeriesOldApex);
// "David Lee (RAMChYLD)  15 Nov 2020" (color32.h); SteelSeriesControllerDetect.cpp and
// SteelSeriesDevices.h name no author.
//
// Protocol notes: docs/hardware/steelseries-apex-old.md. In short: interface 0, one 33-byte output
// report (report id 0x00, command 0x07, "all zones") carrying the five zones as R, G, B and a
// fourth byte OpenRGB fills with its Direct mode value 0x08 (brightness).

/* global device */

const PRODUCTS = [
  { pid: 0x1202, name: "SteelSeries Apex (OG)/Apex Fnatic" },
  { pid: 0x1206, name: "SteelSeries Apex 350" },
];

// RGBController_SteelSeriesOldApex::SetupZones: five one-LED zones, in packet order (one grid
// row).
const LEDS = ["QWERTY", "TenKey", "FunctionKeys", "MXKeys", "Logo"].map((name, i) => ({ name, cell: [i, 0] }));
const SIZE = [5, 1];
// STEELSERIES_OLDAPEX_DIRECT: the Direct mode's value, sent as each zone's fourth byte.
const DIRECT_ALPHA = 0x08;
const REPORT_LENGTH = 33;
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

export function Name() { return "SteelSeries Apex (original) / Apex 350"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-apex-old"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
export function Size() { return SIZE.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LEDS.map((l) => l.name); }
export function LedPositions() { return LEDS.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let enabled = false;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_I(..., 0) for both products: any collection of interface 0.
export function Validate(endpoint) {
  return endpoint.interface === 0;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  enabled = Boolean(product);
  if (!product) {
    device.log(`SteelSeries Apex (old): product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(SIZE.slice());
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // OpenRGB's only mode, Direct, has no command of its own (DeviceUpdateMode ->
  // DeviceUpdateLEDs): the first Render() sends the colours.
}

export function Render() {
  if (!enabled) {
    return;
  }
  const colours = LEDS.map((l) => {
    const [r, g, b] = device.color(l.cell[0], l.cell[1]);
    return [r & 0xFF, g & 0xFF, b & 0xFF];
  });
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (setColorDetailed(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step and no other mode for these keyboards: the zones are switched off
// (black, in Direct).
export function Shutdown(SystemSuspending) {
  if (!enabled) {
    return;
  }
  setColorDetailed(LEDS.map(() => [0, 0, 0]));
}

// RGBController_SteelSeriesOldApex::DeviceUpdateLEDs -> SteelSeriesOldApexController::
// SetColorDetailed: a 32-byte buffer behind report id 0x00 (send_usb_msg, 33 bytes).
function setColorDetailed(colours) {
  const buf = new Array(32).fill(0);
  buf[0x00] = 0x07;
  buf[0x01] = 0x00; // all zones
  colours.forEach(([r, g, b], i) => {
    const offset = 0x02 + i * 4;
    buf[offset] = r;
    buf[offset + 1] = g;
    buf[offset + 2] = b;
    buf[offset + 3] = DIRECT_ALPHA;
  });
  const pkt = [0x00, ...buf];
  return device.write(pkt, REPORT_LENGTH) !== -1;
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
