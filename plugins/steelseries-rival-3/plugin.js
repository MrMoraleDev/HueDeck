// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the four lights (front, middle, rear, logo) of the wired SteelSeries Rival 3.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesRival3Controller/
// SteelSeriesRival3Controller.cpp, SteelSeriesRival3Controller.h,
// RGBController_SteelSeriesRival3.cpp, RGBController_SteelSeriesRival3.h,
// Controllers/SteelSeriesController/SteelSeriesMouseController/SteelSeriesMouseController.cpp,
// SteelSeriesMouseController.h, and Controllers/SteelSeriesController/
// SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "B Horn (bahorn)  29 Aug 2021" (SteelSeriesRival3Controller,
// RGBController_SteelSeriesRival3); "Chris M (Dr_No)  09 Jun 2022" (SteelSeriesMouseController);
// SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h name no author.
//
// Protocol notes: docs/hardware/steelseries-rival-3.md. In short: interface 3, output reports
// with report id 0x00: per frame one 8-byte colour report (0x05) per zone, with the Direct mode's
// brightness byte, then the 4-byte effect report (0x06) selecting Direct (0x04). The save command
// (0x09) is never sent; DPI and polling rate are never touched.

/* global device */

const PRODUCTS = [
  { pid: 0x1824, name: "SteelSeries Rival 3 (Old Firmware)" },
  { pid: 0x184C, name: "SteelSeries Rival 3" },
];

// rival_3 in SteelSeriesRival3Controller.h: zones Front, Middle, Rear, Logo with the zone ids
// 0x01..0x04 (one LED each, sharing one grid row).
const LEDS = [
  { name: "Front", value: 0x01, cell: [0, 0] },
  { name: "Middle", value: 0x02, cell: [1, 0] },
  { name: "Rear", value: 0x03, cell: [2, 0] },
  { name: "Logo", value: 0x04, cell: [3, 0] },
];
const SIZE = [4, 1];
// OpenRGB's Direct mode: value STEELSERIES_MOUSE_EFFECT_DIRECT (0x04), speed 0, brightness
// STEELSERIES_MOUSE_BRIGHTNESS_MAX (0x64). HueDeck's colours already carry the brightness.
const EFFECT_DIRECT = 0x04;
const BRIGHTNESS = 0x64;
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

export function Name() { return "SteelSeries Rival 3"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-rival-3"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
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

// REGISTER_HID_DETECTOR_I(..., 3) for both products: any collection of interface 3.
export function Validate(endpoint) {
  return endpoint.interface === 3;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  enabled = Boolean(product);
  if (!product) {
    device.log(`SteelSeries Rival 3: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(SIZE.slice());
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // OpenRGB's switch to Direct (DeviceUpdateMode): SetLightEffectAll(value - speed). OpenRGB's
  // firmware-version request (0x10) only fills its device info and is left out.
  setLightEffectAll(EFFECT_DIRECT);
}

export function Render() {
  if (!enabled) {
    return;
  }
  const colours = LEDS.map((l) => device.color(l.cell[0], l.cell[1]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (updateLeds(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step and no command in it that hands the mouse its own lighting back
// without choosing an effect: the lights are switched off (black, in Direct).
export function Shutdown(SystemSuspending) {
  if (!enabled) {
    return;
  }
  updateLeds(LEDS.map(() => [0, 0, 0]));
}

// RGBController_SteelSeriesRival3::DeviceUpdateLEDs: each zone's colour, then DeviceUpdateMode.
function updateLeds(colours) {
  let ok = true;
  LEDS.forEach((led, i) => {
    const [r, g, b] = colours[i];
    ok = setColor(led.value, r & 0xFF, g & 0xFF, b & 0xFF, BRIGHTNESS) && ok;
  });
  return setLightEffectAll(EFFECT_DIRECT) && ok;
}

// SteelSeriesRival3Controller::SetLightEffectAll: 4 bytes.
function setLightEffectAll(effect) {
  return device.write([0x00, 0x06, 0x00, effect], 4) !== -1;
}

// SteelSeriesRival3Controller::SetColor: 8 bytes.
function setColor(zoneId, r, g, b, brightness) {
  return device.write([0x00, 0x05, 0x00, zoneId, r, g, b, brightness], 8) !== -1;
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
