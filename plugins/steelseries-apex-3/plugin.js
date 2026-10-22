// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the zone lighting of the SteelSeries Apex 3 (ten zones) and Apex 3 TKL (eight
// zones) keyboards.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesApex3Controller/
// SteelSeriesApex3Controller.cpp, SteelSeriesApex3Controller.h, RGBController_SteelSeriesApex3.cpp,
// RGBController_SteelSeriesApex3.h, SteelSeriesApexTZoneController/
// SteelSeriesApexTZoneController.cpp and .h, SteelSeriesApex8ZoneController/
// SteelSeriesApex8ZoneController.cpp and .h, and Controllers/SteelSeriesController/
// SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Chris M (Dr_No)  23 Feb 2022" (SteelSeriesApex3Controller,
// RGBController_SteelSeriesApex3, SteelSeriesApex8ZoneController); "Paul K. Gerke  27 Oct 2022"
// (SteelSeriesApex8ZoneController); "Edbgon  06 Oct 2021" (SteelSeriesApexTZoneController);
// SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h name no author.
//
// Protocol notes: docs/hardware/steelseries-apex-3.md. In short, two sub-protocols, output
// reports with report id 0x00:
//   Apex 3 (ten zones, interface 3): 33-byte reports, 0x0A brightness then 0x0B colours, both on
//   every update.
//   Apex 3 TKL (eight zones, interface 1, collection 0xFFC0/0x0001): 65-byte reports, 0x21 FF with
//   the colours, 0x23 brightness when it changes; 0x22 FF (the keyboard's own rainbow wave) at
//   shutdown.
// The Apex 3 save command (0x06 / 0x09) is never sent.

/* global device */

const PRODUCTS = [
  { pid: 0x161A, name: "SteelSeries Apex 3", model: "APEX_TZONE" },
  { pid: 0x1622, name: "SteelSeries Apex 3 TKL", model: "APEX_8ZONE" },
];

// RGBController_SteelSeriesApex3::SetupZones: one linear zone "Keyboard" of GetLedCount() LEDs
// named "LED <i>" (one grid row). Brightness: the Direct mode's, GetMaxBrightness(); Lightloom's
// colours already carry the brightness.
const MODELS = {
  APEX_TZONE: { leds: 10, brightness: 0x64, length: 33 },
  APEX_8ZONE: { leds: 8, brightness: 0x10, length: 65 },
};
// APEX3_MODES::DIRECT and RAINBOW_WAVE (mode values; the TKL adds them to command 0x21).
const MODE_DIRECT = 0;
const MODE_RAINBOW_WAVE = 1;
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

const DEFAULT_LEDS = ledsOf(MODELS.APEX_TZONE);

export function Name() { return "SteelSeries Apex 3"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-apex-3"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
// Placeholders (the Apex 3's); Initialize() sets the real ones from device.productId().
export function Size() { return [DEFAULT_LEDS.length, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return DEFAULT_LEDS.map((l) => l.name); }
export function LedPositions() { return DEFAULT_LEDS.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let product = null;
let model = null;
let leds = [];
// SteelSeriesApex8ZoneController::current_brightness is never initialised in OpenRGB, so its
// first SetColor() sends the brightness: null here means "unknown".
let currentBrightness = null;
let lastColours = null;
let lastSentAt = 0;

// Apex 3: REGISTER_HID_DETECTOR_I(..., 3), any collection of interface 3.
// Apex 3 TKL: REGISTER_HID_DETECTOR_IPU(..., 1, 0xFFC0, 1).
// The host binds device.productId() before it calls Validate (crates/lightloom-plugin-host
// session.rs, steps 2 and 3), so each product takes only its own detector line; an unknown
// product id takes nothing.
export function Validate(endpoint) {
  const pid = device.productId();
  if (pid === 0x161A) {
    return endpoint.interface === 3;
  }
  if (pid === 0x1622) {
    return endpoint.interface === 1 && endpoint.usage_page === 0xFFC0 && endpoint.usage === 0x0001;
  }
  return false;
}

export function Initialize() {
  currentBrightness = null;
  lastColours = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  model = product ? MODELS[product.model] : null;
  if (!model) {
    leds = [];
    device.log(`SteelSeries Apex 3: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  leds = ledsOf(model);
  device.setName(product.name);
  device.setSize([leds.length, 1]);
  device.setControllableLeds(leds.map((l) => l.name), leds.map((l) => l.cell.slice()));
  // OpenRGB's switch to Direct (RGBController_SteelSeriesApex3::DeviceUpdateMode): its test
  // compares color_mode (MODE_COLORS_PER_LED, 1) with the flag MODE_FLAG_HAS_PER_LED_COLOR
  // (1 << 5), which never match, so it calls SetColor(modes[active].colors, ...) with the Direct
  // mode's empty colour list. The colours follow in the first Render().
  setColor([], MODE_DIRECT, model.brightness);
}

export function Render() {
  if (!model) {
    return;
  }
  const colours = leds.map((l) => {
    const [r, g, b] = device.color(l.cell[0], l.cell[1]);
    return [r & 0xFF, g & 0xFF, b & 0xFF];
  });
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  // RGBController_SteelSeriesApex3::DeviceUpdateLEDs.
  if (setColor(colours, MODE_DIRECT, model.brightness)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step. The Apex 3 TKL gets its own rainbow wave back (OpenRGB's "Rainbow
// Wave" mode, the keyboard's power-on lighting); OpenRGB has no such command for the Apex 3, whose
// zones are switched off (black, in Direct).
export function Shutdown(SystemSuspending) {
  if (!model) {
    return;
  }
  if (product.model === "APEX_8ZONE") {
    setColor([], MODE_RAINBOW_WAVE, model.brightness);
  } else {
    setColor(leds.map(() => [0, 0, 0]), MODE_DIRECT, model.brightness);
  }
}

function ledsOf(m) {
  return Array.from({ length: m.leds }, (_, i) => ({ name: `LED ${i}`, cell: [i, 0] }));
}

function setColor(colours, mode, brightness) {
  return product.model === "APEX_8ZONE"
    ? setColor8Zone(colours, mode, brightness)
    : setColorTZone(colours, brightness);
}

// SteelSeriesApexTZoneController::SetColor: one buffer, sent twice; the second report keeps the
// brightness byte at offset 3 unless a colour overwrites it.
function setColorTZone(colours, brightness) {
  const buf = new Array(MODELS.APEX_TZONE.length).fill(0);
  buf[0x01] = 0x0A;
  buf[0x03] = brightness;
  let ok = device.write(buf.slice(), buf.length) !== -1;
  buf[0x01] = 0x0B;
  colours.forEach(([r, g, b], i) => {
    const index = (i * 3) & 0xFF;
    buf[index + 3] = r;
    buf[index + 4] = g;
    buf[index + 5] = b;
  });
  ok = (device.write(buf, buf.length) !== -1) && ok;
  return ok;
}

// SteelSeriesApex8ZoneController::SetColor / SetBrightness.
function setColor8Zone(colours, mode, brightness) {
  const buf = new Array(MODELS.APEX_8ZONE.length).fill(0);
  buf[0x01] = (0x21 + mode) & 0xFF;
  buf[0x02] = 0xFF;
  colours.forEach(([r, g, b], i) => {
    const index = (i * 3) & 0xFF;
    buf[index + 3] = r;
    buf[index + 4] = g;
    buf[index + 5] = b;
  });
  let ok = device.write(buf, buf.length) !== -1;
  if (currentBrightness !== brightness) {
    const level = new Array(MODELS.APEX_8ZONE.length).fill(0);
    level[0x01] = 0x23;
    level[0x02] = brightness;
    ok = (device.write(level, level.length) !== -1) && ok;
    currentBrightness = brightness;
  }
  return ok;
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
