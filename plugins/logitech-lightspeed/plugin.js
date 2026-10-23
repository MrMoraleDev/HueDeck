// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: Logitech G mice and headsets that OpenRGB drives as "Lightspeed" devices over a
// wired USB connection: their HID++ 2.0 RGB-effects feature (0x8070) is found at run time, and each
// lighting zone gets a static colour. G502 (Proteus Spectrum, HERO, LIGHTSPEED), G403 (Prodigy,
// LIGHTSPEED), G703 / G703 HERO, G900, G903 / G903 HERO, G Pro Wireless (all on their cable), and
// the G633, G635, G733 and G935 headsets.
// Adapted from OpenRGB Controllers/LogitechController/LogitechControllerDetect.cpp,
// LogitechProtocolCommon.cpp and .h, LogitechLightspeedController/LogitechLightspeedController.cpp
// and .h, LogitechLightspeedController/RGBController_LogitechLightspeed.cpp (GPL-2.0-or-later) at
// commit d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Chris M (Dr_No)  04 May 2021" (LogitechProtocolCommon); "TheRogueZeta
// 05 Aug 2020" (LogitechLightspeedController, RGBController_LogitechLightspeed);
// LogitechControllerDetect.cpp names no author ("This file is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/logitech-lightspeed.md. In short: 20-byte HID++ long reports
// (report id 0x11, device index 0xFF) on the vendor collection, each answered by the device.
// Start: read-only queries (feature indices of 0x0005 device name and 0x8070 / 0x8071 RGB
// effects, the name and type, the zones and their effect lists), then 0x8070 function 0x80 with
// 01 01, OpenRGB's software control, which it pairs with "disable save to flash". Each frame:
// 0x8070 function 0x30 (set zone effect) with the "on" effect, R G B and persistence byte 0x00, per
// zone. Held back at run time: devices that only have the newer 0x8071 feature (OpenRGB sends
// their colours with the persistence byte set to 1). Never sent: hardware effects, onboard
// profiles, DPI, report rate, power or lighting timeouts.

/* global device */

const VENDOR_ID = 0x046D;

// LogitechControllerDetect.cpp, the DetectLogitechWired registrations, in order. "iface" null:
// REGISTER_HID_DETECTOR_PU (any interface).
const PRODUCTS = [
  { pid: 0xC08D, name: "Logitech G502 Wireless Gaming Mouse (wired)", iface: null, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC332, name: "Logitech G502 Proteus Spectrum Gaming Mouse", iface: 1, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC08B, name: "Logitech G502 HERO Gaming Mouse", iface: 1, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC083, name: "Logitech G403 Prodigy Gaming Mouse", iface: 1, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC082, name: "Logitech G403 Wireless Gaming Mouse (wired)", iface: 1, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC087, name: "Logitech G703 Wireless Gaming Mouse (wired)", iface: 1, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC090, name: "Logitech G703 HERO Wireless Gaming Mouse (wired)", iface: null, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC081, name: "Logitech G900 Wireless Gaming Mouse (wired)", iface: 1, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC086, name: "Logitech G903 Wireless Gaming Mouse (wired)", iface: 1, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC091, name: "Logitech G903 HERO Wireless Gaming Mouse (wired)", iface: 2, page: 0xFF00, usage: 0x0002 },
  { pid: 0xC088, name: "Logitech G Pro Wireless Gaming Mouse (wired)", iface: 2, page: 0xFF00, usage: 0x0002 },
  { pid: 0x0A5C, name: "Logitech G633 Gaming Headset", iface: 3, page: 0xFF43, usage: 0x0202 },
  { pid: 0x0A89, name: "Logitech G635 Gaming Headset", iface: 3, page: 0xFF43, usage: 0x0202 },
  { pid: 0x0AB5, name: "Logitech G733 Gaming Headset", iface: 3, page: 0xFF43, usage: 0x0202 },
  { pid: 0x0AFE, name: "Logitech G733 Gaming Headset", iface: 3, page: 0xFF43, usage: 0x0202 },
  { pid: 0x0B1F, name: "Logitech G733 Gaming Headset", iface: 3, page: 0xFF43, usage: 0x0202 },
  { pid: 0x0A87, name: "Logitech G935 Gaming Headset", iface: 3, page: 0xFF43, usage: 0x0202 },
];

const LONG_REPORT = 0x11;
const LONG_LEN = 20;
const RESPONSE_LEN = 64;               // LOGITECH_FAP_RESPONSE_LEN
const DEVICE_INDEX = 0xFF;             // LOGITECH_DEFAULT_DEVICE_INDEX (wired)
const TIMEOUT_MS = 300;                // LOGITECH_PROTOCOL_TIMEOUT
const MAX_RETRY = 10;                  // LOGITECH_LIGHTSPEED_DETECT_MAX_RETRY
const RETRY_PAUSE_MS = 50;
const PAGE_DEVICE_NAME = 0x0005;
const PAGE_RGB_EFFECTS1 = 0x8070;
const PAGE_RGB_EFFECTS2 = 0x8071;
const FN_ROOT_GET_FEATURE = 0x01;
const FN_NAME_GET_COUNT = 0x01;
const FN_NAME_GET_NAME = 0x11;
const FN_NAME_GET_TYPE = 0x21;
const FN_RGB_GET_COUNT = 0x00;
const FN_RGB_GET_INFO = 0x10;
const FN_RGB_GET_CONTROL = 0x20;
const FN_8070_SET_EFFECT = 0x30;
const FN_8070_SET_SW_CTL = 0x80;
const LED_ON = 0x0001;                 // LOGITECH_DEVICE_LED_ON: the effect Direct uses
const NAME_GUARD = 32;                 // HueDeck bound on the name loop (see the notes)
// logitech_led_locations[]: zone names by LED location.
const LOCATIONS = ["Unknown", "Primary", "Logo", "Left", "Right", "Combined",
  "Group One", "Group Two", "Group Three", "Group Four", "Group Five", "Group Six"];
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

export function Name() { return "Logitech Lightspeed (wired)"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/logitech-lightspeed"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
// Placeholders; Initialize() sets the zones the device reports.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Primary LED"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// Logitech G HUB and Logitech Gaming Software drive the lights while they run; HueDeck reports
// them, never closes them.
export function ConflictingProcesses() { return ["lghub.exe", "lghub_agent.exe", "lghub_updater.exe", "LCore.exe"]; }

let rgbIndex = 0;
let directEffect = -1;
let zones = null;
let lastColours = null;
let lastSentAt = 0;

export function Validate(endpoint) {
  const p = PRODUCTS.find((q) => q.pid === device.productId());
  return Boolean(p)
    && (p.iface === null || endpoint.interface === p.iface)
    && endpoint.usage_page === p.page
    && endpoint.usage === p.usage;
}

export function Initialize() {
  zones = null;
  lastColours = null;
  lastSentAt = 0;
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  if (!product) {
    device.log(`Logitech Lightspeed: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  // CreateLogitechLightspeedDevice: up to ten attempts, 50 ms apart, each a fresh logitech_device.
  // As in OpenRGB, a device that only becomes valid on the tenth attempt is still given up.
  let info = null;
  let retry = 0;
  while (retry < MAX_RETRY) {
    device.pause(RETRY_PAUSE_MS);
    const attempt = initialiseDevice();
    retry++;
    if (attempt.heldBack) {
      device.log(`${attempt.name || product.name}: only the HID++ 0x8071 RGB feature, held back (see docs/hardware/logitech-lightspeed.md).`);
      return;
    }
    if (isValid(attempt)) {
      info = attempt;
      break;
    }
  }
  if (!info || retry >= MAX_RETRY) {
    device.log(`${product.name}: the device did not answer the HID++ queries; nothing is driven.`);
    return;
  }
  // RGBController_LogitechLightspeed: "Direct" is the first effect of LED 0 that is LED_ON.
  const led0 = info.leds.get(0);
  directEffect = led0 ? led0.fx.findIndex((mode) => mode === LED_ON) : -1;
  if (directEffect < 0) {
    device.log(`${info.name}: no static ("on") effect in its effect list; nothing is driven.`);
    return;
  }
  rgbIndex = info.rgbIndex;
  zones = [];
  for (let i = 0; i < info.leds.size; i++) {
    const led = info.leds.get(i);
    const location = led ? led.location : 0;
    zones.push(location < LOCATIONS.length ? `${LOCATIONS[location]} LED` : `LED ${i}`);
  }
  device.setName(info.name);
  device.setSize([zones.length, 1]);
  device.setControllableLeds(zones.slice(), zones.map((_, i) => [i, 0]));
  // DeviceUpdateMode() for Direct: setDirectMode(true), RGB feature function 0x80 with 01 01.
  request([rgbIndex, FN_8070_SET_SW_CTL, 0x01, 0x01]);
}

export function Render() {
  if (!zones) {
    return;
  }
  const colours = zones.map((_, i) => device.color(i, 0));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (updateLeds(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step. Leaving software control (function 0x80 with 00 00) is what OpenRGB
// pairs with saving to flash again, so it is not sent; the zones go black in software control.
export function Shutdown(SystemSuspending) {
  if (!zones) {
    return;
  }
  updateLeds(zones.map(() => [0, 0, 0]));
}

// DeviceUpdateLEDs -> logitech_device::setMode(direct effect, speed 0, zone, R, G, B,
// brightness 0) for every zone: with speed and brightness 0 no effect parameter is set.
function updateLeds(colours) {
  let ok = true;
  colours.forEach(([r, g, b], zone) => {
    ok = request([rgbIndex, FN_8070_SET_EFFECT, zone, directEffect, r & 0xFF, g & 0xFF, b & 0xFF]) && ok;
  });
  return ok;
}

// logitech_device::initialiseDevice() for a wired device (always connected).
function initialiseDevice() {
  const info = { name: "", type: 0, rgbIndex: 0, leds: new Map(), heldBack: false };
  flushReadQueue();
  getDeviceName(info);
  // OpenRGB dumps the whole feature list here only at debug log level; not ported.
  let page = 0;
  for (const candidate of [PAGE_RGB_EFFECTS1, PAGE_RGB_EFFECTS2]) {
    const index = getFeatureIndex(candidate);
    if (index > 0) {
      info.rgbIndex = index;
      page = candidate;
      break;
    }
  }
  if (page === PAGE_RGB_EFFECTS2) {
    info.heldBack = true;
  } else if (page === PAGE_RGB_EFFECTS1) {
    getRgbConfig8070(info);
  }
  return info;
}

// logitech_device::is_valid().
function isValid(info) {
  const c = info.name.length > 0 ? info.name.charCodeAt(0) : 0;
  return info.name.length > 3 && info.type <= 8 && c >= 32 && c < 122 && info.leds.size > 0 && info.rgbIndex > 0;
}

// logitech_device::flushReadQueue(): read until nothing comes for 300 ms.
function flushReadQueue() {
  for (;;) {
    device.read([LONG_REPORT], RESPONSE_LEN, TIMEOUT_MS);
    if (device.getLastReadSize() <= 0) {
      return;
    }
  }
}

// logitech_device::getFeatureIndex(page): root feature (index 0) function 0x01.
function getFeatureIndex(page) {
  const resp = request([0x00, FN_ROOT_GET_FEATURE, page >> 8, page & 0xFF]);
  return resp.data[0];
}

// logitech_device::getDeviceName(): name length, the name in chunks, the device type.
function getDeviceName(info) {
  const index = getFeatureIndex(PAGE_DEVICE_NAME);
  if (index <= 0) {
    return;
  }
  const length = request([index, FN_NAME_GET_COUNT]).data[0];
  let guard = 0;
  while (info.name.length < length && guard++ < NAME_GUARD) {
    const resp = request([index, FN_NAME_GET_NAME, info.name.length]);
    let chunk = "";
    for (let i = 0; i < resp.data.length && resp.data[i] !== 0; i++) {
      chunk += String.fromCharCode(resp.data[i]);
    }
    if (chunk.length === 0) {
      break;   // HueDeck: OpenRGB would ask again forever
    }
    info.name += chunk;
  }
  info.type = request([index, FN_NAME_GET_TYPE]).data[0];
}

// logitech_device::getRGBconfig() for feature 0x8070.
function getRgbConfig8070(info) {
  const idx = info.rgbIndex;
  device.write(longRequest([idx, FN_RGB_GET_COUNT]), LONG_LEN);
  let resp;
  do {
    resp = readResponse();
  } while (resp.size === LONG_LEN && resp.featureIndex !== idx && resp.command !== FN_RGB_GET_COUNT);
  const count = resp.data[0];
  for (let i = 0; i < count; i++) {
    const zone = request([idx, FN_RGB_GET_INFO, i]);
    if (zone.size === LONG_LEN && zone.featureIndex === idx && zone.command === FN_RGB_GET_INFO
      && zone.data[0] !== 0x10 && zone.data[1] !== 0x02) {
      const led = { location: (zone.data[1] << 8) | zone.data[2], fx: [] };
      const fxCount = zone.data[3];
      for (let j = 0; j < fxCount; j++) {
        const fx = request([idx, FN_RGB_GET_CONTROL, i, j]);
        led.fx.push((fx.data[2] << 8) | fx.data[3]);
      }
      if (!info.leds.has(zone.data[0])) {
        info.leds.set(zone.data[0], led);
      }
    }
  }
}

// One long request (11 FF <feature> <function> <params>), then one read of the answer.
function request(fields) {
  device.write(longRequest(fields), LONG_LEN);
  return readResponse();
}

function longRequest(fields) {
  const pkt = new Array(LONG_LEN).fill(0);
  pkt[0] = LONG_REPORT;
  pkt[1] = DEVICE_INDEX;
  fields.forEach((v, i) => { pkt[2 + i] = v & 0xFF; });
  return pkt;
}

// hid_read_timeout(dev, response, 64, 300); a missing answer reads as zeros.
function readResponse() {
  const bytes = device.read([LONG_REPORT], RESPONSE_LEN, TIMEOUT_MS) || [];
  const size = Math.min(device.getLastReadSize(), bytes.length);
  const at = (i) => (i < size ? bytes[i] & 0xFF : 0);
  const data = [];
  for (let i = 4; i < RESPONSE_LEN; i++) {
    data.push(at(i));
  }
  return { size, featureIndex: at(2), command: at(3), data };
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
