// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the ARGB lights of the Cooler Master GM27-FQS and GM34-CWQ ARGB monitors and of
// the GD160 ARGB gaming desk, per LED, in their software (direct) mode.
// Adapted from OpenRGB Controllers/CoolerMasterController/CoolerMasterControllerDetect.cpp,
// CMMonitorController/CMMonitorController.cpp and .h, CMMonitorController/
// RGBController_CMMonitorController.cpp and .h, CMGD160Controller/CMGD160Controller.cpp and .h,
// CMGD160Controller/RGBController_CMGD160Controller.cpp and .h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Morgan Guimard (morg)  18 Sep 2023" (CMMonitorController,
// RGBController_CMMonitorController); "Logan Phillips (Eclipse)  16 Oct 2025" (CMGD160Controller,
// RGBController_CMGD160Controller); CoolerMasterControllerDetect.cpp names no author ("This file
// is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/coolermaster-monitor-desk.md. In short: interface 0 (usage page
// 0xFF00, usage 1), 65-byte output reports (report id 0x00). First frame: software mode on
// (80 07 02 <side> 00 01; the desk has two sides). Every frame: per side seven packets 00..05, 86
// with the colours. Shutdown: software mode off (80 07 02 <side> 00 00), back to the device's own
// effect. Never sent: hardware-effect or "custom" colour writes (80 0B / 0F, 10 ...), which the
// device stores, and the GD160's unexplained start-up "reset" (80 11, 80 0B 01 02, 80 18 01 02).

/* global device */

const PRODUCTS = [
  { pid: 0x01BB, name: "Cooler Master GM27-FQS ARGB Monitor", model: "MONITOR" },
  { pid: 0x01BF, name: "Cooler Master GM34-CWQ ARGB Monitor", model: "MONITOR" },
  { pid: 0x01A9, name: "Cooler Master GD160 ARGB Gaming Desk", model: "GD160" },
];

// Per side: the side byte, the first-packet bytes 5 and 6 (CMMonitorController::SendDirect:
// 01 80; CMGD160Controller::SendColorData(colors, 0x07, 0x01, 0xFF, true): 01 FF) and the LEDs.
// LEDs are OpenRGB's (one linear zone per side, one row per zone).
const MONITOR_LEDS = 47;   // RGBController_CMMonitorController::SetupZones
const DESK_LEDS = 96;      // CM_GD160_LEDS_PER_SIDE
const MODELS = {
  MONITOR: {
    size: [MONITOR_LEDS, 1],
    sides: [{ side: 0x02, mode: 0x01, brightness: 0x80, row: 0, count: MONITOR_LEDS, name: (i) => String(i + 1) }],
  },
  GD160: {
    size: [DESK_LEDS, 2],
    sides: [
      { side: 0x01, mode: 0x01, brightness: 0xFF, row: 0, count: DESK_LEDS, name: (i) => `Front LED ${i + 1}` },
      { side: 0x02, mode: 0x01, brightness: 0xFF, row: 1, count: DESK_LEDS, name: (i) => `Back LED ${i + 1}` },
    ],
  },
};

const PACKET_SIZE = 65;       // CM_MONITOR_PACKET_LENGTH, CM_GD160_PACKET_LENGTH
const SIDE_DATA_LENGTH = 436; // CM_MONITOR_COLOR_DATA_LENGTH, CM_GD160_SIDE_DATA_LENGTH
const KEEPALIVE_MS = 1000;

export function Name() { return "Cooler Master ARGB Monitor / Desk"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/coolermaster-monitor-desk"; }
export function VendorId() { return 0x2516; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
// Placeholders (the monitors'); Initialize() sets the product's own.
export function Size() { return MODELS.MONITOR.size.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 4.0; }
export function LedNames() { return leds(MODELS.MONITOR).map((l) => l.name); }
export function LedPositions() { return leds(MODELS.MONITOR).map((l) => l.cell); }
export function ImageUrl() { return ""; }
export function ConflictingProcesses() { return ["MasterPlus.exe", "MasterPlusApp.exe"]; }

let model = null;
let ledList = [];
let softwareMode = false;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IPU(..., 0, 0xFF00, 1) for all three.
export function Validate(endpoint) {
  return endpoint.interface === 0 && endpoint.usage_page === 0xFF00 && endpoint.usage === 0x0001;
}

export function Initialize() {
  softwareMode = false;
  lastColours = null;
  lastSentAt = 0;
  const product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  model = product ? MODELS[product.model] : null;
  if (!model) {
    device.log(`Cooler Master monitor / desk: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  ledList = leds(model);
  device.setName(product.name);
  device.setSize(model.size.slice());
  device.setControllableLeds(ledList.map((l) => l.name), ledList.map((l) => l.cell));
  // Nothing is sent here: Direct is OpenRGB's first mode and switching to it sends nothing; the
  // first frame turns software mode on.
}

export function Render() {
  if (!model) {
    return;
  }
  const colours = ledList.map((l) => device.color(l.cell[0], l.cell[1]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (sendDirect(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step; its own way out of Direct (SetMode) starts by turning software mode
// off, which hands the lights back to the device's own (stored) effect. HueDeck sends only that.
export function Shutdown(SystemSuspending) {
  if (!model) {
    return;
  }
  setSoftwareMode(false);
}

// SetSoftwareModeEnabled (monitor) / SetControlMode (desk, one report per side).
function setSoftwareMode(on) {
  let ok = true;
  for (const s of model.sides) {
    const buf = new Array(PACKET_SIZE).fill(0);
    buf[1] = 0x80;
    buf[2] = 0x07;
    buf[3] = 0x02;
    buf[4] = s.side;
    buf[6] = on ? 0x01 : 0x00;
    ok = write(buf) && ok;
  }
  softwareMode = on && ok;
  return ok;
}

// SendDirect / SendColorData: per side a 436-byte colour buffer (R, G, B per LED) in seven
// packets: 00 (07 02 <side> <mode> <brightness> + 58 bytes), 01..05 (63 bytes each), 86.
function sendDirect(colours) {
  let ok = softwareMode || setSoftwareMode(true);
  let index = 0;
  for (const s of model.sides) {
    const data = new Array(SIDE_DATA_LENGTH).fill(0);
    for (let i = 0; i < s.count; i++, index++) {
      data[i * 3] = colours[index][0] & 0xFF;
      data[i * 3 + 1] = colours[index][1] & 0xFF;
      data[i * 3 + 2] = colours[index][2] & 0xFF;
    }
    let offset = 0;
    for (let p = 0; p < 7; p++) {
      const buf = new Array(PACKET_SIZE).fill(0);
      buf[1] = p < 6 ? p : 0x86;
      if (p === 0) {
        buf[2] = 0x07;
        buf[3] = 0x02;
        buf[4] = s.side;
        buf[5] = s.mode;
        buf[6] = s.brightness;
        for (let i = 0; i < PACKET_SIZE - 7; i++) {
          buf[7 + i] = data[offset + i];
        }
        offset += PACKET_SIZE - 7;
      } else {
        for (let i = 0; i < PACKET_SIZE - 2; i++) {
          buf[2 + i] = data[offset + i];
        }
        offset += PACKET_SIZE - 2;
      }
      ok = write(buf) && ok;
    }
  }
  return ok;
}

function leds(m) {
  const list = [];
  for (const s of m.sides) {
    for (let i = 0; i < s.count; i++) {
      list.push({ name: s.name(i), cell: [i, s.row] });
    }
  }
  return list;
}

function write(buf) {
  return device.write(buf, PACKET_SIZE) !== -1;
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
