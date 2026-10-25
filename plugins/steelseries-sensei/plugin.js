// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the logo and scroll-wheel lights of the SteelSeries mice OpenRGB drives with
// its Sensei controller (Rival 310 and its editions, Sensei TEN and its edition, Sensei 310).
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesSenseiController/
// SteelSeriesSenseiController.cpp, SteelSeriesSenseiController.h,
// RGBController_SteelSeriesSensei.cpp, RGBController_SteelSeriesSensei.h, and
// Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Based on SteelSeries Rival controller / B Horn (bahorn)  13 May 2020"
// (SteelSeriesSenseiController, RGBController_SteelSeriesSensei); SteelSeriesControllerDetect.cpp
// and SteelSeriesDevices.h name no author.
//
// Protocol notes: docs/hardware/steelseries-sensei.md. In short: interface 0, one 66-byte output
// report (report id 0x00, command 0x5B) per zone and frame, logo (zone 0) then scroll wheel
// (zone 1). OpenRGB's save command (0x59) is never sent; DPI is never touched.

/* global device */

const PRODUCTS = [
  { pid: 0x1720, name: "SteelSeries Rival 310" },
  { pid: 0x171E, name: "SteelSeries Rival 310 CS:GO Howl Edition" },
  { pid: 0x1736, name: "SteelSeries Rival 310 PUBG Edition" },
  { pid: 0x1832, name: "SteelSeries Sensei TEN" },
  { pid: 0x1834, name: "SteelSeries Sensei TEN CS:GO Neon Rider Edition" },
  { pid: 0x1722, name: "SteelSeries Sensei 310" },
];

// RGBController_SteelSeriesSensei::SetupZones: zone 0 "Logo", zone 1 "Scroll Wheel" (one LED
// each, sharing one grid row).
const LEDS = [
  { name: "Logo", zone: 0, cell: [0, 0] },
  { name: "Scroll Wheel", zone: 1, cell: [1, 0] },
];
const SIZE = [2, 1];
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

export function Name() { return "SteelSeries Rival 310 / Sensei"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-sensei"; }
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
// SteelSeries' own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let enabled = false;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_I(..., 0) for every product: any collection of interface 0.
export function Validate(endpoint) {
  return endpoint.interface === 0;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  enabled = Boolean(product);
  if (!product) {
    device.log(`SteelSeries Sensei: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(SIZE.slice());
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // OpenRGB's Direct mode has no mode command of its own: DeviceUpdateMode only sends the
  // colours (DeviceUpdateLEDs), which the first Render() does.
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

// OpenRGB has no exit step and no command that gives these mice their own lighting back: the
// lights are switched off (black, in the static-colour command).
// Exit modes (Lightloom extension): "keep" sends the last frame once more with the same reports.
// OpenRGB sends no keepalive, so the device is expected to hold the colours until it loses power
// (unverified on hardware); nothing is stored. "off" is black; "restore" (and SignalRGB's call
// without a mode) is the exit above; suspend stays as it was.
export function Shutdown(SystemSuspending, mode) {
  if (!enabled) {
    return;
  }
  const keep = !SystemSuspending && mode === "keep";
  updateLeds(LEDS.map((l) => (keep ? device.color(l.cell[0], l.cell[1]) : [0, 0, 0])));
}

// Exit modes (Lightloom extension, docs/host-api/save-shutdown.md).
export function ShutdownModes() { return ["restore", "keep", "off"]; }

// DeviceUpdateLEDs: DeviceUpdateZoneLEDs(0), then (1); Direct mode -> SetColor(zone, r, g, b).
function updateLeds(colours) {
  let ok = true;
  LEDS.forEach((led, i) => {
    const [r, g, b] = colours[i];
    ok = setColor(led.zone, r & 0xFF, g & 0xFF, b & 0xFF) && ok;
  });
  return ok;
}

// SteelSeriesSenseiController::SetColor: a 65-byte buffer behind report id 0x00.
function setColor(zoneId, r, g, b) {
  const buf = new Array(65).fill(0);
  buf[0x00] = 0x5B; // command
  buf[0x02] = zoneId;
  buf[0x1C] = r;
  buf[0x1D] = g;
  buf[0x1E] = b;
  buf[0x1F] = r; // the original software repeats the colour
  buf[0x20] = g;
  buf[0x21] = b;
  buf[0x13] = 0x01; // static colour flag
  buf[0x1B] = 0x01; // number of colours
  const pkt = [0x00, ...buf];
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
