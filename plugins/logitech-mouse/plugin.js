// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the lights of the older wired Logitech G mice with fixed feature indices:
// G203 Prodigy, G Pro / G Pro HERO (one zone), G303 Daedalus Apex and G403 HERO (two zones), the
// G203 LIGHTSYNC (three LEDs) and the G600.
// Adapted from OpenRGB Controllers/LogitechController/LogitechControllerDetect.cpp,
// LogitechGLightsyncController/LogitechGLightsyncController.cpp and .h,
// RGBController_LogitechGLightsync.cpp and RGBController_LogitechGLightsync1zone.cpp,
// LogitechG203LController/LogitechG203LController.cpp and .h, RGBController_LogitechG203L.cpp,
// LogitechG600Controller/LogitechG600Controller.cpp and .h, RGBController_LogitechG600.cpp
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "TheRogueZeta  21 Apr 2021" (LogitechGLightsyncController and its
// RGBControllers); "Eric Samuelson (edbgon)  06 Oct 2020" (LogitechG203LController,
// RGBController_LogitechG203L); "Austin B (austinleroy)  11 Sep 2025" (LogitechG600Controller,
// RGBController_LogitechG600); LogitechControllerDetect.cpp names no author ("This file is part of
// the OpenRGB project").
//
// Protocol notes: docs/hardware/logitech-mouse.md. In short:
// - Lightsync (G203 Prodigy, G Pro, G303, G403 HERO): interface 1, two collections of usage page
//   0xFF00. Start: the HID++ short report 10 FF 0E 8A 01 01 on the usage-1 collection, which
//   OpenRGB comments "If direct, disable save to flash"; then per zone the long report
//   11 FF 0E 3A|3C <zone> 01 R G B 02 (a static colour) on the usage-2 collection, each followed
//   by a read of the reply.
// - G203 LIGHTSYNC: start 11 FF 0E 50 01 03 07 ("enable software control"), then per frame the
//   per-key report 11 FF 12 10 01 R G B 02 R G B 03 R G B FF and the apply report 11 FF 12 70,
//   twice (OpenRGB's "dirty workaround for color lag").
// - G600: one 8-byte feature report F1 R G B 00 00 00 00 (mode 0, "Direct").
// Never sent: OpenRGB's hardware effects (the G203 LIGHTSYNC's carry a byte OpenRGB marks as the
// end byte, 0x01, at the place the HID++ RGB-effects feature keeps its persistence flag), DPI,
// report rate, buttons or onboard profiles.

/* global device */

const VENDOR_ID = 0x046D;

// LogitechControllerDetect.cpp, in registration order.
const PRODUCTS = [
  { pid: 0xC084, name: "Logitech G203 Prodigy", kind: "LIGHTSYNC_1ZONE", fn: 0x3A },
  { pid: 0xC092, name: "Logitech G203 Lightsync", kind: "G203L" },
  { pid: 0xC09D, name: "Logitech G203 Lightsync", kind: "G203L" },
  { pid: 0xC080, name: "Logitech G303 Daedalus Apex", kind: "LIGHTSYNC_2ZONE", fn: 0x3A },
  { pid: 0xC08F, name: "Logitech G403 HERO", kind: "LIGHTSYNC_2ZONE", fn: 0x3A },
  { pid: 0xC24A, name: "Logitech G600 Gaming Mouse", kind: "G600" },
  { pid: 0xC085, name: "Logitech G Pro Gaming Mouse", kind: "LIGHTSYNC_1ZONE", fn: 0x3C },
  { pid: 0xC08C, name: "Logitech G Pro HERO Gaming Mouse", kind: "LIGHTSYNC_1ZONE", fn: 0x3C },
];

// LEDs in OpenRGB order (value = zone id sent); one-LED zones share row 0.
const MODELS = {
  LIGHTSYNC_1ZONE: { size: [1, 1], leds: [{ name: "Logo", value: 0, cell: [0, 0] }] },
  LIGHTSYNC_2ZONE: {
    size: [2, 1],
    leds: [{ name: "DPI", value: 0, cell: [0, 0] }, { name: "Logo", value: 1, cell: [1, 0] }],
  },
  G203L: {
    size: [3, 1],
    leds: [
      { name: "Mouse Left", value: 1, cell: [0, 0] },
      { name: "Mouse Center", value: 2, cell: [1, 0] },
      { name: "Mouse Right", value: 3, cell: [2, 0] },
    ],
  },
  G600: { size: [1, 1], leds: [{ name: "All", value: 0, cell: [0, 0] }] },
};

const VENDOR_PAGE = 0xFF00;
const G600_PAGE = 0xFF80;
const SHORT_REPORT = 0x10;
const LONG_REPORT = 0x11;
const LONG_LEN = 20;
const DEVICE_INDEX = 0xFF;
// The colour-effects feature index OpenRGB hard-codes for these mice (DetectLogitechMouse*).
const LIGHTSYNC_FEATURE = 0x0E;
// LogitechGLightsyncController::SetDirectMode: function 8, software id 0x0A.
const LIGHTSYNC_SET_DIRECT = 0x8A;
const LIGHTSYNC_MODE_STATIC = 0x01;
// OpenRGB waits for the replies without a timeout (hid_read); HueDeck never does.
const LIGHTSYNC_READ_TIMEOUT_MS = 300;
// LogitechG203LController::SendPacket: hid_read_timeout(..., 10).
const G203L_READ_TIMEOUT_MS = 10;
const G600_REPORT = 0xF1;
const G600_MODE_DIRECT = 0x00;
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

const DEFAULT_MODEL = MODELS.LIGHTSYNC_1ZONE;

export function Name() { return "Logitech G mouse (fixed-feature)"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/logitech-mouse"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
// Placeholders (the one-zone model's); Initialize() sets the product's own.
export function Size() { return DEFAULT_MODEL.size.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return DEFAULT_MODEL.leds.map((l) => l.name); }
export function LedPositions() { return DEFAULT_MODEL.leds.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// Logitech G HUB and Logitech Gaming Software drive the lights while they run; HueDeck reports
// them, never closes them.
export function ConflictingProcesses() { return ["lghub.exe", "lghub_agent.exe", "lghub_updater.exe", "LCore.exe"]; }

let product = null;
let model = null;
let lastColours = null;
let lastSentAt = 0;

// Lightsync mice: REGISTER_HID_DETECTOR_IP(..., 1, 0xFF00); OpenRGB then opens both the usage-1
// (short report) and the usage-2 (long report) collection of interface 1.
// G203 LIGHTSYNC: REGISTER_HID_DETECTOR_IPU(..., 1, 0xFF00, 2). G600: REGISTER_HID_DETECTOR_IP(...,
// 1, 0xFF80), any usage.
export function Validate(endpoint) {
  const p = PRODUCTS.find((q) => q.pid === device.productId());
  if (!p || endpoint.interface !== 1) {
    return false;
  }
  if (p.kind === "G600") {
    return endpoint.usage_page === G600_PAGE;
  }
  if (p.kind === "G203L") {
    return endpoint.usage_page === VENDOR_PAGE && endpoint.usage === 0x0002;
  }
  return endpoint.usage_page === VENDOR_PAGE && (endpoint.usage === 0x0001 || endpoint.usage === 0x0002);
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  model = product ? MODELS[product.kind] : null;
  if (!model) {
    device.log(`Logitech mouse: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(model.size.slice());
  device.setControllableLeds(model.leds.map((l) => l.name), model.leds.map((l) => l.cell.slice()));
  if (product.kind === "G203L") {
    // LogitechG203LController constructor: "enable software control"; then DeviceUpdateMode()
    // for Direct sends the (still black) colours once.
    const pkt = new Array(LONG_LEN).fill(0);
    pkt.splice(0, 7, LONG_REPORT, DEVICE_INDEX, 0x0E, 0x50, 0x01, 0x03, 0x07);
    g203lSend(pkt);
    g203lSetDevice(model.leds.map(() => [0, 0, 0]));
  } else if (product.kind !== "G600") {
    // RGBController_LogitechGLightsync(1zone)::DeviceUpdateMode() for Direct:
    // SetDirectMode(true), on the short-report collection; the reply comes on the long one.
    useUsage(0x0001);
    device.write([SHORT_REPORT, DEVICE_INDEX, LIGHTSYNC_FEATURE, LIGHTSYNC_SET_DIRECT, 0x01, 0x01, 0x00], 7);
    useUsage(0x0002);
    device.read([LONG_REPORT], LONG_LEN, LIGHTSYNC_READ_TIMEOUT_MS);
  }
  // G600: DeviceUpdateMode() and DeviceUpdateLEDs() send the same report; the first Render()
  // sends it.
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

// OpenRGB has no exit step. Lightsync: leaving software control (SetDirectMode(false)) is the
// command OpenRGB pairs with turning saving back on, so it is not sent; the LEDs go black in
// software control, which the mouse does not save. G203 LIGHTSYNC and G600: black.
export function Shutdown(SystemSuspending) {
  if (!model) {
    return;
  }
  updateLeds(model.leds.map(() => [0, 0, 0]));
}

function updateLeds(colours) {
  if (product.kind === "G600") {
    // LogitechG600Controller::SetMode(mode, speed, colour): F1 R G B mode speed 00 00.
    const [r, g, b] = colours[0];
    return device.send_report([G600_REPORT, r & 0xFF, g & 0xFF, b & 0xFF, G600_MODE_DIRECT, 0x00, 0x00, 0x00], 8) !== -1;
  }
  if (product.kind === "G203L") {
    // RGBController_LogitechG203L::DeviceUpdateLEDs: SetDevice twice.
    const first = g203lSetDevice(colours);
    const second = g203lSetDevice(colours);
    return first && second;
  }
  // RGBController_LogitechGLightsync(1zone)::DeviceUpdateLEDs: every zone, static colour, on
  // the long-report collection Initialize() left active.
  let ok = true;
  model.leds.forEach((led, i) => {
    const [r, g, b] = colours[i];
    const pkt = new Array(LONG_LEN).fill(0);
    pkt[0] = LONG_REPORT;
    pkt[1] = DEVICE_INDEX;
    pkt[2] = LIGHTSYNC_FEATURE;
    pkt[3] = product.fn;
    pkt[4] = led.value;
    pkt[5] = LIGHTSYNC_MODE_STATIC;
    pkt[6] = r & 0xFF;
    pkt[7] = g & 0xFF;
    pkt[8] = b & 0xFF;
    pkt[9] = 0x02;
    ok = device.write(pkt, LONG_LEN) !== -1 && ok;
    device.read([LONG_REPORT], LONG_LEN, LIGHTSYNC_READ_TIMEOUT_MS);
  });
  return ok;
}

// LogitechG203LController::SetDevice, then SendApply.
function g203lSetDevice(colours) {
  const pkt = new Array(LONG_LEN).fill(0);
  pkt.splice(0, 4, LONG_REPORT, DEVICE_INDEX, 0x12, 0x10);
  model.leds.forEach((led, i) => {
    const [r, g, b] = colours[i];
    pkt.splice(4 + 4 * i, 4, led.value, r & 0xFF, g & 0xFF, b & 0xFF);
  });
  pkt[0x10] = 0xFF;
  const sent = g203lSend(pkt);
  const apply = new Array(LONG_LEN).fill(0);
  apply.splice(0, 4, LONG_REPORT, DEVICE_INDEX, 0x12, 0x70);
  return g203lSend(apply) && sent;
}

// LogitechG203LController::SendPacket: write, then read the reply (10 ms). OpenRGB reopens the
// device when either fails; HueDeck's host owns the handle, so a failed write is only reported.
function g203lSend(pkt) {
  if (device.write(pkt, LONG_LEN) === -1) {
    return false;
  }
  device.read([LONG_REPORT], LONG_LEN, G203L_READ_TIMEOUT_MS);
  return true;
}

// Switch between the two Lightsync collections (HueDeck's host opens one at a time).
function useUsage(usage) {
  if (typeof device.set_endpoint === "function") {
    device.set_endpoint(1, usage, VENDOR_PAGE);
  }
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
