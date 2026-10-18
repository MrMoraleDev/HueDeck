// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck first-party plugin: SteelSeries Arctis 5 headset (2019 edition, USB 1038:12AA),
// the two ear-cup lights.
//
// Protocol facts come from HueDeck's own recordings of the headset's HID traffic, written up in
// docs/hardware/steelseries-arctis-5.md; no code of any other project was used. In short:
// output reports on interface 5; report 0x06 (37 bytes) carries the lighting commands. Per zone
// and frame: two "43 01" packets, the colour (R G B), the zone the colour goes to, an apply and a
// commit. Two 0x04 packets (39 bytes) are sent once at start, as recorded; what they set is not
// known.

/* global device */

export function Name() { return "SteelSeries Arctis 5"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-arctis-5"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return [0x12AA]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "headphones"; }
export function Size() { return [2, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Left ear cup", "Right ear cup"]; }
export function LedPositions() { return [[0, 0], [1, 0]]; }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

const COMMAND_LENGTH = 37;
const SETUP_LENGTH = 39;

export function Validate(endpoint) {
  return endpoint.interface === 5;
}

export function Initialize() {
  // Sent once at start in every recording (meaning unknown).
  send([0x04, 0x40, 0x06, 0x10, 0xF6, 0x60, 0x09, 0x2C, 0xFF, 0x7F], SETUP_LENGTH);
  send([0x04, 0x40, 0x01, 0x11, 0x54, 0x9B], SETUP_LENGTH);
  send([0x06, 0x8A, 0x42, 0x00, 0x20, 0x05]);
  send([0x06, 0x81, 0x43, 0x01, 0x23]);
}

export function Render() {
  for (let zone = 0; zone < 2; zone++) {
    sendZone(zone, device.color(zone, 0));
  }
}

export function Shutdown() {
  for (let zone = 0; zone < 2; zone++) {
    sendZone(zone, [0, 0, 0]);
  }
}

/** One zone's colour, in the order the headset was seen to take it. */
function sendZone(zone, rgb) {
  send([0x06, 0x81, 0x43, 0x01, 0x22]);
  send([0x06, 0x81, 0x43, 0x01, 0x23]);
  send([0x06, 0x8A, 0x42, 0x00, 0x20, 0x41, 0x00, rgb[0] & 0xFF, rgb[1] & 0xFF, rgb[2] & 0xFF, 0xFF, 0x32, 0xC8, 0xC8]);
  send([0x06, 0x8A, 0x42, 0x00, 0x20, 0x41, 0x08, zone, 0x01]);
  send([0x06, 0x8A, 0x42, 0x00, 0x20, 0x60, zone]);
  send([0x06, 0x8A, 0x42, 0x20, 0x05]);
}

function send(bytes, length = COMMAND_LENGTH) {
  const packet = new Array(length).fill(0);
  for (let i = 0; i < bytes.length && i < length; i++) {
    packet[i] = bytes[i];
  }
  device.write(packet, length);
}
