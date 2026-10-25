// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the lights of the older SteelSeries Rival mice (Rival 100, 105, 106 and 110;
// the Rival 300 family; Rival 600; Rival 700 and 710), per LED. The Rival 650 / 650 Wireless,
// which OpenRGB's driver also covers, are held back (see below).
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesRivalController/
// SteelSeriesRivalController.cpp, SteelSeriesRivalController.h, RGBController_SteelSeriesRival.cpp,
// RGBController_SteelSeriesRival.h, and Controllers/SteelSeriesController/
// SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "B Horn (bahorn)  13 May 2020" (SteelSeriesRivalController,
// RGBController_SteelSeriesRival); SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h name
// no author ("This file is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/steelseries-rival.md. In short: interface 0, five sub-protocols
// picked by product id. Rival 100 and 300: 10-byte output reports (0x07 effect, 0x05 / 0x08
// colour). Rival 600: one 7-byte output report per LED whose first byte (the report id) is 0x1C.
// Rival 700: one 579-byte feature report per LED. OpenRGB's save command (0x09) is never sent;
// DPI and polling rate are never touched.
//
// Held back: Rival 650 (1038:172B) and Rival 650 Wireless (1038:1726). OpenRGB's colour update
// for them sends, per LED, four reports it does not explain: 03 00 00 00 30 ..., 03 00 30 00 2C,
// 05 00 <zone> FF ..., 1C 00 55 00 46. Opcode 0x03 is the sensitivity (DPI) command in other Rival
// protocols, and the first two read like a write of 0x2C bytes at offset 0x30 of some memory.
// Until that is understood on hardware, this plugin does not claim those ids.

/* global device */

const PRODUCTS = [
  { pid: 0x1702, name: "SteelSeries Rival 100", model: "RIVAL_100" },
  { pid: 0x170C, name: "SteelSeries Rival 100 DotA 2 Edition", model: "RIVAL_100" },
  { pid: 0x1814, name: "SteelSeries Rival 105", model: "RIVAL_100" },
  { pid: 0x1816, name: "SteelSeries Rival 106", model: "RIVAL_100" },
  { pid: 0x1729, name: "SteelSeries Rival 110", model: "RIVAL_100" },
  // OpenRGB also registers 0x1710 as "SteelSeries Rival 300 Black Ops Edition"; its detector
  // loop stops at the first registration that creates a controller, which is this one.
  { pid: 0x1710, name: "SteelSeries Rival 300", model: "RIVAL_300" },
  { pid: 0x1714, name: "Acer Predator Gaming Mouse (Rival 300)", model: "RIVAL_300" },
  { pid: 0x1394, name: "SteelSeries Rival 300 CS:GO Fade Edition", model: "RIVAL_300" },
  { pid: 0x1716, name: "SteelSeries Rival 300 CS:GO Fade Edition (stm32)", model: "RIVAL_300" },
  { pid: 0x171A, name: "SteelSeries Rival 300 CS:GO Hyperbeast Edition", model: "RIVAL_300" },
  { pid: 0x1392, name: "SteelSeries Rival 300 Dota 2 Edition", model: "RIVAL_300" },
  { pid: 0x1718, name: "SteelSeries Rival 300 HP Omen Edition", model: "RIVAL_300" },
  { pid: 0x1724, name: "SteelSeries Rival 600", model: "RIVAL_600" },
  { pid: 0x172E, name: "SteelSeries Rival 600 Dota 2 Edition", model: "RIVAL_600" },
  { pid: 0x1700, name: "SteelSeries Rival 700", model: "RIVAL_700" },
  { pid: 0x1730, name: "SteelSeries Rival 710", model: "RIVAL_700" },
];

// RGBController_SteelSeriesRival::SetupZones: LEDs in OpenRGB order with their led.value (the
// zone id the controller sends). Grid: one-LED zones (Logo, Scroll Wheel) share row 0, the
// six-LED "Mouse" zone is row 1.
const LOGO_AND_WHEEL = [
  { name: "Logo", value: 0, cell: [0, 0] },
  { name: "Scroll Wheel", value: 1, cell: [1, 0] },
];
const MODELS = {
  RIVAL_100: { size: [1, 1], leds: [{ name: "Logo", value: 0, cell: [0, 0] }] },
  RIVAL_300: { size: [2, 1], leds: LOGO_AND_WHEEL },
  RIVAL_700: { size: [2, 1], leds: LOGO_AND_WHEEL },
  RIVAL_600: {
    size: [6, 2],
    leds: [
      { name: "Logo", value: 0x01, cell: [0, 0] },
      { name: "Scroll Wheel", value: 0x00, cell: [1, 0] },
      { name: "Left top", value: 0x02, cell: [0, 1] },
      { name: "Left mid", value: 0x04, cell: [1, 1] },
      { name: "Left bottom", value: 0x06, cell: [2, 1] },
      { name: "Right top", value: 0x03, cell: [3, 1] },
      { name: "Right mid", value: 0x05, cell: [4, 1] },
      { name: "Right bottom", value: 0x07, cell: [5, 1] },
    ],
  },
};

// STEELSERIES_RIVAL_EFFECT_DIRECT, the effect OpenRGB's Direct mode selects.
const EFFECT_DIRECT = 0x01;
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

const DEFAULT_MODEL = MODELS[PRODUCTS[0].model];

export function Name() { return "SteelSeries Rival"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-rival"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
// Placeholders (the first product's); Initialize() sets the real ones from device.productId().
export function Size() { return DEFAULT_MODEL.size.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return DEFAULT_MODEL.leds.map((l) => l.name); }
export function LedPositions() { return DEFAULT_MODEL.leds.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let product = null;
let model = null;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_I(..., 0) for every product: any collection of interface 0.
export function Validate(endpoint) {
  return endpoint.interface === 0;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  model = product ? MODELS[product.model] : null;
  if (!model) {
    device.log(`SteelSeries Rival: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(model.size.slice());
  device.setControllableLeds(model.leds.map((l) => l.name), model.leds.map((l) => l.cell.slice()));
  // OpenRGB's switch to Direct (DeviceUpdateMode): SetLightEffectAll(EFFECT_DIRECT); its
  // DeviceUpdateLEDs follows in the first Render().
  setLightEffectAll(product.model, EFFECT_DIRECT);
}

export function Render() {
  if (!model) {
    return;
  }
  const colours = model.leds.map((l) => device.color(l.cell[0], l.cell[1]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (updateLeds(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step and these mice have no "back to onboard lighting" command in it:
// the lights are switched off (black, in Direct).
// Exit modes (Lightloom extension): "keep" sends the last frame once more with the same reports.
// OpenRGB sends no keepalive, so the device is expected to hold the colours until it loses power
// (unverified on hardware); nothing is stored. "off" is black; "restore" (and SignalRGB's call
// without a mode) is the exit above; suspend stays as it was.
export function Shutdown(SystemSuspending, mode) {
  if (!model) {
    return;
  }
  const keep = !SystemSuspending && mode === "keep";
  updateLeds(model.leds.map((l) => (keep ? device.color(l.cell[0], l.cell[1]) : [0, 0, 0])));
}

// Exit modes (Lightloom extension, docs/host-api/save-shutdown.md).
export function ShutdownModes() { return ["restore", "keep", "off"]; }

// RGBController_SteelSeriesRival::DeviceUpdateLEDs: every LED, in OpenRGB order.
function updateLeds(colours) {
  let ok = true;
  model.leds.forEach((led, i) => {
    const [r, g, b] = colours[i];
    ok = setColor(product.model, led.value, r & 0xFF, g & 0xFF, b & 0xFF) && ok;
  });
  return ok;
}

// SteelSeriesRivalController::SetLightEffectAll / SetLightEffect.
function setLightEffectAll(proto, effect) {
  if (proto === "RIVAL_100") {
    setLightEffect(proto, 0, effect);
  } else if (proto === "RIVAL_300") {
    setLightEffect(proto, 0, effect);
    setLightEffect(proto, 1, effect);
  }
  // RIVAL_600 and RIVAL_700: nothing (OpenRGB's switch has no case for them).
}

function setLightEffect(proto, zoneId, effect) {
  const buf = new Array(9).fill(0);
  if (proto === "RIVAL_100") {
    buf[0x00] = 0x07;
    buf[0x01] = 0x00;
  } else if (proto === "RIVAL_300") {
    buf[0x00] = 0x07;
    buf[0x01] = (zoneId + 1) & 0xFF;
  } else if (proto === "RIVAL_700") {
    return true;
  }
  buf[0x02] = effect;
  return sendUsbMsg(buf);
}

// SteelSeriesRivalController::SetColor.
function setColor(proto, zoneId, r, g, b) {
  const buf = new Array(9).fill(0);
  if (proto === "RIVAL_100") {
    buf[0x00] = 0x05;
    buf[0x01] = 0x00;
  } else if (proto === "RIVAL_300") {
    buf[0x00] = 0x08;
    buf[0x01] = (zoneId + 1) & 0xFF;
  } else if (proto === "RIVAL_600") {
    return setRival600Color(zoneId, r, g, b);
  } else if (proto === "RIVAL_700") {
    return setRival700Color(zoneId, r, g, b);
  }
  buf[0x02] = r;
  buf[0x03] = g;
  buf[0x04] = b;
  return sendUsbMsg(buf);
}

// Written as is (no 0x00 prefix): hidapi takes byte 0, 0x1C, as the report id.
function setRival600Color(zoneId, r, g, b) {
  const pkt = [0x1C, 0x27, 0x00, (1 << zoneId) & 0xFF, r, g, b];
  return device.write(pkt, 7) !== -1;
}

// A 578-byte buffer behind a 0x00 report id: one 579-byte feature report.
function setRival700Color(zoneId, r, g, b) {
  const pkt = new Array(579).fill(0);
  pkt[0x01] = 0x05;
  pkt[0x03] = zoneId;
  pkt[0x04] = r;
  pkt[0x05] = g;
  pkt[0x06] = b;
  pkt[0x0C] = zoneId;
  pkt[0x0D] = 0x01;
  return device.send_report(pkt, 579) !== -1;
}

// send_usb_msg(): report id 0x00, then the buffer; size + 1 bytes.
function sendUsbMsg(data) {
  const pkt = [0x00, ...data];
  return device.write(pkt, pkt.length) !== -1;
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
