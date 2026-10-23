// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: ASUS ROG / TUF keyboards with Aura per-key lighting over USB: the ROG Strix
// Scope TKL family (OpenRGB's "Aura keyboard" protocol) and the keyboards of OpenRGB's "Aura TUF
// keyboard" protocol (ROG Azoth, Claymore, Falchion, Strix Flare / Flare II / Flare II Animate,
// Strix Scope / Scope RX / Scope NX Wireless Deluxe / Scope II / Scope II 96, TUF Gaming K1, K3,
// K3 Gen II, K5, K7), per key.
// Adapted from OpenRGB Controllers/AsusAuraUSBController/AsusAuraUSBControllerDetect.cpp,
// AsusAuraKeyboardController/AsusAuraKeyboardController.cpp and .h,
// AsusAuraKeyboardController/RGBController_AsusAuraKeyboard.cpp and .h,
// AsusAuraTUFKeyboardController/AsusAuraTUFKeyboardController.cpp and .h,
// AsusAuraTUFKeyboardController/RGBController_AsusAuraTUFKeyboard.cpp and .h,
// AsusAuraTUFKeyboardController/AsusAuraTUFKeyboardLayouts.h (layouts.js) (GPL-2.0-or-later) at
// commit d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  19 Mar 2020" (AsusAuraKeyboardController,
// RGBController_AsusAuraKeyboard); "Mola19  03 Mar 2021" (AsusAuraTUFKeyboardController,
// RGBController_AsusAuraTUFKeyboard); "Mola19  02 Nov 2021" (AsusAuraTUFKeyboardLayouts);
// AsusAuraUSBControllerDetect.cpp names no author ("This file is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/asus-aura-keyboard.md. In short: 65-byte output reports with
// report id 0x00 on the vendor collection (usage page 0xFF00). Per frame: 00 C0 81 <count> 00 and
// up to 15 LEDs as <LED id> R G B. The TUF protocol reads one input report after each one; the
// keyboard layout (and on the Claymore the numpad position) is read once at start. Never sent:
// the save command (00 50 55), hardware effects (00 51 2C / 00 51 28 / 00 51 91).

/* global device */

import { TUF_LAYOUTS, SCOPE_TKL_LAYOUT } from "./layouts.js";

const VENDOR_ID = 0x0B05;
const SCOPE_TKL = "scope-tkl";   // AuraKeyboardController (DetectAsusAuraUSBKeyboards)
const TUF = "tuf";               // AuraTUFKeyboardController (DetectAsusAuraTUFUSBKeyboard)

const CLAYMORE = 0x184D;
const K1 = 0x1945;
const K5 = 0x1899;

// AsusAuraUSBControllerDetect.cpp, in registration order: every keyboard is
// REGISTER_HID_DETECTOR_IP(..., interface, 0xFF00). `map`: the layout table
// RGBController_AuraTUFKeyboard::SetupZones picks for the product (TUF protocol).
const PRODUCTS = [
  { pid: 0x190C, name: "ASUS ROG Strix Scope TKL", iface: 1, proto: SCOPE_TKL },
  { pid: 0x1A05, name: "ASUS ROG Strix Scope RX TKL Wireless Deluxe", iface: 1, proto: SCOPE_TKL },
  { pid: 0x1954, name: "ASUS ROG Strix Scope TKL PNK LTD", iface: 1, proto: SCOPE_TKL },
  { pid: 0x19D0, name: "ASUS ROG Strix Scope NX TKL Moonlight White", iface: 1, proto: SCOPE_TKL },
  { pid: 0x1A83, name: "ASUS ROG Azoth USB", iface: 1, proto: TUF, map: "AsusROGAzothLayouts" },
  { pid: 0x1A85, name: "ASUS ROG Azoth 2.4GHz", iface: 1, proto: TUF, map: "AsusROGAzothLayouts" },
  { pid: 0x184D, name: "ASUS ROG Claymore", iface: 1, proto: TUF, map: null },
  { pid: 0x193C, name: "ASUS ROG Falchion (Wired)", iface: 1, proto: TUF, map: "AsusFalchionLayouts" },
  { pid: 0x193E, name: "ASUS ROG Falchion (Wireless)", iface: 1, proto: TUF, map: "AsusFalchionLayouts" },
  { pid: 0x1875, name: "ASUS ROG Strix Flare", iface: 1, proto: TUF, map: "AsusROGStrixFlareLayouts" },
  { pid: 0x18CF, name: "ASUS ROG Strix Flare PNK LTD", iface: 1, proto: TUF, map: "AsusROGStrixFlareLayouts" },
  { pid: 0x18AF, name: "ASUS ROG Strix Flare CoD Black Ops 4 Edition", iface: 1, proto: TUF, map: "AsusROGStrixFlareLayouts" },
  { pid: 0x19FC, name: "ASUS ROG Strix Flare II Animate", iface: 1, proto: TUF, map: "AsusROGStrixFlareIIAnimateLayouts" },
  { pid: 0x19FE, name: "ASUS ROG Strix Flare II", iface: 1, proto: TUF, map: "AsusROGStrixFlareIILayouts" },
  { pid: 0x18F8, name: "ASUS ROG Strix Scope", iface: 1, proto: TUF, map: "AsusROGStrixScopeLayouts" },
  { pid: 0x1951, name: "ASUS ROG Strix Scope RX", iface: 1, proto: TUF, map: "AsusROGStrixScopeLayouts" },
  { pid: 0x1B12, name: "ASUS ROG Strix Scope RX EVA-02 Edition", iface: 1, proto: TUF, map: "AsusROGStrixScopeLayouts" },
  { pid: 0x19F6, name: "ASUS ROG Strix Scope NX Wireless Deluxe USB", iface: 1, proto: TUF, map: "AsusROGStrixScopeLayouts" },
  { pid: 0x19F8, name: "ASUS ROG Strix Scope NX Wireless Deluxe 2.4GHz", iface: 1, proto: TUF, map: "AsusROGStrixScopeLayouts" },
  { pid: 0x1AB3, name: "ASUS ROG Strix Scope II", iface: 1, proto: TUF, map: "AsusROGStrixScopeIILayouts" },
  { pid: 0x1AB5, name: "ASUS ROG Strix Scope II RX", iface: 1, proto: TUF, map: "AsusROGStrixScopeIILayouts" },
  { pid: 0x1AAE, name: "ASUS ROG Strix Scope II 96 Wireless USB", iface: 1, proto: TUF, map: "AsusROGStrixScopeII96WirelessLayouts" },
  { pid: 0x1B78, name: "ASUS ROG Strix Scope II 96 RX Wireless USB", iface: 1, proto: TUF, map: "AsusROGStrixScopeII96RxWirelessLayouts" },
  { pid: 0x1945, name: "ASUS TUF Gaming K1", iface: 2, proto: TUF, map: "AsusTufK1Layouts" },
  { pid: 0x194B, name: "ASUS TUF Gaming K3", iface: 1, proto: TUF, map: "AsusTUFK7Layouts" },
  { pid: 0x1B30, name: "ASUS TUF Gaming K3 GEN II", iface: 1, proto: TUF, map: "AsusTUFK3GamingGen2Layouts" },
  { pid: 0x1899, name: "ASUS TUF Gaming K5", iface: 2, proto: TUF, map: "AsusTufK1Layouts" },
  { pid: 0x18AA, name: "ASUS TUF Gaming K7", iface: 1, proto: TUF, map: "AsusTUFK7Layouts" },
  { pid: 0x1C5E, name: "ASUS TUF Gaming K3 GEN II MIKU EDITION", iface: 1, proto: TUF, map: "AsusTUFK3GamingGen2Layouts" },
];

const USAGE_PAGE = 0xFF00;
const REPORT_LENGTH = 65;          // unsigned char usb_buf[65]
const LEDS_PER_PACKET = 15;
const LAYOUT_UK = 4;               // ASUS_TUF_K7_LAYOUT_UK
const LAYOUT_US = 17;              // ASUS_TUF_K7_LAYOUT_US
// The Claymore's layout comes from the USB release number (bcdDevice >> 12), which Lightloom's host
// does not expose; OpenRGB's default branch (117, US) is used.
const CLAYMORE_DEFAULT_LAYOUT = 117;
// OpenRGB's hid_read() in GetLayout / GetNumpadLocation blocks until a report arrives; Lightloom
// waits at most this long (once, at start).
const PROBE_TIMEOUT_MS = 1000;
// AwaitResponse(20): hid_read_timeout(..., 20) after every TUF report.
const AWAIT_MS = 20;
// AuraKeyboardController::SendDirect ends with a blocking hid_read ("Read to apply"); Lightloom
// waits at most this long for it.
const APPLY_READ_MS = 20;
// An unchanged frame is sent again only after this long (OpenRGB sends only on changes).
const KEEPALIVE_MS = 1000;

export function Name() { return "ASUS Aura Keyboard"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/asus-aura-keyboard"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
// Placeholders; Initialize() sets the product's layout.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// ASUS's own lighting software drives the keyboard while it runs; Lightloom reports it, never
// closes it.
export function ConflictingProcesses() {
  return ["ArmouryCrate.exe", "ArmouryCrate.Service.exe", "LightingService.exe"];
}

let product = null;
let leds = [];             // [name, x, y, id] in OpenRGB's LED order
let lastFrame = null;
let lastFrameAt = 0;

// REGISTER_HID_DETECTOR_IP(..., interface, 0xFF00).
export function Validate(endpoint) {
  if (endpoint == null) {
    return false;
  }
  const known = PRODUCTS.find((p) => p.pid === device.productId());
  const iface = known !== undefined ? known.iface : 1;
  return endpoint.interface === iface && Number(endpoint.usage_page) === USAGE_PAGE;
}

export function Initialize() {
  leds = [];
  lastFrame = null;
  lastFrameAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (product === null) {
    device.log(`ASUS Aura Keyboard: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  let layout = null;
  if (product.proto === SCOPE_TKL) {
    layout = SCOPE_TKL_LAYOUT;
  } else {
    layout = tufLayout();
  }
  if (layout === null) {
    device.setSize([1, 1]);
    device.setControllableLeds([], []);
    return;
  }
  leds = layout.leds.map((l) => l.slice());
  device.setSize([layout.width, layout.height]);
  device.setControllableLeds(leds.map((l) => l[0]), leds.map((l) => [l[1], l[2]]));

  if (product.pid === CLAYMORE) {
    // RGBController_AuraTUFKeyboard::DeviceUpdateMode with Direct active (SetupZones' last step):
    // AllowRemoteControl(1), then AllowRemoteControl(3).
    tufCommand([0x00, 0x41, 0x01]);
    tufCommand([0x00, 0x41, 0x03]);
  }
}

export function Render() {
  if (leds.length === 0) {
    return;
  }
  const colours = leds.map((l) => device.color(l[1], l[2]));
  const frame = frameReports(colours);
  const now = Date.now();
  if (lastFrame !== null && sameReports(frame, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
    return;
  }
  if (sendFrame(frame)) {
    lastFrame = frame;
    lastFrameAt = now;
  } else {
    lastFrame = null;
  }
}

// OpenRGB has no exit step, and these keyboards have no "back to the onboard effect" command in
// it that does not also go through a hardware-effect write (the Claymore's AllowRemoteControl(0)
// follows a mode change and a save there). So every key is set black, on exit and on suspend.
export function Shutdown(SystemSuspending) {
  if (leds.length === 0) {
    return;
  }
  sendFrame(frameReports(leds.map(() => [0, 0, 0])));
  lastFrame = null;
}

// ---------------------------------------------------------------------------------------------
// TUF protocol: layout
// ---------------------------------------------------------------------------------------------
// RGBController_AuraTUFKeyboard::SetupZones.
function tufLayout() {
  let mapName = product.map;
  if (product.pid === CLAYMORE) {
    // GetNumpadLocation(): 00 40 60, reply bytes 4 and 5 (data, after the report id).
    const reply = tufRequest([0x00, 0x40, 0x60]);
    const numpad = reply === null ? 0 : ((reply[6] * 2 + reply[5]) & 0xFF);
    mapName = numpad === 2 ? "AsusClaymoreNumpadRightLayouts"
      : numpad === 3 ? "AsusClaymoreNumpadLeftLayouts" : "AsusClaymoreNoNumpadLayouts";
  }
  const table = TUF_LAYOUTS[mapName];
  let raw;
  if (product.pid === CLAYMORE) {
    raw = CLAYMORE_DEFAULT_LAYOUT;
  } else {
    // GetLayout(): 00 12 12, layout = data[4] * 100 + data[5].
    const reply = tufRequest([0x00, 0x12, 0x12]);
    raw = reply === null ? 0 : reply[5] * 100 + reply[6];
    if (reply === null) {
      device.log("ASUS Aura Keyboard: no layout reply; using the US layout.");
    }
  }
  // `unsigned char layout = controller->GetLayout();` truncates to 8 bits.
  let layout = raw & 0xFF;
  if (table[layout % 100] === undefined) {
    layout = Math.floor(layout / 100) === 2 ? LAYOUT_UK : LAYOUT_US;
  } else {
    layout = layout % 100;
  }
  const found = table[layout];
  if (found === undefined) {
    // OpenRGB's keyboard[layout] would create an empty layout: no LEDs.
    device.log(`ASUS Aura Keyboard: no ${layout === LAYOUT_UK ? "UK" : "US"} layout for this keyboard; nothing is sent.`);
    return null;
  }
  device.log(`ASUS Aura Keyboard: layout ${raw} -> ${layout}, ${found.leds.length} LEDs.`);
  return found;
}

// ---------------------------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------------------------
function frameReports(colours) {
  if (product.proto === SCOPE_TKL) {
    return scopeTklReports(colours);
  }
  return tufReports(colours);
}

// AuraTUFKeyboardController::UpdateLeds.
function tufReports(colours) {
  const perLed = product.pid !== K1 && product.pid !== K5;
  const reports = [];
  const packets = Math.ceil(colours.length / LEDS_PER_PACKET);
  for (let i = 0; i < packets; i++) {
    const buf = new Array(REPORT_LENGTH).fill(0);
    const remaining = colours.length - i * LEDS_PER_PACKET;
    const count = remaining > 0x0F ? 0x0F : remaining;
    buf[0] = 0x00;
    buf[1] = 0xC0;
    buf[2] = 0x81;
    buf[3] = count;
    buf[4] = 0x00;
    for (let j = 0; j < count; j++) {
      const led = i * LEDS_PER_PACKET + j;
      const c = colours[led];
      buf[j * 4 + 5] = perLed ? leds[led][3] : 0x00;
      buf[j * 4 + 6] = c[0] & 0xFF;
      buf[j * 4 + 7] = c[1] & 0xFF;
      buf[j * 4 + 8] = c[2] & 0xFF;
    }
    reports.push(buf);
  }
  return reports;
}

// RGBController_AuraKeyboard::DeviceUpdateLEDs + AuraKeyboardController::SendDirect.
function scopeTklReports(colours) {
  const frameCount = colours.length & 0xFF;
  const frameData = [];
  colours.forEach((c, i) => frameData.push(leds[i][3], c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF));
  const packetCount = Math.floor(frameCount / 15) + ((frameCount % 15) === 0 ? 0 : 1);
  const reports = [];
  const buf = new Array(REPORT_LENGTH).fill(0);
  buf[0x00] = 0x00;
  buf[0x01] = 0xC0;
  buf[0x02] = 0x81;
  buf[0x03] = (packetCount * 15) & 0xFF;
  buf[0x04] = 0x00;
  let total = 0;
  for (let packet = 0; packet < packetCount; packet++) {
    let size = 15 * 4;
    if (frameCount - total < 15) {
      size = (frameCount - total) * 4;
      for (let i = 0x05 + size; i < 0x05 + 15 * 4; i++) {
        buf[i] = 0xFF;
      }
    }
    for (let i = 0; i < size; i++) {
      buf[0x05 + i] = frameData[packet * 15 * 4 + i];
    }
    reports.push(buf.slice());
    buf[0x03] = (buf[0x03] - 0x0F) & 0xFF;
    total += 0x0F;
  }
  return reports;
}

function sendFrame(reports) {
  let ok = true;
  if (product.proto === TUF) {
    for (const report of reports) {
      // ClearResponses(); hid_write(); AwaitResponse(20).
      device.clearReadBuffer();
      ok = device.write(report, REPORT_LENGTH) !== -1 && ok;
      device.read([0x00], REPORT_LENGTH, AWAIT_MS);
    }
  } else {
    for (const report of reports) {
      ok = device.write(report, REPORT_LENGTH) !== -1 && ok;
    }
    // "Read to apply".
    device.read([0x00], REPORT_LENGTH, APPLY_READ_MS);
  }
  return ok;
}

// ---------------------------------------------------------------------------------------------
// TUF protocol: commands
// ---------------------------------------------------------------------------------------------
// ClearResponses(); hid_write(); hid_read(): the reply, report id first, or null.
function tufRequest(bytes) {
  device.clearReadBuffer();
  if (device.write(pad(bytes), REPORT_LENGTH) === -1) {
    return null;
  }
  const data = device.read([0x00], REPORT_LENGTH, PROBE_TIMEOUT_MS);
  if (device.getLastReadSize() <= 0 || data == null || data.length < 7) {
    return null;
  }
  return Array.from(data, (b) => b & 0xFF);
}

// ClearResponses(); hid_write(); AwaitResponse(20).
function tufCommand(bytes) {
  device.clearReadBuffer();
  device.write(pad(bytes), REPORT_LENGTH);
  device.read([0x00], REPORT_LENGTH, AWAIT_MS);
}

function pad(bytes) {
  const buf = new Array(REPORT_LENGTH).fill(0);
  bytes.forEach((b, i) => { buf[i] = b & 0xFF; });
  return buf;
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
