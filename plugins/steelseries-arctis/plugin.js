// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the two ear-cup lights of the first SteelSeries Arctis 5 headset (USB
// 1038:1250), following OpenRGB's driver.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesArctis5Controller/
// SteelSeriesArctis5Controller.cpp, SteelSeriesArctis5Controller.h,
// RGBController_SteelSeriesArctis5.cpp, RGBController_SteelSeriesArctis5.h, and
// Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Morgan Guimard  04 Mar 2022" (SteelSeriesArctis5Controller,
// RGBController_SteelSeriesArctis5); SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h name
// no author.
//
// Protocol notes: docs/hardware/steelseries-arctis.md. In short: interface 5, output report 0x06
// (37 bytes): per ear cup and frame two 0x81 0x43 reports, the colour, the zone, an apply and a
// commit. Only lighting reports are sent; volume, sidetone, microphone and the other headset
// settings are never touched.
//
// OpenRGB's detector also lists 1038:12AA (Arctis 5, second version). This plugin does not claim
// it: HueDeck's own plugins/steelseries-arctis-5 owns 12AA (written from recordings and confirmed
// on hardware), and OpenRGB's stream differs from the recorded working one (see the notes). The
// host's plugin index lets the last scanned plugin win a VID:PID, so two claims would make the
// choice depend on scan order.

/* global device */

const PRODUCTS = [
  { pid: 0x1250, name: "SteelSeries Arctis 5" },
];

// RGBController_SteelSeriesArctis5::SetupZones: zone 0 "Left", zone 1 "Right" (one LED each,
// sharing one grid row).
const LEDS = [
  { name: "Left", zone: 0, cell: [0, 0] },
  { name: "Right", zone: 1, cell: [1, 0] },
];
const SIZE = [2, 1];
const REPORT_ID = 0x06;
const REPORT_LENGTH = 37;

export function Name() { return "SteelSeries Arctis 5 (OpenRGB protocol)"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-arctis"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "headphones"; }
export function Size() { return SIZE.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LEDS.map((l) => l.name); }
export function LedPositions() { return LEDS.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let enabled = false;

// REGISTER_HID_DETECTOR_I(..., 5) for both products: any collection of interface 5.
export function Validate(endpoint) {
  return endpoint.interface === 5;
}

export function Initialize() {
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  enabled = Boolean(product);
  if (!product) {
    device.log(`SteelSeries Arctis 5: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(SIZE.slice());
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // OpenRGB's Direct mode has no mode command (DeviceUpdateMode is empty): nothing to send.
}

// Every frame, both ear cups, as OpenRGB's DeviceUpdateLEDs does (and as the recorded
// SignalRGB-compatible session did, see the notes).
export function Render() {
  if (!enabled) {
    return;
  }
  for (const led of LEDS) {
    const [r, g, b] = device.color(led.cell[0], led.cell[1]);
    setColor(led.zone, r & 0xFF, g & 0xFF, b & 0xFF);
  }
}

// OpenRGB has no exit step and no onboard-lighting command for this headset: the ear cups are
// switched off (black).
export function Shutdown(SystemSuspending) {
  if (!enabled) {
    return;
  }
  for (const led of LEDS) {
    setColor(led.zone, 0, 0, 0);
  }
}

// SteelSeriesArctis5Controller::SetColor: six reports of 37 bytes per zone.
function setColor(zoneId, r, g, b) {
  // "Two packets are sent before a color change"
  let ok = send([0x81, 0x43, 0x01, 0x22]);
  ok = send([0x81, 0x43, 0x01, 0x23]) && ok;
  // "First packet: send color"
  ok = send([0x8A, 0x42, 0x00, 0x20, 0x41, 0x00, r, g, b, 0xFF, 0x32, 0xC8, 0xC8]) && ok;
  // "Second packet: apply to zone"
  ok = send([0x8A, 0x42, 0x00, 0x20, 0x41, 0x08, zoneId, 0x01]) && ok;
  // "Thrid packet: apply to zone"
  ok = send([0x8A, 0x42, 0x00, 0x20, 0x60, zoneId]) && ok;
  // "Last packet: apply"
  return send([0x8A, 0x42, 0x00, 0x20, 0x05]) && ok;
}

/** Report 0x06 with `body` after the report id, zero-padded to 37 bytes. */
function send(body) {
  const pkt = new Array(REPORT_LENGTH).fill(0);
  pkt[0] = REPORT_ID;
  body.forEach((v, i) => { pkt[1 + i] = v; });
  return device.write(pkt, REPORT_LENGTH) !== -1;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
