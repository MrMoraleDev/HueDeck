// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the light of the SteelSeries Siberia 350 headset.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesSiberiaController/
// SteelSeriesSiberiaController.cpp, SteelSeriesSiberiaController.h,
// RGBController_SteelSeriesSiberia.cpp, RGBController_SteelSeriesSiberia.h, and
// Controllers/SteelSeriesController/SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "E Karlsson (pilophae)  18 Jun 2020" (SteelSeriesSiberiaController,
// RGBController_SteelSeriesSiberia); SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h name
// no author.
//
// Protocol notes: docs/hardware/steelseries-siberia.md. In short: interface 3, four 16-byte
// output reports with report id 0x01 per colour: three fixed commands OpenRGB calls "Command 1, 2
// and 4" around the colour command 0x83 R G B. What the three fixed commands do is not documented
// in OpenRGB; they are sent exactly as OpenRGB sends them, and nothing else is.

/* global device */

const PRODUCTS = [{ pid: 0x1229, name: "SteelSeries Siberia 350" }];

const LEDS = [{ name: "Headset LED", cell: [0, 0] }];
const SIZE = [1, 1];
const REPORT_ID = 0x01;
const REPORT_LENGTH = 16;

export function Name() { return "SteelSeries Siberia 350"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-siberia"; }
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
// SteelSeries' own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let enabled = false;
let lastColour = null;

// REGISTER_HID_DETECTOR_I(..., 3): any collection of interface 3.
export function Validate(endpoint) {
  return endpoint.interface === 3;
}

export function Initialize() {
  lastColour = null;
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  enabled = Boolean(product);
  if (!product) {
    device.log(`SteelSeries Siberia: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(SIZE.slice());
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // OpenRGB's only mode, Static, sends the colour and nothing else (DeviceUpdateMode ->
  // DeviceUpdateLEDs); the first Render() does that.
}

export function Render() {
  if (!enabled) {
    return;
  }
  const [r, g, b] = device.color(LEDS[0].cell[0], LEDS[0].cell[1]);
  const colour = [r & 0xFF, g & 0xFF, b & 0xFF];
  // Sent only when the colour changes, with no keepalive: three of the four commands are not
  // explained in OpenRGB (see the notes), so they go out as rarely as possible.
  if (lastColour !== null && colour.every((v, i) => v === lastColour[i])) {
    return;
  }
  if (setColor(colour[0], colour[1], colour[2])) {
    lastColour = colour;
  }
}

// OpenRGB has no exit step and no other mode: the light is switched off (black).
export function Shutdown(SystemSuspending) {
  if (!enabled) {
    return;
  }
  setColor(0, 0, 0);
}

// SteelSeriesSiberiaController::SetColor.
function setColor(r, g, b) {
  // Commands 1, 2 and 4 are copied from OpenRGB without an explanation there; they may be
  // register writes of the headset's controller.
  let ok = sendUsbMsg([0x95, 0x80, 0xBF]); // "Command 1"
  ok = sendUsbMsg([0x80, 0x52, 0x20]) && ok; // "Command 2"
  ok = sendUsbMsg([0x83, r, g, b]) && ok; // "Command 3 (Set color)"
  return sendUsbMsg([0x93, 0x03, 0x80]) && ok; // "Command 4"
}

// send_usb_msg(): report id 0x01, a 0x00 "magic" byte, the command, the payload length, the
// payload; 16 bytes in all.
function sendUsbMsg(data) {
  const pkt = new Array(REPORT_LENGTH).fill(0);
  pkt[0] = REPORT_ID;
  pkt[1] = 0x00;
  pkt[2] = data[0];
  pkt[3] = data.length - 1;
  for (let i = 1; i < data.length; i++) {
    pkt[3 + i] = data[i];
  }
  return device.write(pkt, REPORT_LENGTH) !== -1;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
