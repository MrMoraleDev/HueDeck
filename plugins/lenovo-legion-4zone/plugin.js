// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the four-zone keyboard backlight of Lenovo Legion 5 / IdeaPad Gaming laptops
// (ITE controller 048D, usage page 0xFF89, usage 0xCC).
// Adapted from OpenRGB Controllers/LenovoControllers/Lenovo4ZoneUSBController/
// Lenovo4ZoneUSBController.cpp and .h, LenovoDevices4Zone.h, RGBController_Lenovo4ZoneUSB.cpp and
// .h, Lenovo4ZoneUSBControllerDetect.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: these files carry no author line ("This file is part of the OpenRGB
// project").
//
// Protocol notes: docs/hardware/lenovo-legion-4zone.md. In short: one 33-byte feature report,
// report id 0xCC: CC 16 <effect> <speed> <brightness> R G B x4 00 <wave ltr> <wave rtl>, zeros.
// Direct is the static effect (1) with each zone's colour. Nothing else is ever sent.

/* global device, keyboardBrightness:readonly */

const VENDOR_ID = 0x048D;
const USAGE_PAGE = 0xFF89;
const USAGE = 0xCC;

// Lenovo4ZoneUSBControllerDetect.cpp, in registration order. 0xC963 is registered twice
// ("Lenovo Ideapad 3-15ach6" and "Lenovo 5 2021 Ideapad"); it is listed once, with the first name.
const PRODUCTS = [
  { pid: 0xC963, name: "Lenovo Ideapad 3-15ach6" },
  { pid: 0xC985, name: "Lenovo 5 2023" },
  { pid: 0xC984, name: "Lenovo 5 2023 Ideapad" },
  { pid: 0xC975, name: "Lenovo 5 2022" },
  { pid: 0xC973, name: "Lenovo 5 2022 Ideapad" },
  { pid: 0xC965, name: "Lenovo 5 2021" },
  { pid: 0xC955, name: "Lenovo 5 2020" },
];

const PACKET_SIZE = 33; // LENOVO_4_ZONE_HID_PACKET_SIZE
// LENOVO_4_ZONE_EFFECT_* and _SPEED_*, _BRIGHTNESS_*.
const EFFECT_STATIC = 1;
const EFFECT_SMOOTH = 6;
const SPEED_SLOWEST = 1;
const SPEED_SLOW = 2;
const BRIGHTNESS_LOW = 1;
const BRIGHTNESS_HIGH = 2;

// lenovo_4_zone_leds, left to right.
const LED_NAMES = ["Left side", "Left center", "Right center", "Right side"];
const LED_POSITIONS = [[0, 0], [1, 0], [2, 0], [3, 0]];
const KEEPALIVE_MS = 1000;

export function Name() { return "Lenovo Legion 4-Zone Keyboard"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/lenovo-legion-4zone"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
export function Size() { return [4, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LED_NAMES.slice(); }
export function LedPositions() { return LED_POSITIONS.map((p) => p.slice()); }
export function ImageUrl() { return ""; }
// Lenovo's own software drives the backlight while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["LegionSpace.exe", "LegionZone.exe", "LenovoVantage.exe", "LenovoVantageService.exe"]; }
export function ControllableParameters() {
  return [
    { property: "keyboardBrightness", group: "lighting", label: "Backlight Level",
      description: "The keyboard's own brightness step (OpenRGB's Direct brightness, 1 = Low or 2 = High).",
      type: "combobox", values: ["High", "Low"], default: "High" },
  ];
}

let product = null;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_PU(..., 0xFF89, 0xCC): that usage page and usage, any interface.
export function Validate(endpoint) {
  return endpoint != null && endpoint.usage_page === USAGE_PAGE && endpoint.usage === USAGE;
}

// OpenRGB's controller sends nothing when it is created.
export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (!product) {
    device.log(`Lenovo 4-Zone: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
}

export function Render() {
  if (!product) {
    return;
  }
  const colours = LED_POSITIONS.map(([x, y]) => device.color(x, y));
  const brightness = brightnessStep();
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && lastColours.brightness === brightness
      && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  // RGBController_Lenovo4ZoneUSB::DeviceUpdateLEDs: state.SetColors(colors); setMode(state).
  if (setMode(EFFECT_STATIC, SPEED_SLOWEST, brightness, colours)) {
    lastColours = colours;
    lastColours.brightness = brightness;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step. The keyboard has hardware effects that OpenRGB selects with the same
// report (no save command exists): it is handed back to Spectrum Cycle (effect 6) at Slow speed,
// as DeviceUpdateMode would send it (zone colours = the last frame's, unused by that effect).
export function Shutdown(SystemSuspending) {
  if (!product) {
    return;
  }
  const colours = lastColours ?? LED_POSITIONS.map(() => [0xFF, 0xFF, 0xFF]);
  setMode(EFFECT_SMOOTH, SPEED_SLOW, brightnessStep(), colours);
  lastColours = null;
}

// Lenovo4ZoneUSBController::setMode(KeyboardState): uint8_t buffer[33] = { ... }, rest zero.
function setMode(effect, speed, brightness, colours) {
  const buf = new Array(PACKET_SIZE).fill(0);
  buf[0] = 0xCC;
  buf[1] = 0x16;
  buf[2] = effect;
  buf[3] = speed;
  buf[4] = brightness;
  colours.forEach(([r, g, b], i) => {
    buf[5 + i * 3] = r & 0xFF;
    buf[6 + i * 3] = g & 0xFF;
    buf[7 + i * 3] = b & 0xFF;
  });
  buf[17] = 0x00;
  buf[18] = 0; // wave_ltr: only the Rainbow Wave effect sets it
  buf[19] = 0; // wave_rtl
  return device.send_report(buf, PACKET_SIZE) !== -1;
}

function brightnessStep() {
  const value = typeof keyboardBrightness === "undefined" ? "High" : keyboardBrightness;
  return value === "Low" ? BRIGHTNESS_LOW : BRIGHTNESS_HIGH;
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
