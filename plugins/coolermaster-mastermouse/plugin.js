// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the lights of the Cooler Master MasterMouse MM530, MM711, MM720 and MM730, per
// LED (scroll wheel, logo, and on the MM530 the buttons).
// Adapted from OpenRGB Controllers/CoolerMasterController/CoolerMasterControllerDetect.cpp,
// CMMMController/CMMMController.cpp and .h, CMMMController/RGBController_CMMMController.cpp and
// .h, CMMM711Controller/CMMM711Controller.cpp and .h, CMMM711Controller/
// RGBController_CMMM711Controller.cpp and .h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Chris M (Dr_No)  14 Feb 2021" and "Dracrius  12 Mar 2022" (CMMMController,
// RGBController_CMMMController); "Chris M (Dr_No)  14 Feb 2021" (CMMM711Controller,
// RGBController_CMMM711Controller); CoolerMasterControllerDetect.cpp names no author ("This file
// is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/coolermaster-mastermouse.md. In short: interface 1 (usage page
// 0xFF00, usage 1), 65-byte output reports (report id 0x00), each followed by an input report
// read. Start: 41 80; for the MM530/MM720/MM730 also OpenRGB's switch to its Direct ("custom",
// 0xB0) mode. Every frame: one 51 A8 report with the colours. Never sent: the save command
// (50 55), DPI, polling rate or any other setting. The MM712 has its own plugin
// (coolermaster-mm712); the MM531 is not driven (OpenRGB's detector line for it is commented out).

/* global device */

const PRODUCTS = [
  { pid: 0x0065, name: "Cooler Master MM530", model: "MM530" },
  { pid: 0x0101, name: "Cooler Master MM711", model: "MM711" },
  { pid: 0x0141, name: "Cooler Master MM720", model: "MM7XX" },
  { pid: 0x0165, name: "Cooler Master MM730", model: "MM7XX" },
];

// LEDs in OpenRGB order (Init_Controller), one linear zone = one row. `at` are the three byte
// offsets of the LED's colour in the 51 A8 report (CMMMController's wheel/buttons/logo_bytes,
// CMMM711Controller::SetLedsDirect).
const MODELS = {
  MM530: {
    command: 0x2C, // CM_MM5XX_COMMAND
    leds: [
      { name: "Scroll Wheel", cell: [0, 0], at: [11, 12, 13] },
      { name: "Buttons", cell: [1, 0], at: [5, 6, 7] },
      { name: "Logo", cell: [2, 0], at: [8, 9, 10] },
    ],
  },
  MM7XX: {
    command: 0x2B, // CM_MM7XX_COMMAND
    // The buttons bytes (11..13) stay 0: these mice have no buttons LED.
    leds: [
      { name: "Scroll Wheel", cell: [0, 0], at: [5, 6, 7] },
      { name: "Logo", cell: [1, 0], at: [8, 9, 10] },
    ],
  },
  MM711: {
    command: null, // OpenRGB sends no mode packet when switching the MM711 to Direct
    leds: [
      { name: "Scroll Wheel LED", cell: [0, 0], at: [5, 6, 7] },
      { name: "Logo LED", cell: [1, 0], at: [8, 9, 10] },
    ],
  },
};

const PACKET_SIZE = 65;           // CM_MM_PACKET_SIZE, CM_MM711_PACKET_SIZE
const READ_TIMEOUT_MS = 250;      // CM_MM_INTERRUPT_TIMEOUT, CM_MM711_INTERRUPT_TIMEOUT
const MODE_CUSTOM = 0xB0;         // CM_MM_MODE_CUSTOM: OpenRGB's Direct mode
const BRIGHTNESS_MAX = 0xFF;      // CM_MM_ARGB_BRIGHTNESS_MAX_DEFAULT: Direct's brightness
const KEEPALIVE_MS = 1000;

const DEFAULT_MODEL = MODELS.MM7XX;

export function Name() { return "Cooler Master MasterMouse"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/coolermaster-mastermouse"; }
export function VendorId() { return 0x2516; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
// Placeholders (MM720/MM730); Initialize() sets the product's own.
export function Size() { return [DEFAULT_MODEL.leds.length, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return DEFAULT_MODEL.leds.map((l) => l.name); }
export function LedPositions() { return DEFAULT_MODEL.leds.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
export function ConflictingProcesses() { return ["MasterPlus.exe", "MasterPlusApp.exe"]; }

let product = null;
let model = null;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IPU(..., 1, 0xFF00, 1) for every product here.
export function Validate(endpoint) {
  return endpoint.interface === 1 && endpoint.usage_page === 0xFF00 && endpoint.usage === 0x0001;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  model = product ? MODELS[product.model] : null;
  if (!model) {
    device.log(`Cooler Master MasterMouse: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize([model.leds.length, 1]);
  device.setControllableLeds(model.leds.map((l) => l.name), model.leds.map((l) => l.cell.slice()));
  // Constructor: SendInitPacket. (Its 52 2B/2C, 52 A8 and 52 28 status reads only fill OpenRGB's
  // user interface and are left out.)
  sendAndRead([0x00, 0x41, 0x80]);
  if (model.command !== null) {
    switchToCustom(model.command);
  }
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
  if (setLedsDirect(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step, and its only way back to a hardware mode rewrites the mode's
// parameters (speed, colours), which Lightloom does not know. So the LEDs are set black; nothing
// was saved, so the mouse's stored lighting comes back when it is next powered up.
export function Shutdown(SystemSuspending) {
  if (!model) {
    return;
  }
  setLedsDirect(model.leds.map(() => [0, 0, 0]));
}

// CMMMController::SendUpdate(CM_MM_MODE_CUSTOM, speed 0, colour 0, brightness 0xFF): what
// OpenRGB's switch to its Direct mode sends.
function switchToCustom(command) {
  // SendUsingZonesPacket(0xB0): 51 30, mode byte 0 (1 only for the multilayer mode).
  sendAndRead([0x00, 0x51, 0x30, 0x00, 0x00, 0x00]);
  const buf = new Array(PACKET_SIZE).fill(0);
  buf[1] = 0x51;
  buf[2] = command;
  buf[5] = MODE_CUSTOM;     // CM_MM_MODE_BYTE
  buf[6] = 0x00;            // CM_MM_SPEED_BYTE: Direct's speed
  buf[7] = 0x20;            // CM_MM_NFI_1 (0x00 only for static and spectrum)
  buf[8] = 0xFF;            // CM_MM_NFI_2
  buf[9] = 0xFF;            // CM_MM_NFI_3
  buf[10] = BRIGHTNESS_MAX; // CM_MM_BRIGHTNESS_BYTE
  // 11..16: colour and "sky" colour, all 0 for Direct.
  for (let i = 17; i < PACKET_SIZE; i++) {
    buf[i] = 0xFF;
  }
  sendAndRead(buf);
  // SendApplyPacket(0xB0): "post apply for zoned modes".
  sendAndRead([0x00, 0x51, 0x28, 0x00, 0x00, MODE_CUSTOM]);
}

// SetLedsDirect: 51 A8, each LED's colour at its byte offsets. OpenRGB scales the colours by
// the Direct mode's brightness (0xFF of 0xFF), which leaves them unchanged.
function setLedsDirect(colours) {
  const buf = new Array(PACKET_SIZE).fill(0);
  buf[1] = 0x51;
  buf[2] = 0xA8;
  model.leds.forEach((led, i) => {
    for (let c = 0; c < 3; c++) {
      buf[led.at[c]] = Math.floor(((colours[i][c] & 0xFF) * BRIGHTNESS_MAX) / 0xFF);
    }
  });
  return sendAndRead(buf);
}

// hid_write of the whole 65-byte buffer, then hid_read_timeout(250 ms) for the answer.
function sendAndRead(bytes) {
  const buf = new Array(PACKET_SIZE).fill(0);
  bytes.forEach((b, i) => { buf[i] = b & 0xFF; });
  const ok = device.write(buf, PACKET_SIZE) !== -1;
  device.read([0x00], PACKET_SIZE, READ_TIMEOUT_MS);
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
