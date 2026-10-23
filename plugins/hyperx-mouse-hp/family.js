// SPDX-License-Identifier: GPL-3.0-or-later
//
// HyperX Pulsefire mice in OpenRGB's Direct mode: Surge, FPS Pro, Core, Dart (wired and
// wireless), Raid and Haste (Kingston 0x0951 and HP 0x03F0 product ids). Shared, byte for byte, by
// plugins/hyperx-mouse and plugins/hyperx-mouse-hp (a plugin has a single vendor id, hence two
// directories; the tests check the copies match).
// Adapted from OpenRGB Controllers/HyperXMouseController/HyperXMouseControllerDetect.cpp and the
// HyperXPulsefireSurgeController/, HyperXPulsefireFPSProController/, HyperXPulsefireHasteController/,
// HyperXPulsefireDartController/ and HyperXPulsefireRaidController/ controller and RGBController
// files (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors, as in the file headers: "Adam Honse (CalcProgrammer1)  25 Jul 2020" (Pulsefire
// Surge), "Adam Honse (CalcProgrammer1)  26 Dec 2020" (Pulsefire FPS Pro), "Adam Honse
// (CalcProgrammer1)  19 Aug 2020" (Pulsefire Haste), "Santeri Pikarinen (santeri3700)
// 26 Dec 2020" (Pulsefire Dart), "Morgan Guimard (morg)  06 Apr 2022" (Pulsefire Raid); the
// detector file names no author ("This file is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/hyperx-mouse.md. Only the Direct colour reports are sent: never
// the Dart's save command (0xDE, "Save current settings to the on-board memory"), never the
// Surge's profile select / profile brightness commands (0x07 / 0x03), never DPI, polling rate or
// anything else that is not lighting.

// HyperXMouseControllerDetect.cpp registrations (the _WIN32 branch for the Haste).
export const PRODUCTS = [
  { vid: 0x0951, pid: 0x16D3, name: "HyperX Pulsefire Surge", variant: "SURGE", iface: 1, page: 0xFF01 },
  { vid: 0x03F0, pid: 0x0490, name: "HyperX Pulsefire Surge (HP)", variant: "SURGE", iface: 1, page: 0xFF01 },
  { vid: 0x0951, pid: 0x16D7, name: "HyperX Pulsefire FPS Pro", variant: "FPS_PRO", iface: 1, page: 0xFF01 },
  { vid: 0x0951, pid: 0x16DE, name: "HyperX Pulsefire Core", variant: "FPS_PRO", iface: 1, page: 0xFF01 },
  { vid: 0x03F0, pid: 0x0D8F, name: "HyperX Pulsefire Core (HP)", variant: "FPS_PRO", iface: 1, page: 0xFF01 },
  { vid: 0x0951, pid: 0x16E1, name: "HyperX Pulsefire Dart (Wireless)", variant: "DART", iface: 2, page: 0xFF00 },
  { vid: 0x03F0, pid: 0x068E, name: "HyperX Pulsefire Dart (Wireless)", variant: "DART", iface: 2, page: 0xFF00 },
  { vid: 0x0951, pid: 0x16E2, name: "HyperX Pulsefire Dart (Wired)", variant: "DART", iface: 1, page: 0xFF13 },
  { vid: 0x03F0, pid: 0x088E, name: "HyperX Pulsefire Dart (Wired)", variant: "DART", iface: 1, page: 0xFF13 },
  { vid: 0x0951, pid: 0x16E4, name: "HyperX Pulsefire Raid", variant: "RAID", iface: 1, page: 0xFF01, usage: 0x01 },
  { vid: 0x03F0, pid: 0x0F8F, name: "HyperX Pulsefire Haste", variant: "HASTE", iface: 3, page: 0xFF90 },
  { vid: 0x0951, pid: 0x1727, name: "HyperX Pulsefire Haste", variant: "HASTE", iface: 3, page: 0xFF90 },
];

// Per variant: LEDs (name, grid cell) in OpenRGB order, grid size, frame writer, and how long an
// unchanged frame may go unsent (0 = a frame on every Render(): OpenRGB's keepalive threads
// resend whenever 50 ms passed without an update; Raid: 1 s; the Dart has no keepalive).
const VARIANTS = {
  SURGE: {
    leds: [...Array.from({ length: 32 }, (_, i) => [`LED Strip LED ${i + 1}`, i, 0]), ["Logo", 0, 1]],
    size: [32, 2],
    frame: surgeFrame,
    resendMs: 0,
  },
  FPS_PRO: { leds: [["Logo", 0, 0]], size: [1, 1], frame: fpsProFrame, resendMs: 0 },
  HASTE: { leds: [["Logo", 0, 0]], size: [1, 1], frame: hasteFrame, resendMs: 0 },
  DART: { leds: [["Scroll Wheel", 0, 0], ["Logo", 1, 0]], size: [2, 1], frame: dartFrame, resendMs: 1000 },
  RAID: { leds: [["Scroll Wheel", 0, 0], ["Logo", 1, 0]], size: [2, 1], frame: raidFrame, resendMs: 1000 },
};

export function productFor(vid, pid) {
  return PRODUCTS.find((p) => p.vid === vid && p.pid === pid) ?? null;
}

// REGISTER_HID_DETECTOR_IP (interface, usage page) and _IPU (also the usage).
export function validate(product, endpoint) {
  if (!product || endpoint.interface !== product.iface || endpoint.usage_page !== product.page) {
    return false;
  }
  return product.usage === undefined || endpoint.usage === product.usage;
}

/** Sets the device's name and LED grid; returns the session state (null for an unknown product). */
export function start(dev, product) {
  if (!product) {
    return null;
  }
  const variant = VARIANTS[product.variant];
  dev.setName(product.name);
  dev.setSize(variant.size.slice());
  dev.setControllableLeds(variant.leds.map((l) => l[0]), variant.leds.map((l) => [l[1], l[2]]));
  // OpenRGB's constructors send nothing: the first Direct frame takes the mouse over.
  return { variant, last: null, sentAt: 0 };
}

export function render(dev, st) {
  if (!st) {
    return;
  }
  const colours = st.variant.leds.map((l) => rgb(dev.color(l[1], l[2])));
  const now = Date.now();
  if (st.variant.resendMs > 0 && st.last !== null && same(colours, st.last) && now - st.sentAt < st.variant.resendMs) {
    return;
  }
  st.variant.frame(dev, colours);
  st.last = colours;
  st.sentAt = now;
}

/**
 * OpenRGB has no exit step and no "back to the stored lighting" command for these mice: one black
 * Direct frame. (OpenRGB notes that the Raid "will reset to default (flash)" without its
 * keepalive, so it may go back to its own lighting once frames stop coming.)
 */
export function shutdown(dev, st) {
  if (!st) {
    return;
  }
  st.variant.frame(dev, st.variant.leds.map(() => [0, 0, 0]));
}

// HyperXPulsefireSurgeController::SendDirect: one 264-byte feature report 07 14 00 A0, the strip's
// red, green and blue planes at 0x08 / 0x28 / 0x48, the logo at 0x6C.
function surgeFrame(dev, colours) {
  const buf = new Array(264).fill(0);
  buf[0x00] = 0x07;
  buf[0x01] = 0x14;
  buf[0x03] = 0xA0;
  for (let i = 0; i < 32; i++) {
    buf[0x08 + i] = colours[i][0];
    buf[0x28 + i] = colours[i][1];
    buf[0x48 + i] = colours[i][2];
  }
  buf[0x6C] = colours[32][0];
  buf[0x6D] = colours[32][1];
  buf[0x6E] = colours[32][2];
  dev.send_report(buf, 264);
}

// HyperXPulsefireFPSProController::SendDirect: 264-byte feature report 07 0A R G B ... [8] = A0.
function fpsProFrame(dev, colours) {
  const buf = new Array(264).fill(0);
  buf[0x00] = 0x07;
  buf[0x01] = 0x0A;
  buf[0x02] = colours[0][0];
  buf[0x03] = colours[0][1];
  buf[0x04] = colours[0][2];
  buf[0x08] = 0xA0;
  dev.send_report(buf, 264);
}

// HyperXPulsefireHasteController::SendDirect: setup 00 04 F2 ... [8] = 02, then 00 81 R G B ...
// [8] = 02; two 65-byte feature reports.
function hasteFrame(dev, colours) {
  const setup = new Array(65).fill(0);
  setup[0x01] = 0x04;
  setup[0x02] = 0xF2;
  setup[0x08] = 0x02;
  dev.send_report(setup, 65);
  const buf = new Array(65).fill(0);
  buf[0x01] = 0x81;
  buf[0x02] = colours[0][0];
  buf[0x03] = colours[0][1];
  buf[0x04] = colours[0][2];
  buf[0x08] = 0x02;
  dev.send_report(buf, 65);
}

// RGBController_HyperXPulsefireDart::DeviceUpdateMode in Direct (static mode 0x00, brightness
// 0x64, speed 0): HyperXPulsefireDartController::SendDirect for the scroll wheel (LED 0x10), then
// the logo (LED 0x00); 65-byte output reports 00 D2 <led> 00 08 R G B R G B 64 00, 2 ms apart.
function dartFrame(dev, colours) {
  dartLed(dev, colours[0], 0x10);
  dartLed(dev, colours[1], 0x00);
}

function dartLed(dev, [r, g, b], led) {
  const buf = new Array(65).fill(0);
  buf[0x01] = 0xD2;
  buf[0x02] = led;
  buf[0x03] = 0x00;
  buf[0x04] = 0x08;
  buf[0x05] = r;
  buf[0x06] = g;
  buf[0x07] = b;
  buf[0x08] = r;
  buf[0x09] = g;
  buf[0x0A] = b;
  buf[0x0B] = 0x64;
  buf[0x0C] = 0x00;
  dev.write(buf, 65);
  dev.pause(2);
}

// HyperXPulsefireRaidController::SendColors: 264-byte feature report 07 0A, scroll wheel and logo
// R G B, [8] = A0; then 10 ms.
function raidFrame(dev, colours) {
  const buf = new Array(264).fill(0);
  buf[0] = 0x07;
  buf[1] = 0x0A;
  for (let i = 0; i < 2; i++) {
    buf[3 * i + 2] = colours[i][0];
    buf[3 * i + 3] = colours[i][1];
    buf[3 * i + 4] = colours[i][2];
  }
  buf[8] = 0xA0;
  dev.send_report(buf, 264);
  dev.pause(10);
}

function rgb(c) {
  return c && c.length >= 3 ? [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF] : [0, 0, 0];
}

function same(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) {
      return false;
    }
  }
  return true;
}
