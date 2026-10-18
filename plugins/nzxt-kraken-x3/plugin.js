// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck first-party plugin: NZXT Kraken X53 / X63 / X73 ("X3") liquid coolers, lighting only.
//
// Protocol facts come from HueDeck's own recordings of the device's HID traffic, written up in
// docs/hardware/nzxt-kraken-x3.md; no code of any other project was used. In short: 64-byte
// output reports; per channel two colour packets (0x22 0x10 / 0x22 0x11, 20 LEDs each, colour
// order G R B) and an apply packet (0x22 0xA0); the logo LED has its own packet (0x2A 0x04).
// Channel 1 is the RGB header on the pump (fans, strips: up to 40 LEDs), channel 2 the pump ring
// (8 LEDs, clockwise from 12 o'clock). Pump and fan speeds are never touched.

/* global device */

export function Name() { return "NZXT Kraken X3"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/nzxt-kraken-x3"; }
export function VendorId() { return 0x1E71; }
// Kraken X53, X63 and X73 share this product id.
export function ProductId() { return [0x2007]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "aio"; }
export function Size() { return [5, 5]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function ImageUrl() { return ""; }
// NZXT's own software holds the device while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["NZXT CAM.exe"]; }

// The pump ring on a 5x5 grid, in the device's LED order (clockwise from 12 o'clock); the
// logo sits in the middle of the cap.
const RING = [[3, 0], [4, 1], [4, 3], [3, 4], [1, 4], [0, 3], [0, 1], [1, 0]];
const LOGO = [2, 2];

export function LedNames() {
  return [...RING.map((_, i) => `Pump ring ${i + 1}`), "Logo"];
}
export function LedPositions() { return [...RING, LOGO]; }

/** Name of the RGB header channel (the name SignalRGB setups use too, so imports match). */
const HEADER_CHANNEL = "Channel 1";
/** LEDs the RGB header takes. */
const HEADER_LEDS = 40;

const REPORT_LENGTH = 64;
const LEDS_PER_PACKET = 20;
const HEADER = 0x01;
const PUMP_RING = 0x02;

export function Validate(endpoint) {
  return endpoint.interface === 0 || endpoint.interface === -1;
}

export function Initialize() {
  device.addChannel(HEADER_CHANNEL, HEADER_LEDS);
}

export function Render() {
  sendChannel(HEADER, headerColors());
  sendChannel(PUMP_RING, RING.flatMap(([x, y]) => grb(device.color(x, y))));
  sendLogo(grb(device.color(LOGO[0], LOGO[1])));
}

export function Shutdown() {
  // Dark, in the order the device was seen to accept it.
  sendChannel(PUMP_RING, new Array(RING.length * 3).fill(0));
  sendChannel(HEADER, new Array(HEADER_LEDS * 3).fill(0));
  sendLogo([0, 0, 0]);
}

/** The header channel's colours, G R B, exactly HEADER_LEDS long. */
function headerColors() {
  const channel = device.channel(HEADER_CHANNEL);
  const colors = channel ? channel.getColors("Inline", "GRB") : [];
  const out = new Array(HEADER_LEDS * 3).fill(0);
  for (let i = 0; i < out.length && i < colors.length; i++) {
    out[i] = colors[i] & 0xFF;
  }
  return out;
}

function grb(rgb) {
  return [rgb[1] & 0xFF, rgb[0] & 0xFF, rgb[2] & 0xFF];
}

/** Two colour packets (20 LEDs each) and the apply packet for one channel. */
function sendChannel(channel, colors) {
  for (let half = 0; half < 2; half++) {
    const start = half * LEDS_PER_PACKET * 3;
    const slice = colors.slice(start, start + LEDS_PER_PACKET * 3);
    write([0x22, 0x10 + half, channel, 0x00, ...slice]);
  }
  write([0x22, 0xA0, channel, 0x00, 0x01, 0x00, 0x00, 0x28, 0x00, 0x00, 0x80, 0x00, 0x32, 0x00, 0x00, 0x01]);
}

function sendLogo(color) {
  const packet = [0x2A, 0x04, 0x04, 0x04, 0x00, 0x32, 0x00, ...color];
  // Constant in every recorded logo packet (meaning unknown).
  packet.length = 60;
  packet.fill(0, 10, 56);
  packet.splice(56, 4, 0x01, 0x00, 0x01, 0x03);
  write(packet);
}

function write(bytes) {
  const packet = new Array(REPORT_LENGTH).fill(0);
  for (let i = 0; i < bytes.length && i < REPORT_LENGTH; i++) {
    packet[i] = bytes[i];
  }
  device.write(packet, REPORT_LENGTH);
}
