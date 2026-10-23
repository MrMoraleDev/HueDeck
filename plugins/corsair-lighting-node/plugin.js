// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the two RGB headers of Corsair's Lighting Node family (Lighting Node Core and
// Pro, Commander Pro, LS100 and LT100 lighting kits, 1000D Obsidian and SPEC OMEGA RGB cases), per
// LED, as two channels of up to 204 LEDs.
// Adapted from OpenRGB Controllers/CorsairLightingNodeController/CorsairLightingNodeController.cpp
// and .h, RGBController_CorsairLightingNode.cpp and .h, CorsairLightingNodeControllerDetect.cpp
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (calcprogrammer1@gmail.com)  12 Jan 2020" (every file of the
// controller).
//
// Protocol notes: docs/hardware/corsair-lighting-node.md. In short: 65-byte output reports
// [0x00, packet id, ...], each followed by a 17-byte input report read (15 ms). Start: firmware
// request (0x02). Every frame, per channel: port state software (0x38 ch 02), the colours 50 LEDs
// at a time as one direct packet per colour component (0x32 ch start count component data), commit
// (0x33 FF). Shutdown: port state hardware (0x38 ch 01) on both channels and a commit, which hands
// the headers back to the device's own stored effect.
// Never sent: effect configuration (0x35), reset (0x37), begin (0x34), brightness (0x39), LED
// count / protocol (0x3A / 0x3B), and nothing about fans or temperatures (the Commander Pro's fan
// and sensor commands are not part of OpenRGB's driver either).

/* global device */

const VENDOR_ID = 0x1B1C;
// CorsairLightingNodeControllerDetect.cpp, REGISTER_HID_DETECTOR (any collection), in file order.
const PRODUCTS = [
  { pid: 0x0C1A, name: "Corsair Lighting Node Core" },
  { pid: 0x0C0B, name: "Corsair Lighting Node Pro" },
  { pid: 0x0C10, name: "Corsair Commander Pro" },
  { pid: 0x0C1E, name: "Corsair LS100 Lighting Kit" },
  { pid: 0x1D00, name: "Corsair 1000D Obsidian" },
  { pid: 0x1D04, name: "Corsair SPEC OMEGA RGB" },
  { pid: 0x0C23, name: "Corsair LT100" },
];

// CorsairLightingNodeController.h
const WRITE_PACKET_SIZE = 65;
const READ_PACKET_SIZE = 17;
const READ_TIMEOUT = 15;
const PACKET_ID_FIRMWARE = 0x02;
const PACKET_ID_DIRECT = 0x32;
const PACKET_ID_COMMIT = 0x33;
const PACKET_ID_PORT_STATE = 0x38;
const PORT_STATE_HARDWARE = 0x01;
const PORT_STATE_SOFTWARE = 0x02;
const NUM_CHANNELS = 2;
const LEDS_PER_PACKET = 50;

// RGBController_CorsairLightingNode::SetupZones: two zones "Corsair RGB Header n", 0..204 LEDs.
const CHANNELS = ["Corsair RGB Header 1", "Corsair RGB Header 2"];
const CHANNEL_LED_LIMIT = 204;

/** Unchanged frames are sent again after this long (the device falls back to its own effect when it
 * gets no commit for about 20 s; OpenRGB's keepalive commits every 5 s). */
const RESEND_MS = 1000;

export function Name() { return "Corsair Lighting Node"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/corsair-lighting-node"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// iCUE holds these controllers while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["iCUE.exe", "Corsair.Service.exe", "CueLLAccessService.exe"]; }

let product = null;
let lastFrame = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR names no interface, usage page or usage: OpenRGB runs the detector on every
// HID collection of the id. Lightloom binds one instance to the first collection Validate accepts, so
// this plugin takes only vendor-defined collections (usage page 0xFF00..0xFFFF), where these
// reports go (as the NZXT Hue 2 plugin does). The products' descriptors are unverified.
export function Validate(endpoint) {
  return endpoint != null && Number(endpoint.usage_page) >= 0xFF00 && Number(endpoint.usage_page) <= 0xFFFF;
}

export function Initialize() {
  lastFrame = null;
  lastSentAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (!product) {
    device.log(`Corsair Lighting Node: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize([1, 1]);
  device.setControllableLeds([], []);
  // CorsairLightingNodeController constructor: SendFirmwareRequest.
  writeAndRead([0x00, PACKET_ID_FIRMWARE]);
  device.SetLedLimit(NUM_CHANNELS * CHANNEL_LED_LIMIT);
  for (const name of CHANNELS) {
    device.addChannel(name, CHANNEL_LED_LIMIT);
  }
}

export function Render() {
  if (!product) {
    return;
  }
  const frame = CHANNELS.map((name) => channelColours(name));
  const now = Date.now();
  if (lastFrame !== null && sameFrame(frame, lastFrame) && now - lastSentAt < RESEND_MS) {
    return;
  }
  // RGBController_CorsairLightingNode::DeviceUpdateLEDs (Direct): every zone in order.
  frame.forEach((colours, channel) => setChannelLeds(channel, colours));
  lastFrame = frame;
  lastSentAt = now;
}

// OpenRGB has no exit step. Lightloom puts both headers back under hardware control with OpenRGB's
// own port-state packet (as its SetChannelEffect does before configuring an effect) and commits,
// so the device runs the effect it has stored. Nothing is configured or stored.
export function Shutdown(SystemSuspending) {
  if (!product) {
    return;
  }
  for (let channel = 0; channel < NUM_CHANNELS; channel++) {
    writeAndRead([0x00, PACKET_ID_PORT_STATE, channel, PORT_STATE_HARDWARE]);
  }
  writeAndRead([0x00, PACKET_ID_COMMIT, 0xFF]);
  lastFrame = null;
}

function channelColours(name) {
  const channel = device.channel(name);
  const rgb = channel ? channel.getColors("Inline", "RGB") : [];
  const count = Math.min(Math.floor((rgb ? rgb.length : 0) / 3), CHANNEL_LED_LIMIT);
  const out = [];
  for (let i = 0; i < count; i++) {
    out.push([rgb[i * 3] & 0xFF, rgb[i * 3 + 1] & 0xFF, rgb[i * 3 + 2] & 0xFF]);
  }
  return out;
}

// CorsairLightingNodeController::SetChannelLEDs.
function setChannelLeds(channel, colours) {
  writeAndRead([0x00, PACKET_ID_PORT_STATE, channel, PORT_STATE_SOFTWARE]);
  let offset = 0;
  let remaining = colours.length;
  while (remaining > 0) {
    const size = Math.min(remaining, LEDS_PER_PACKET);
    const part = colours.slice(offset, offset + size);
    for (let component = 0; component < 3; component++) {
      // SendDirect: channel, start, count, colour component (0 red, 1 green, 2 blue), data.
      writeAndRead([0x00, PACKET_ID_DIRECT, channel, offset & 0xFF, size, component, ...part.map((c) => c[component])]);
    }
    remaining -= size;
    offset += size;
  }
  writeAndRead([0x00, PACKET_ID_COMMIT, 0xFF]);
}

// WriteAndRead: one 65-byte output report, then one bounded input report read.
function writeAndRead(bytes) {
  const buf = new Array(WRITE_PACKET_SIZE).fill(0);
  bytes.forEach((b, i) => { buf[i] = b & 0xFF; });
  device.write(buf, WRITE_PACKET_SIZE);
  device.read([0x00], READ_PACKET_SIZE, READ_TIMEOUT);
}

function sameFrame(a, b) {
  for (let ch = 0; ch < a.length; ch++) {
    if (a[ch].length !== b[ch].length) {
      return false;
    }
    for (let i = 0; i < a[ch].length; i++) {
      if (a[ch][i][0] !== b[ch][i][0] || a[ch][i][1] !== b[ch][i][1] || a[ch][i][2] !== b[ch][i][2]) {
        return false;
      }
    }
  }
  return true;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
