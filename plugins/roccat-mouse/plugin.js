// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the lights of Roccat mice, per LED: Kone Aimo and Kone Aimo 16K, Kone Pro, Kone
// Pro Air (wireless and wired), Kone XP, Burst Core and Burst Pro. The Burst Pro Air and the Kova,
// which OpenRGB also drives, are held back (see below).
// Adapted from OpenRGB Controllers/RoccatController/RoccatKoneAimoController/
// RoccatKoneAimoController.cpp, .h and RGBController_RoccatKoneAimo.cpp;
// RoccatKoneProController/RoccatKoneProController.cpp, .h and RGBController_RoccatKonePro.cpp;
// RoccatKoneProAirController/RoccatKoneProAirController.cpp, .h and RGBController_RoccatKoneProAir.cpp;
// RoccatKoneXPController/RoccatKoneXPController.cpp, .h and RGBController_RoccatKoneXP.cpp;
// RoccatBurstController/RoccatBurstController.cpp, .h and RGBController_RoccatBurst.cpp; and
// Controllers/RoccatController/RoccatControllerDetect.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Thibaud M (enlight3d)  17 Nov 2020" (Kone Aimo files); "Garrett Denham
// (GardenOfWyers)  12 Jan 2024" (Kone Pro files); "Plunti  10 Jun 2024" (Kone Pro Air files);
// "Mola19  12 Jun 2024" (Kone XP files); "Morgan Guimard (morg)  24 Feb 2022"
// (RoccatBurstController.cpp) and "Morgan Guimard (morg)  01 Jun 2022" (RoccatBurstController.h,
// RGBController_RoccatBurst); RoccatControllerDetect.cpp names no author.
//
// Protocol notes: docs/hardware/roccat-mouse.md. In short, by product id:
//   KONE_AIMO      feature reports: init 0E 06 01 01 00 FF; each frame a 46-byte 0D 2E report
//                  (R G B 00 per LED) and a 3-byte GET_FEATURE of report 0x0D.
//   KONE_PRO,      feature reports: software control 0E 06 01 01 00 FF at start; each frame an
//   BURST_*        11-byte 0D 0B report (R G B per LED); at exit 0E 06 01 00 00 FF, OpenRGB's
//                  "switch to built-in mode".
//   KONE_XP        as KONE_PRO with a ready poll (report 0x04); each frame a 122-byte 0D 7A report
//                  (R G B FF FF FF per LED).
//   KONE_PRO_AIR   16-byte output reports 00 10 10 0B 00 09 64 64 64 0B, then R G B per LED.
// Never sent: the profile ("flash") report 0x06 (OpenRGB's SetMode for the hardware effects, which
// on the Kone XP also carries DPI and polling rate), the Kone XP's profile select (0x04 xx 80 FF),
// the Kone Pro Air's settings write (10 50 14 ...), DPI or polling rate.
//
// Held back (not claimed): Roccat Burst Pro Air (1E7D:2CA6): every colour update is a report 0x06,
// the report id the Burst and Kone Pro use for their profile write, and OpenRGB does not know
// whether it is saved. Roccat Kova (1E7D:2CEE): no direct mode; every colour change rewrites the
// stored profile (report 0x06, with DPI and polling rate in it).

/* global device */

const PRODUCTS = [
  // RoccatControllerDetect.cpp, REGISTER_HID_DETECTOR_IPU(name, ..., interface, usage page, usage).
  { pid: 0x2E27, name: "Roccat Kone Aimo", variant: "KONE_AIMO", iface: 0, page: 0x000B, usage: 0x0000 },
  { pid: 0x2E2C, name: "Roccat Kone Aimo 16K", variant: "KONE_AIMO", iface: 0, page: 0x000B, usage: 0x0000 },
  { pid: 0x2C88, name: "Roccat Kone Pro", variant: "KONE_PRO", iface: 3, page: 0xFF01, usage: 0x0001 },
  { pid: 0x2C8E, name: "Roccat Kone Pro Air", variant: "KONE_PRO_AIR", iface: 2, page: 0xFF00, usage: 0x0001 },
  { pid: 0x2C92, name: "Roccat Kone Pro Air (Wired)", variant: "KONE_PRO_AIR", iface: 1, page: 0xFF13, usage: 0x0001 },
  { pid: 0x2C8B, name: "Roccat Kone XP", variant: "KONE_XP", iface: 3, page: 0xFF01, usage: 0x0001 },
  { pid: 0x2DE6, name: "Roccat Burst Core", variant: "BURST_CORE", iface: 3, page: 0xFF01, usage: 0x0001 },
  { pid: 0x2DE1, name: "Roccat Burst Pro", variant: "BURST_PRO", iface: 3, page: 0xFF01, usage: 0x0001 },
];

// LEDs in OpenRGB order (the RGBControllers' SetupZones). Grid: a zone with several LEDs gets a
// row of its own, consecutive one-LED zones share a row.
const VARIANTS = {
  // Zones Scroll Wheel (channel 0), Strip left (1..4), Strip right (5..8), Lower left (9), Lower
  // right (10); `ch` is the channel index of SetChannelColors (bytes 2 + 4 * ch ...).
  KONE_AIMO: {
    size: [4, 4],
    leds: [
      { name: "Wheel LED", cell: [0, 0], ch: 0 },
      { name: "Strip left LED 1", cell: [0, 1], ch: 1 },
      { name: "Strip left LED 2", cell: [1, 1], ch: 2 },
      { name: "Strip left LED 3", cell: [2, 1], ch: 3 },
      { name: "Strip left LED 4", cell: [3, 1], ch: 4 },
      { name: "Strip right LED 1", cell: [0, 2], ch: 5 },
      { name: "Strip right LED 2", cell: [1, 2], ch: 6 },
      { name: "Strip right LED 3", cell: [2, 2], ch: 7 },
      { name: "Strip right LED 4", cell: [3, 2], ch: 8 },
      { name: "Lower left LED", cell: [0, 3], ch: 9 },
      { name: "Lower right LED", cell: [1, 3], ch: 10 },
    ],
  },
  KONE_PRO: { size: [2, 1], leds: [{ name: "Left Button", cell: [0, 0] }, { name: "Right Button", cell: [1, 0] }] },
  KONE_PRO_AIR: { size: [2, 1], leds: [{ name: "Left Button", cell: [0, 0] }, { name: "Right Button", cell: [1, 0] }] },
  KONE_XP: {
    size: [9, 3],
    leds: [
      ...Array.from({ length: 9 }, (_, i) => ({ name: `Left LED ${i + 1}`, cell: [i, 0] })),
      ...Array.from({ length: 9 }, (_, i) => ({ name: `Right LED ${i + 1}`, cell: [i, 1] })),
      { name: "Scrollwheel LED", cell: [0, 2] },
      { name: "DPI button LED", cell: [1, 2] },
    ],
  },
  BURST_CORE: { size: [1, 1], leds: [{ name: "Scroll Wheel", cell: [0, 0] }] },
  BURST_PRO: { size: [2, 1], leds: [{ name: "Scroll Wheel", cell: [0, 0] }, { name: "Logo", cell: [1, 0] }] },
};

/** Render() resends unchanged colours only after this long. */
const KEEPALIVE_MS = 1000;
/**
 * Kone XP WaitUntilReady polls report 0x04 every 200 ms up to 100 times (20 s) after its profile
 * write; Lightloom sends no profile write and waits at most this many polls (2 s).
 */
const KONE_XP_READY_POLLS = 10;

const DEFAULT = VARIANTS.KONE_AIMO;

export function Name() { return "Roccat Mouse"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/roccat-mouse"; }
export function VendorId() { return 0x1E7D; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
// Placeholders (the first product's); Initialize() sets the real ones from device.productId().
export function Size() { return DEFAULT.size.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return DEFAULT.leds.map((l) => l.name); }
export function LedPositions() { return DEFAULT.leds.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// Roccat's / Turtle Beach's own software drives the lights while it runs; Lightloom reports it.
export function ConflictingProcesses() {
  return ["ROCCAT_Swarm.exe", "ROCCAT_Swarm_Monitor.exe", "Turtle Beach Swarm II.exe"];
}

let product = null;
let model = null;
let lastColours = null;
let lastSentAt = 0;

function productFor(pid) {
  return PRODUCTS.find((p) => p.pid === pid) ?? null;
}

// REGISTER_HID_DETECTOR_IPU: interface, usage page and usage all match.
export function Validate(endpoint) {
  const p = productFor(device.productId());
  return p !== null && endpoint.interface === p.iface && endpoint.usage_page === p.page && endpoint.usage === p.usage;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  product = productFor(device.productId());
  model = product ? VARIANTS[product.variant] : null;
  if (!model) {
    device.log(`Roccat Mouse: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(model.size.slice());
  device.setControllableLeds(model.leds.map((l) => l.name), model.leds.map((l) => l.cell.slice()));
  switch (product.variant) {
    case "KONE_AIMO":
      // RoccatKoneAimoController::SendInit: read a report first ("swarm does it"), then the init.
      device.get_report([0x00], 3);
      device.send_report([0x0E, 0x06, 0x01, 0x01, 0x00, 0xFF], 6);
      break;
    case "KONE_PRO":
    case "BURST_CORE":
    case "BURST_PRO":
      // SetupDirectMode() -> SwitchControl(true), sent when OpenRGB creates the controller.
      switchControl(true);
      break;
    case "KONE_XP":
      // DeviceUpdateMode() for Direct is SetMode(profile), WaitUntilReady, EnableDirect(true),
      // WaitUntilReady. The profile write is left out (it also carries DPI and polling rate).
      switchControl(true);
      waitUntilReadyKoneXP();
      break;
    default:
      // KONE_PRO_AIR: OpenRGB sends nothing before the first colours.
      break;
  }
}

export function Render() {
  if (!model) return;
  const colours = model.leds.map((l) => device.color(l.cell[0], l.cell[1]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (sendColours(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step. Kone Pro, Burst and Kone XP: SwitchControl(false) / EnableDirect(false),
// what OpenRGB sends to hand the mouse to its built-in lighting, without the profile write before
// it. Kone Aimo and Kone Pro Air have no such command in OpenRGB: their LEDs are set black.
export function Shutdown(SystemSuspending) {
  if (!model) return;
  switch (product.variant) {
    case "KONE_PRO":
    case "BURST_CORE":
    case "BURST_PRO":
      switchControl(false);
      break;
    case "KONE_XP":
      switchControl(false);
      waitUntilReadyKoneXP();
      break;
    default:
      sendColours(model.leds.map(() => [0, 0, 0]));
  }
}

function sendColours(colours) {
  switch (product.variant) {
    case "KONE_AIMO": return koneAimoUpdate(colours);
    case "KONE_XP": return koneXpDirect(colours);
    case "KONE_PRO_AIR": return koneProAirDirect(colours);
    default: return direct0D0B(colours);
  }
}

// RoccatKoneProController / RoccatBurstController::SwitchControl, RoccatKoneXPController::EnableDirect.
function switchControl(on) {
  return device.send_report([0x0E, 0x06, 0x01, on ? 0x01 : 0x00, 0x00, 0xFF], 6) !== -1;
}

// RoccatKoneXPController::WaitUntilReady (capped, see KONE_XP_READY_POLLS).
function waitUntilReadyKoneXP() {
  let ready = false;
  for (let i = 0; !ready && i < KONE_XP_READY_POLLS; i++) {
    if (i !== 0) device.pause(200);
    const buf = device.get_report([0x04], 4);
    ready = Array.isArray(buf) && buf[1] === 1;
  }
}

// RoccatKoneAimoController::SetChannelColors + SendUpdate: the whole 46-byte buffer, then a
// 3-byte read of report 0x0D. (The C++ reads that answer into its colour buffer, so the header
// of later frames is whatever the mouse answered; Lightloom keeps 0D 2E, see the hardware notes.)
function koneAimoUpdate(colours) {
  const buf = new Array(46).fill(0);
  buf[0] = 0x0D;
  buf[1] = 0x2E;
  model.leds.forEach((led, i) => {
    const at = 0x02 + led.ch * 4;
    buf[at] = colours[i][0] & 0xFF;
    buf[at + 1] = colours[i][1] & 0xFF;
    buf[at + 2] = colours[i][2] & 0xFF;
  });
  const ok = device.send_report(buf, 46) !== -1;
  device.get_report([0x0D], 3);
  return ok;
}

// RoccatKoneProController / RoccatBurstController::SendDirect: 0D 0B, R G B per LED, 11 bytes.
function direct0D0B(colours) {
  const buf = new Array(11).fill(0);
  buf[0] = 0x0D;
  buf[1] = 0x0B;
  colours.forEach(([r, g, b], i) => {
    buf[0x02 + 3 * i] = r & 0xFF;
    buf[0x03 + 3 * i] = g & 0xFF;
    buf[0x04 + 3 * i] = b & 0xFF;
  });
  return device.send_report(buf, 11) !== -1;
}

// RoccatKoneXPController::SendDirect: 0D 7A, then R G B FF FF FF per LED (at most 20), 0x7A bytes.
function koneXpDirect(colours) {
  const buf = new Array(0x7A).fill(0);
  buf[0] = 0x0D;
  buf[1] = 0x7A;
  colours.slice(0, 20).forEach(([r, g, b], i) => {
    buf[0x02 + i * 6] = r & 0xFF;
    buf[0x03 + i * 6] = g & 0xFF;
    buf[0x04 + i * 6] = b & 0xFF;
    buf[0x05 + i * 6] = 0xFF;
    buf[0x06 + i * 6] = 0xFF;
    buf[0x07 + i * 6] = 0xFF;
  });
  return device.send_report(buf, 0x7A) !== -1;
}

// RoccatKoneProAirController::SendDirect -> SendRGB(true, colours, 0x09, SPEED_MAX 0x0B,
// BRIGHTNESS_MAX 0x64): a 16-byte output report, report id 0x00 first.
function koneProAirDirect(colours) {
  const buf = new Array(16).fill(0);
  buf[1] = 0x10;
  buf[2] = 0x10;
  buf[3] = 0x0B;
  buf[4] = 0x00;
  buf[5] = 0x09;
  buf[6] = 0x64;
  buf[7] = 0x64;
  buf[8] = 0x64;
  buf[9] = 0x0B;
  colours.forEach(([r, g, b], i) => {
    buf[10 + 3 * i] = r & 0xFF;
    buf[11 + 3 * i] = g & 0xFF;
    buf[12 + 3 * i] = b & 0xFF;
  });
  return device.write(buf, 16) !== -1;
}

function sameColours(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) return false;
  }
  return true;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
