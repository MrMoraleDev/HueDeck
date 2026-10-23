// SPDX-License-Identifier: GPL-3.0-or-later
//
// Wooting keyboards (One, Two, Two Lekker Edition, Two HE, 60HE family, 80HE, UwU RGB) in
// OpenRGB's Direct mode, over the three report formats OpenRGB knows: V1 (legacy firmware on the
// Atmel vendor id 0x03EB), V2 (firmware up to 2.11, usage page 0x1337) and V3 (firmware 2.12 and
// later, usage page 0xFF55). Shared, byte for byte, by plugins/wooting (0x31E3) and
// plugins/wooting-legacy (0x03EB): a plugin has a single vendor id, hence two directories; the
// tests check the copies match.
// Adapted from OpenRGB Controllers/WootingKeyboardController/: WootingKeyboardControllerDetect.cpp,
// WootingKeyboardController.cpp / .h, RGBController_WootingKeyboard.cpp / .h,
// WootingV1KeyboardController/*, WootingV2KeyboardController/*, WootingV3KeyboardController/*
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors, as in the file headers: "Chris M (Dr_No)  09 Jul 2021" and "Diogo Trindade
// (diogotr7)  25 Dec 2025".
//
// Protocol notes: docs/hardware/wooting.md. Sent: the colour-init command (0x21) at start, the
// raw colour report (11) every frame and, at shutdown, the reset-all colour command (0x20), which
// OpenRGB's header defines but never sends: Wooting's public RGB SDK uses it to give the keys
// their profile colours back. Nothing else (no analog, profile or setting command).

import { LAYOUTS } from "./layouts.js";

const NEW_VID = 0x31E3;
const OLD_VID = 0x03EB;
const PAGE_V2 = 0x1337;
const PAGE_V3 = 0xFF55;

const RAW_COLORS_REPORT = 11;
const RESET_ALL_COMMAND = 32;
const COLOR_INIT_COMMAND = 33;

// REGISTER_WOOTING_DETECTOR: the base id with the gamepad-emulation nibble 2 (none), 0 (Xbox) and
// 1 (DirectInput).
const MODELS = [
  ["Wooting One", 0x1100], ["Wooting Two", 0x1200], ["Wooting Two Lekker Edition", 0x1210],
  ["Wooting Two HE", 0x1220], ["Wooting Two HE (ARM)", 0x1230], ["Wooting 60HE", 0x1300],
  ["Wooting 60HE (ARM)", 0x1310], ["Wooting 60HE+", 0x1320], ["Wooting 60HEv2", 0x1340],
  ["Wooting 80HE", 0x1400], ["Wooting UwU RGB", 0x1510],
];

export const PRODUCTS = [
  ...MODELS.flatMap(([name, base]) => [base | 2, base | 0, base | 1].map((pid) => ({ vid: NEW_VID, pid, name }))),
  { vid: OLD_VID, pid: 0xFF01, name: "Wooting One (Legacy)" },
  { vid: OLD_VID, pid: 0xFF02, name: "Wooting Two (Legacy)" },
];

export function productFor(vid, pid) {
  return PRODUCTS.find((p) => p.vid === vid && p.pid === pid) ?? null;
}

// DetectWootingControllers: the keyboard type from the product id.
function wootingType(pid) {
  if (pid === 0xFF01) return "TKL";
  if (pid === 0xFF02) return "FULL";
  switch (pid & 0xFFF0) {
    case 0x1100: return "TKL";
    case 0x1200: case 0x1210: case 0x1220: case 0x1230: return "FULL";
    case 0x1300: case 0x1310: case 0x1320: case 0x1340: return "60PER";
    case 0x1400: return "80PER";
    case 0x1510: return "3PAD";
    default: return "FULL";
  }
}

// The collections the detector drives: 0x03EB with usage page 0x1337 is V1; on 0x31E3 usage page
// 0x1337 is V2 and 0xFF55 is V3 (any interface, any usage).
function pages(vid) {
  return vid === OLD_VID ? [PAGE_V2] : [PAGE_V3, PAGE_V2];
}

function protocolFor(vid, page) {
  if (page === PAGE_V2) return vid === OLD_VID ? "V1" : "V2";
  if (page === PAGE_V3 && vid === NEW_VID) return "V3";
  return null;
}

export function validate(product, endpoint) {
  return product !== null && pages(product.vid).includes(endpoint.usage_page);
}

/**
 * Picks the collection (V3 before V2 when a keyboard shows both; OpenRGB would drive both),
 * sets the name and LED grid, and sends OpenRGB's SendInitialize(). Returns the session state, or
 * null when the product or its report format is unknown (then nothing is sent).
 */
export function start(dev, product) {
  if (!product) {
    return null;
  }
  const page = pickCollection(dev, product.vid);
  const proto = protocolFor(product.vid, page);
  if (!proto) {
    dev.log(`Wooting: no collection with usage page ${pages(product.vid).map((p) => `0x${p.toString(16)}`).join(" or ")}; nothing is sent.`);
    return null;
  }
  const layout = LAYOUTS[wootingType(product.pid)];
  dev.setName(product.name);
  dev.setSize(layout.size.slice());
  dev.setControllableLeds(layout.leds.map((l) => l[0]), layout.leds.map((l) => [l[2], l[3]]));
  sendFeature(dev, COLOR_INIT_COMMAND);
  dev.pause(50);
  return { proto, layout, last: null, sentAt: 0 };
}

/** Unchanged frames go out again after this long (OpenRGB has no keepalive for Wooting). */
const RESEND_MS = 1000;

export function render(dev, st) {
  if (!st) {
    return;
  }
  const colours = st.layout.leds.map((l) => rgb(dev.color(l[2], l[3])));
  const now = Date.now();
  if (st.last !== null && same(colours, st.last) && now - st.sentAt < RESEND_MS) {
    return;
  }
  sendFrame(dev, st, colours);
  st.last = colours;
  st.sentAt = now;
}

// OpenRGB has no exit step. The reset-all colour command hands the keys back to the colours of
// the keyboard's active profile (a volatile command; nothing is stored).
export function shutdown(dev, st) {
  if (!st) {
    return;
  }
  sendFeature(dev, RESET_ALL_COMMAND);
}

// WootingKeyboardController::wooting_usb_send_feature: an 8-byte feature report
// 00 D0 DA <command> <p3> <p2> <p1> <p0> (all parameters 0 here).
function sendFeature(dev, command) {
  dev.send_report([0x00, 0xD0, 0xDA, command, 0x00, 0x00, 0x00, 0x00], 8);
}

// RGBController_WootingKeyboard::DeviceUpdateLEDs: the colours into a 6 x 21 frame buffer at each
// LED's index, then the protocol's SendDirect with all 126 entries.
function sendFrame(dev, st, colours) {
  const fb = [];
  for (let i = 0; i < 126; i++) fb.push([0, 0, 0]);
  st.layout.leds.forEach((l, i) => {
    fb[l[1]] = colours[i];
  });
  if (st.proto === "V1") v1Frame(dev, fb);
  else if (st.proto === "V2") v2Frame(dev, fb);
  else v3Frame(dev, fb);
}

// R5G6B5 of an OpenRGB colour (RGB32ToRGBcolor16): red in the high bits.
function rgb565([r, g, b]) {
  return ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | ((b & 0xF8) >> 3);
}

// WootingV2KeyboardController::SendDirect: one 257-byte output report 00 D0 DA 0B, then the 126
// colours as little-endian R5G6B5.
function v2Frame(dev, fb) {
  const buf = new Array(257).fill(0);
  buf[1] = 0xD0;
  buf[2] = 0xDA;
  buf[3] = RAW_COLORS_REPORT;
  fb.forEach((c, i) => {
    const v = rgb565(c);
    buf[4 + i * 2] = v & 0xFF;
    buf[5 + i * 2] = v >> 8;
  });
  dev.write(buf, 257);
}

// WootingV3KeyboardController::SendDirect: one 2046-byte output report with report id 4,
// 04 D1 DA 0B, then the colours as in V2.
function v3Frame(dev, fb) {
  const buf = new Array(2046).fill(0);
  buf[0] = 4;
  buf[1] = 0xD1;
  buf[2] = 0xDA;
  buf[3] = RAW_COLORS_REPORT;
  fb.forEach((c, i) => {
    const v = rgb565(c);
    buf[4 + i * 2] = v & 0xFF;
    buf[5 + i * 2] = v >> 8;
  });
  dev.write(buf, 2046);
}

// WootingV1KeyboardController: frame-buffer cell -> LED index of the keyboard's PWM controllers
// (null = no LED), and the PWM register of each LED index modulo 24.
const V1_INDEX = [
  [0, null, 11, 12, 23, 24, 36, 47, 85, 84, 49, 48, 59, 61, 73, 81, 80, 113, 114, 115, 116],
  [2, 1, 14, 13, 26, 25, 35, 38, 37, 87, 86, 95, 51, 63, 75, 72, 74, 96, 97, 98, 99],
  [3, 4, 15, 16, 27, 28, 39, 42, 40, 88, 89, 52, 53, 71, 76, 83, 77, 102, 103, 104, 100],
  [5, 6, 17, 18, 29, 30, 41, 46, 44, 90, 93, 54, 57, 65, null, null, null, 105, 106, 107, null],
  [9, 8, 19, 20, 31, 34, 32, 45, 43, 91, 92, 55, null, 66, null, 78, null, 108, 109, 110, 101],
  [10, 22, 21, null, null, null, 33, null, null, null, 94, 58, 67, 68, 70, 79, 82, null, 111, 112, null],
];
const PWM_MEM_MAP = [
  0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
  0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D,
];
// key_code_limit: always WOOTING_TWO_KEY_CODE_LIMIT in OpenRGB's constructor, so part 4 is sent
// for the One too.
const V1_KEY_CODE_LIMIT = 116;
// RGB_PARTS: slave number and register start address of each 96-byte part.
const V1_PARTS = [[0, 0], [0, 96], [1, 0], [1, 96], [2, 0]];

// WootingV1KeyboardController::SendDirect: five 129-byte output reports 00 D0 DA 0B <slave>
// <register>, 96 bytes of PWM registers (red at the LED's register, green + 0x10, blue + 0x20),
// and a CRC-16/CCITT of bytes 0..126, little-endian.
function v1Frame(dev, fb) {
  const parts = V1_PARTS.map(() => new Array(96).fill(0));
  fb.forEach((c, index) => {
    const led = V1_INDEX[Math.floor(index / 21)][index % 21];
    if (led === null || led > V1_KEY_CODE_LIMIT) {
      return;
    }
    const part = led >= 96 ? 4 : led >= 72 ? 3 : led >= 48 ? 2 : led >= 24 ? 1 : 0;
    const reg = PWM_MEM_MAP[led % 24];
    parts[part][reg] = c[0];
    parts[part][reg + 0x10] = c[1];
    parts[part][reg + 0x20] = c[2];
  });
  V1_PARTS.forEach(([slave, register], k) => {
    const rep = new Array(129).fill(0);
    rep[1] = 0xD0;
    rep[2] = 0xDA;
    rep[3] = RAW_COLORS_REPORT;
    rep[4] = slave;
    rep[5] = register;
    for (let i = 0; i < 96; i++) rep[6 + i] = parts[k][i];
    const crc = crc16ccitt(rep, 127);
    rep[127] = crc & 0xFF;
    rep[128] = crc >> 8;
    dev.write(rep, 129);
  });
}

function crc16ccitt(bytes, n) {
  let crc = 0;
  for (let i = 0; i < n; i++) {
    crc ^= bytes[i] << 8;
    for (let b = 0; b < 8; b++) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xFFFF : (crc << 1) & 0xFFFF;
    }
  }
  return crc;
}

/**
 * OpenRGB's detector runs on every collection and drives the ones with a Wooting usage page.
 * HueDeck opens one: make the V3 one active if there is one, else the V2 / V1 one. Returns that
 * collection's usage page (from the host's endpoint list, else from the open collection's report
 * descriptor), or null.
 */
function pickCollection(dev, vid) {
  const wanted = pages(vid);
  if (typeof dev.getHidEndpoints === "function") {
    const endpoints = dev.getHidEndpoints() || [];
    for (const page of wanted) {
      const e = endpoints.find((x) => x.usage_page === page);
      if (e) {
        if (typeof dev.set_endpoint === "function") {
          dev.set_endpoint(e.interface, e.usage, e.usage_page, e.collection);
        }
        return page;
      }
    }
  }
  if (typeof dev.getHidReportFields === "function") {
    const fields = dev.getHidReportFields();
    if (fields && wanted.includes(fields.usagePage)) {
      return fields.usagePage;
    }
  }
  return null;
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
