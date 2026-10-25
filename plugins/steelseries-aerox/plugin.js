// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the lights of the SteelSeries Aerox 3, Aerox 5 and Aerox 9 mice (wired models,
// and the wireless models through their 2.4 GHz receiver or on the cable) and of the Rival 3
// Wireless, which OpenRGB drives with its Aerox wireless controller.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesAerox3Controller/
// SteelSeriesAerox3Controller.cpp and .h, SteelSeriesAerox5Controller/SteelSeriesAerox5Controller.cpp
// and .h, SteelSeriesAeroxWirelessController/SteelSeriesAeroxWirelessController.cpp and .h,
// SteelSeriesRival3Controller/RGBController_SteelSeriesRival3.cpp and .h,
// SteelSeriesMouseController/SteelSeriesMouseController.cpp and .h, SteelSeriesControllerDetect.cpp
// and SteelSeriesDevices.h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Chris M (Dr_No)  09 Jun 2022" (SteelSeriesAerox3Controller,
// SteelSeriesMouseController); "Bobby Quantum (BobbyQuantum)  19 May 2024"
// (SteelSeriesAerox5Controller); "Ensar S (esensar)  09 Sep 2024" and
// "Clari (clari7744) & JAO1988 (JAO1988)  02 Oct 2026" (SteelSeriesAeroxWirelessController);
// "B Horn (bahorn)  29 Aug 2021" (RGBController_SteelSeriesRival3); SteelSeriesControllerDetect.cpp
// and SteelSeriesDevices.h name no author.
//
// Protocol notes: docs/hardware/steelseries-aerox.md. In short: interface 3, collection
// 0xFFC0/0x0001, output reports with report id 0x00. Colour command 0x21 (0x26 for the reactive
// colour, 0x13 on the Rival 3 Wireless), brightness command 0x23 sent when the brightness
// changes. Through the receiver every command gets bit 0x40 and is followed by a read of the
// receiver's answer. Left out: OpenRGB's start-up report 0x2D, which sets the DPI (sensitivity)
// table, and its save command 0x11; DPI and polling rate are never touched.

/* global device */

const PRODUCTS = [
  { pid: 0x1838, name: "SteelSeries Aerox 3 Wireless", controller: "wireless", proto: "AEROX_3_WIRELESS" },
  { pid: 0x183A, name: "SteelSeries Aerox 3 Wireless Wired", controller: "wireless", proto: "AEROX_3_WIRELESS_WIRED" },
  { pid: 0x1878, name: "SteelSeries Aerox 3 CS2 Dragon Lore Edition Wireless", controller: "wireless", proto: "AEROX_3_WIRELESS" },
  { pid: 0x187A, name: "SteelSeries Aerox 3 CS2 Dragon Lore Edition Wireless Wired", controller: "wireless", proto: "AEROX_3_WIRELESS_WIRED" },
  { pid: 0x1890, name: "SteelSeries Aerox 3 Wireless Gen 2", controller: "wireless", proto: "AEROX_3_GEN2_WIRELESS" },
  { pid: 0x1836, name: "SteelSeries Aerox 3 Wired", controller: "aerox3" },
  { pid: 0x1852, name: "SteelSeries Aerox 5 Wireless", controller: "wireless", proto: "AEROX_5_WIRELESS" },
  { pid: 0x1854, name: "SteelSeries Aerox 5 Wireless Wired", controller: "wireless", proto: "AEROX_5_WIRELESS_WIRED" },
  { pid: 0x185C, name: "SteelSeries Aerox 5 Destiny 2 Edition Wireless", controller: "wireless", proto: "AEROX_5_DESTINY_WIRELESS" },
  { pid: 0x185E, name: "SteelSeries Aerox 5 Destiny 2 Edition Wireless Wired", controller: "wireless", proto: "AEROX_5_DESTINY_WIRELESS_WIRED" },
  { pid: 0x1860, name: "SteelSeries Aerox 5 Diablo IV Edition Wireless", controller: "wireless", proto: "AEROX_5_DIABLO_WIRELESS" },
  { pid: 0x1862, name: "SteelSeries Aerox 5 Diablo IV Edition Wireless Wired", controller: "wireless", proto: "AEROX_5_DIABLO_WIRELESS_WIRED" },
  { pid: 0x1850, name: "SteelSeries Aerox 5 Wired", controller: "aerox5" },
  { pid: 0x1858, name: "SteelSeries Aerox 9 Wireless", controller: "wireless", proto: "AEROX_9_WIRELESS" },
  { pid: 0x185A, name: "SteelSeries Aerox 9 Wireless Wired", controller: "wireless", proto: "AEROX_9_WIRELESS_WIRED" },
  { pid: 0x1830, name: "SteelSeries Rival 3 Wireless", controller: "wireless", proto: "RIVAL_3_WIRELESS" },
];

// SteelSeriesAeroxWirelessController::IsWireless(): the 2.4 GHz receiver ids.
const WIRELESS = new Set(["AEROX_3_WIRELESS", "AEROX_3_GEN2_WIRELESS", "AEROX_5_WIRELESS",
  "AEROX_5_DESTINY_WIRELESS", "AEROX_5_DIABLO_WIRELESS", "AEROX_9_WIRELESS"]);
const WIRELESS_FLAG = 0x40;
const WIRELESS_PACKET_SIZE = 64;
const WIRED_PACKET_SIZE = 65;
// OpenRGB waits up to STEELSERIES_AEROX_WIRELESS_TIMEOUT (250 ms) for the receiver's answer, on its
// own per-device thread. Render() runs on Lightloom's frame loop (about 33 ms per frame) and a frame
// is up to five commands, so Lightloom waits 20 ms at most: a sleeping or out-of-range mouse costs
// at most 100 ms per frame instead of 1.25 s. The answer is never looked at either way.
const READ_TIMEOUT_MS = 20;

// The steelseries_mouse tables (zone name, zone id), one LED per zone, sharing one grid row.
const ZONES = {
  aerox_3: [["Front", 0], ["Middle", 1], ["Rear", 2]],
  aerox_5: [["Front", 0], ["Middle", 1], ["Rear", 2], ["Reactive", 3]],
  aerox_9: [["Front", 0], ["Middle", 1], ["Rear", 2], ["Reactive", 3]],
  aerox_3_wireless: [["Front", 0], ["Middle", 1], ["Rear", 2]],
  rival_3_wireless: [["Scroll Wheel", 1]],
};

// OpenRGB's Direct mode: value STEELSERIES_MOUSE_EFFECT_DIRECT (0x04), speed 0, brightness
// STEELSERIES_MOUSE_BRIGHTNESS_MAX (0x64). Lightloom's colours already carry the brightness.
const EFFECT_DIRECT = 0x04;
const BRIGHTNESS = 0x64;
// Render() resends unchanged colours only after this long (a receiver answers every command).
const KEEPALIVE_MS = 1000;

const DEFAULT_LEDS = ledsOf(PRODUCTS[0]);

export function Name() { return "SteelSeries Aerox"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-aerox"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
// Placeholders (the first product's); Initialize() sets the real ones from device.productId().
export function Size() { return [DEFAULT_LEDS.length, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return DEFAULT_LEDS.map((l) => l.name); }
export function LedPositions() { return DEFAULT_LEDS.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let product = null;
let leds = [];
// current_brightness is never initialised in OpenRGB's three controllers, so their first
// SetColor() sends the brightness: null here means "unknown".
let currentBrightness = null;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IPU(..., 3, 0xFFC0, 1) for every product.
export function Validate(endpoint) {
  return endpoint.interface === 3 && endpoint.usage_page === 0xFFC0 && endpoint.usage === 0x0001;
}

export function Initialize() {
  currentBrightness = null;
  lastColours = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (!product) {
    leds = [];
    device.log(`SteelSeries Aerox: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  leds = ledsOf(product);
  device.setName(product.name);
  device.setSize([leds.length, 1]);
  device.setControllableLeds(leds.map((l) => l.name), leds.map((l) => l.cell.slice()));
  // OpenRGB's controllers send report 0x2D at start ("sets sensitivity and allows software
  // mode??": the DPI table); Lightloom leaves it out. Its firmware-version request (0x90) only
  // fills OpenRGB's device info and is left out too. The switch to Direct (DeviceUpdateMode ->
  // SetLightEffectAll(0x04)) sends nothing on these controllers.
  setLightEffectAll(EFFECT_DIRECT);
}

export function Render() {
  if (!product) {
    return;
  }
  const colours = leds.map((l) => device.color(l.cell[0], l.cell[1]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (updateLeds(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// Exit modes (Lightloom extension, docs/host-api/save-shutdown.md).
export function ShutdownModes() { return ["restore", "keep", "off"]; }

// OpenRGB has no exit step; its only other modes are hardware effects (rainbow breathing), not
// the mouse's own saved lighting: the lights are switched off (black, in Direct).
// Exit modes (Lightloom extension): "keep" sends the last frame once more with the same reports
// (never OpenRGB's Save()). OpenRGB sends no keepalive, so the device is expected to hold the
// colours until it loses power (unverified on hardware); nothing is stored. "off" is black;
// "restore" (and SignalRGB's call without a mode) is the exit above; suspend stays as it was.
export function Shutdown(SystemSuspending, mode) {
  if (!product) {
    return;
  }
  const keep = !SystemSuspending && mode === "keep";
  updateLeds(leds.map((l) => (keep ? device.color(l.cell[0], l.cell[1]) : [0, 0, 0])));
}

function ledsOf(p) {
  let table;
  if (p.controller === "aerox3") {
    table = ZONES.aerox_3;
  } else if (p.controller === "aerox5") {
    table = ZONES.aerox_5;
  } else if (p.proto === "AEROX_9_WIRELESS" || p.proto === "AEROX_9_WIRELESS_WIRED") {
    table = ZONES.aerox_9;
  } else if (p.proto === "RIVAL_3_WIRELESS") {
    table = ZONES.rival_3_wireless;
  } else {
    table = ZONES.aerox_3_wireless;
  }
  return table.map(([name, value], i) => ({ name, value, cell: [i, 0] }));
}

// RGBController_SteelSeriesRival3::DeviceUpdateLEDs: each zone's colour with the Direct mode's
// brightness, then DeviceUpdateMode (SetLightEffectAll(value - speed)).
function updateLeds(colours) {
  let ok = true;
  leds.forEach((led, i) => {
    const [r, g, b] = colours[i];
    ok = setColor(led.value, r & 0xFF, g & 0xFF, b & 0xFF, BRIGHTNESS) && ok;
  });
  return setLightEffectAll(EFFECT_DIRECT) && ok;
}

function setLightEffectAll(effect) {
  if (product.controller !== "wireless" || product.proto === "RIVAL_3_WIRELESS") {
    return true; // "Not used by this device" / ignored on the Rival 3 Wireless
  }
  if (effect === 0x05) {
    const buffer = new Array(WIRELESS_PACKET_SIZE).fill(0);
    buffer[1] = 0x22;
    buffer[2] = 0xFF;
    return writeBuffer(buffer);
  }
  return true;
}

function setColor(zoneId, r, g, b, brightness) {
  if (product.controller === "aerox3") {
    return aerox3SetColor(zoneId, r, g, b, brightness);
  }
  if (product.controller === "aerox5") {
    return aerox5SetColor(zoneId, r, g, b, brightness);
  }
  return wirelessSetColor(zoneId, r, g, b, brightness);
}

// SteelSeriesAerox3Controller::SetColor / SetBrightness.
function aerox3SetColor(zoneId, r, g, b, brightness) {
  const buffer = new Array(WIRED_PACKET_SIZE).fill(0);
  buffer[1] = 0x21;
  buffer[0x02] = (1 << zoneId) & 0xFF;
  const offset = 3 + zoneId * 3;
  buffer[offset] = r;
  buffer[offset + 1] = g;
  buffer[offset + 2] = b;
  let ok = device.write(buffer, WIRED_PACKET_SIZE) !== -1;
  if (brightness !== currentBrightness) {
    ok = writeWired([0x00, 0x23, brightness]) && ok;
    currentBrightness = brightness;
  }
  return ok;
}

// SteelSeriesAerox5Controller::SetColor / SetBrightness, each write followed by 2 ms.
function aerox5SetColor(zoneId, r, g, b, brightness) {
  const buffer = new Array(WIRED_PACKET_SIZE).fill(0);
  buffer[1] = 0x21;
  buffer[2] = 0x01;
  let offset = 0x03;
  if (zoneId === 1) {
    buffer[2] = 0x02;
    offset = 0x06;
  } else if (zoneId === 2) {
    buffer[2] = 0x04;
    offset = 0x09;
  } else if (zoneId === 3) {
    buffer[1] = 0x26; // reactive colour
    offset = 0x04;
  } else if (zoneId !== 0) {
    return true;
  }
  buffer[offset] = r;
  buffer[offset + 1] = g;
  buffer[offset + 2] = b;
  let ok = device.write(buffer, WIRED_PACKET_SIZE) !== -1;
  device.pause(2);
  if (brightness !== currentBrightness) {
    // OpenRGB's buffer here is 3 bytes long but written as 65: the bytes past it are
    // undefined in the C++; Lightloom sends zeros.
    ok = writeWired([0x00, 0x23, brightness]) && ok;
    device.pause(2);
    currentBrightness = brightness;
  }
  return ok;
}

// SteelSeriesAeroxWirelessController::SetColor / SetBrightness: ten brightness steps.
function wirelessSetColor(zoneId, r, g, b, brightness) {
  const buffer = new Array(WIRELESS_PACKET_SIZE).fill(0);
  if (product.proto === "RIVAL_3_WIRELESS") {
    buffer[1] = 0x13;
    buffer[2] = zoneId;
    buffer[3] = r;
    buffer[4] = g;
    buffer[5] = b;
  } else {
    buffer[1] = 0x21;
    buffer[2] = 0x01;
    if (zoneId === 3 && (product.proto === "AEROX_9_WIRELESS_WIRED" || product.proto === "AEROX_9_WIRELESS")) {
      buffer[0x03] = 0x00;
      buffer[0x01] = 0x26; // reactive colour
    } else {
      buffer[0x03] = zoneId;
    }
    buffer[0x04] = r;
    buffer[0x05] = g;
    buffer[0x06] = b;
  }
  let ok = writeBuffer(buffer);
  const steps = Math.floor(brightness / 10);
  if (steps !== currentBrightness) {
    const level = new Array(WIRELESS_PACKET_SIZE).fill(0);
    level[1] = 0x23;
    level[2] = steps;
    ok = writeBuffer(level) && ok;
    currentBrightness = steps;
  }
  return ok;
}

// SteelSeriesAeroxWirelessController::WriteBuffer: through the receiver, bit 0x40 on the command
// and a read of the receiver's answer (whose content OpenRGB does not look at).
function writeBuffer(buffer) {
  const wireless = WIRELESS.has(product.proto);
  if (wireless) {
    buffer[1] |= WIRELESS_FLAG;
  }
  const ok = device.write(buffer, WIRELESS_PACKET_SIZE) !== -1;
  if (wireless) {
    device.read([0x00], WIRELESS_PACKET_SIZE, READ_TIMEOUT_MS);
  }
  return ok;
}

function writeWired(bytes) {
  const buffer = new Array(WIRED_PACKET_SIZE).fill(0);
  bytes.forEach((v, i) => { buffer[i] = v; });
  return device.write(buffer, WIRED_PACKET_SIZE) !== -1;
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
