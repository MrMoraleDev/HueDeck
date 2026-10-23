// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: ASUS Aura USB lighting controllers of motherboards (the "AURA LED Controller"
// USB device of X570/Z490-and-later boards, the older "Aura Addressable" header controllers), the
// ROG Strix LC III cooler (same protocol) and the ROG AURA Terminal, per LED: the onboard LEDs and
// RGB headers as one row, every addressable header as a Lightloom channel.
// Adapted from OpenRGB Controllers/AsusAuraUSBController/AsusAuraUSBControllerDetect.cpp,
// AsusAuraUSBController/AsusAuraUSBController.cpp and .h, AsusAuraUSBController/
// AsusAuraMainboardController.cpp and .h, AsusAuraUSBController/AsusAuraAddressableController.cpp
// and .h, AsusAuraUSBController/RGBController_AsusAuraUSB.cpp and .h, AsusAuraUSBController/
// RGBController_AsusAuraMainboard.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Martin Hartl (inlart)  25 Apr 2020" (AsusAuraUSBController,
// AsusAuraMainboardController); "Adam Honse (CalcProgrammer1)  18 Jan 2020"
// (AsusAuraAddressableController, RGBController_AsusAuraUSB); "rytypete  30 Aug 2023"
// (RGBController_AsusAuraMainboard); AsusAuraUSBControllerDetect.cpp names no author ("This file
// is part of the OpenRGB project").
//
// Protocol notes: docs/hardware/asus-aura-usb.md. In short: 65-byte output reports with report id
// 0xEC. Start: firmware request (EC 82) and configuration table request (EC B0), both read back;
// mainboards then get EC 52 53 00 01. Per zone with LEDs, once: the Direct effect (mainboards
// EC 35 <effect channel> 00 00 FF, addressable controllers EC 3B <channel> 00 FF). Per frame:
// EC 40 <channel | 0x80 on the last report> <offset> <count> R G B ..., 20 LEDs per report.
// Never sent: the commit / save command (EC 3F 55), the "shutdown effect" variants (byte 4 = 01)
// and every hardware effect.

/* global device */

const VENDOR_ID = 0x0B05;
const MAINBOARD = "mainboard";
const ADDRESSABLE = "addressable";

// AsusAuraUSBControllerDetect.cpp, in registration order. `validate`: the usage page and usage of
// REGISTER_HID_DETECTOR_PU; null for REGISTER_HID_DETECTOR (any collection, see Validate).
// OpenRGB names the mainboard and addressable controllers "ASUS " + the DMI board name (+
// " Addressable"); Lightloom has no DMI name, so it uses the detector's name.
const PRODUCTS = [
  { pid: 0x1867, name: "ASUS Aura Addressable", kind: ADDRESSABLE, validate: null },
  { pid: 0x1872, name: "ASUS Aura Addressable", kind: ADDRESSABLE, validate: null },
  { pid: 0x18A3, name: "ASUS Aura Addressable", kind: ADDRESSABLE, validate: null },
  { pid: 0x18A5, name: "ASUS Aura Addressable", kind: ADDRESSABLE, validate: null },
  { pid: 0x1B29, name: "ASUS ROG Strix LC III", kind: MAINBOARD, validate: null },
  { pid: 0x18F3, name: "ASUS Aura Motherboard", kind: MAINBOARD, validate: null },
  { pid: 0x1939, name: "ASUS Aura Motherboard", kind: MAINBOARD, validate: null },
  { pid: 0x19AF, name: "ASUS Aura Motherboard", kind: MAINBOARD, validate: { page: 0xFF72, usage: 0x00A1 } },
  { pid: 0x1AA6, name: "ASUS Aura Motherboard", kind: MAINBOARD, validate: null },
  { pid: 0x1BED, name: "ASUS Aura Motherboard", kind: MAINBOARD, validate: null },
  { pid: 0x1889, name: "ASUS ROG AURA Terminal", kind: ADDRESSABLE, validate: null },
];

// ---------------------------------------------------------------------------------------------
// Protocol constants (AsusAuraUSBController.h, AsusAuraMainboardController.h,
// AsusAuraAddressableController.h, RGBController_AsusAuraUSB.h)
// ---------------------------------------------------------------------------------------------
const REPORT_LENGTH = 65;                       // unsigned char usb_buf[65]
const REPORT_ID = 0xEC;
const REQUEST_FIRMWARE_VERSION = 0x82;          // reply byte 1 = 0x02, version at bytes 2..17
const REQUEST_CONFIG_TABLE = 0xB0;              // reply byte 1 = 0x30, table at bytes 4..63
const FIRMWARE_REPLY = 0x02;
const CONFIG_REPLY = 0x30;
const CONTROL_MODE_DIRECT = 0x40;
const DIRECT_CMD_APPLY = 0x80;
const DIRECT_CMD_8BIT_OVERFLOW = 0x10;
const LEDS_PER_PACKET = 0x14;
const MAINBOARD_CONTROL_MODE_EFFECT = 0x35;
const ADDRESSABLE_CONTROL_MODE_EFFECT = 0x3B;
const MODE_DIRECT = 0xFF;
const MAINBOARD_DIRECT_CHANNEL = 0x04;          // direct_channel of the fixed (mainboard) device
const ADDRESSABLE_MAX_LEDS = 500;               // AURA_ADDRESSABLE_MAX_LEDS

// Config table offsets (config_table = reply bytes 4..63).
const CFG_ADDRESSABLE_HEADERS = 0x02;
const CFG_MAINBOARD_LEDS = 0x1B;
const CFG_RGB_HEADERS = 0x1D;

// OpenRGB's hid_read() blocks until a report arrives. Lightloom polls instead: at most 10 input
// reports that are not the reply, at most 1000 ms without any report, polled every 10 ms.
const REPLY_WAIT_MS = 1000;
const REPLY_MAX_REPORTS = 10;
const POLL_MS = 10;

// An unchanged frame is sent again only after this long (OpenRGB sends only on changes).
const KEEPALIVE_MS = 1000;

export function Name() { return "ASUS Aura USB"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/asus-aura-usb"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "motherboard"; }
// Placeholders; Initialize() sets the real geometry from the configuration table.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// ASUS's own lighting software holds the controller while it runs; Lightloom reports it, never
// closes it.
export function ConflictingProcesses() {
  return ["ArmouryCrate.exe", "ArmouryCrate.Service.exe", "LightingService.exe"];
}

// ---------------------------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------------------------
let zones = [];            // OpenRGB zone order: { fixed, effectChannel, directChannel, leds | channel }
let product = null;
let lastFrame = null;
let lastFrameAt = 0;

// REGISTER_HID_DETECTOR (no interface, usage page or usage) runs OpenRGB's detector on every HID
// collection of the device. Lightloom binds one instance to the first collection Validate accepts,
// so for those ids it takes vendor-defined collections only (usage page 0xFF00..0xFFFF), where the
// EC reports go. 0x19AF is registered with REGISTER_HID_DETECTOR_PU(0xFF72, 0x00A1): exactly that.
export function Validate(endpoint) {
  if (endpoint == null) {
    return false;
  }
  const page = Number(endpoint.usage_page);
  const known = PRODUCTS.find((p) => p.pid === device.productId());
  if (known !== undefined && known.validate !== null) {
    return page === known.validate.page && Number(endpoint.usage) === known.validate.usage;
  }
  return page >= 0xFF00 && page <= 0xFFFF;
}

export function Initialize() {
  zones = [];
  lastFrame = null;
  lastFrameAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (product === null) {
    device.log(`ASUS Aura USB: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);

  // AuraUSBController constructor: GetFirmwareVersion(), GetConfigTable().
  const firmware = request(REQUEST_FIRMWARE_VERSION, FIRMWARE_REPLY);
  if (firmware !== null) {
    device.log(`ASUS Aura USB: firmware ${ascii(firmware.slice(2, 18))}.`);
  }
  const reply = request(REQUEST_CONFIG_TABLE, CONFIG_REPLY);
  if (reply === null) {
    // OpenRGB: "Could not read config table, can not add device".
    device.log("ASUS Aura USB: could not read the configuration table; nothing is sent.");
    device.setSize([1, 1]);
    device.setControllableLeds([], []);
    return;
  }
  const config = reply.slice(4, 64);
  zones = product.kind === MAINBOARD ? mainboardZones(config) : addressableZones(config);

  const fixed = zones.find((z) => z.fixed);
  if (fixed !== undefined) {
    device.setSize([fixed.ledNames.length, 1]);
    device.setControllableLeds(fixed.ledNames.slice(), fixed.ledNames.map((_, i) => [i, 0]));
  } else {
    device.setSize([1, 1]);
    device.setControllableLeds([], []);
  }
  const headers = zones.filter((z) => !z.fixed);
  device.SetLedLimit(headers.length * ADDRESSABLE_MAX_LEDS);
  for (const zone of headers) {
    device.addChannel(zone.channel, ADDRESSABLE_MAX_LEDS);
  }
  device.log(`ASUS Aura USB: ${fixed ? fixed.ledNames.length : 0} onboard LEDs, ${headers.length} addressable headers.`);

  if (product.kind === MAINBOARD) {
    // AuraMainboardController::SetGen1(), sent by OpenRGB's constructor (not explained there).
    writeReport([REPORT_ID, 0x52, 0x53, 0x00, 0x01]);
  }
}

export function Render() {
  if (zones.length === 0) {
    return;
  }
  const colours = zones.map((zone) => zoneColours(zone));
  // RGBController_AuraUSB::DeviceUpdateMode, run by OpenRGB before its first update: the Direct
  // effect for every zone with LEDs. A Lightloom channel can get LEDs later, so a zone gets it the
  // first time it has LEDs.
  const reports = [];
  zones.forEach((zone, i) => {
    if (!zone.modeSent && colours[i].length > 0) {
      reports.push(effectReport(zone));
    }
  });
  const frame = [];
  zones.forEach((zone, i) => frame.push(...sendDirect(zone.directChannel, colours[i])));
  const now = Date.now();
  if (reports.length === 0 && lastFrame !== null && sameReports(frame, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
    return;
  }
  let ok = true;
  zones.forEach((zone, i) => {
    if (!zone.modeSent && colours[i].length > 0) {
      zone.modeSent = writeReport(effectReport(zone));
      ok = zone.modeSent && ok;
    }
  });
  for (const report of frame) {
    ok = writeReport(report) && ok;
  }
  if (ok) {
    lastFrame = frame;
    lastFrameAt = now;
  } else {
    lastFrame = null;
  }
}

// OpenRGB has no exit step. The protocol has hardware effects, but no read-back of the effect the
// board had before (that one lives in the board's saved settings, which only the commit command
// writes and Lightloom never sends), so every zone Lightloom switched to Direct is set black, on exit
// and on suspend alike. After a power cycle the board shows its saved effect again.
export function Shutdown(SystemSuspending) {
  for (const zone of zones) {
    if (!zone.modeSent) {
      continue;
    }
    const black = zoneColours(zone).map(() => [0, 0, 0]);
    for (const report of sendDirect(zone.directChannel, black)) {
      writeReport(report);
    }
  }
  lastFrame = null;
}

// ---------------------------------------------------------------------------------------------
// Zones
// ---------------------------------------------------------------------------------------------
// AuraMainboardController constructor + RGBController_AuraUSB::SetupZones.
function mainboardZones(config) {
  const total = config[CFG_MAINBOARD_LEDS];
  let rgbHeaders = config[CFG_RGB_HEADERS];
  const addressable = config[CFG_ADDRESSABLE_HEADERS];
  if (total < rgbHeaders) {
    rgbHeaders = 0;
  }
  const out = [];
  let effectChannel = 0;
  if (total > 0) {
    const onboard = total - rgbHeaders;
    const ledNames = [];
    for (let i = 0; i < total; i++) {
      ledNames.push(i >= onboard ? `Aura Mainboard, RGB Header ${i + 1 - onboard}` : `Aura Mainboard, LED ${i + 1}`);
    }
    out.push({ fixed: true, effectChannel, directChannel: MAINBOARD_DIRECT_CHANNEL, ledNames, modeSent: false });
    effectChannel++;
  }
  for (let i = 0; i < addressable; i++) {
    out.push({
      fixed: false, effectChannel, directChannel: i, channel: `Addressable RGB Header ${i + 1}`, modeSent: false,
    });
    effectChannel++;
  }
  return out;
}

// AuraAddressableController constructor: one addressable device per header, effect channel =
// zone index (SendEffect is called with the zone index).
function addressableZones(config) {
  const out = [];
  for (let i = 0; i < config[CFG_ADDRESSABLE_HEADERS]; i++) {
    out.push({
      fixed: false, effectChannel: i, directChannel: i, channel: `Addressable RGB Header ${i + 1}`, modeSent: false,
    });
  }
  return out;
}

function zoneColours(zone) {
  if (zone.fixed) {
    return zone.ledNames.map((_, i) => device.color(i, 0));
  }
  const channel = device.channel(zone.channel);
  const rgb = channel ? channel.getColors("Inline", "RGB") : [];
  const count = Math.min(Math.floor((rgb ? rgb.length : 0) / 3), ADDRESSABLE_MAX_LEDS);
  const out = [];
  for (let i = 0; i < count; i++) {
    out.push([rgb[i * 3] & 0xFF, rgb[i * 3 + 1] & 0xFF, rgb[i * 3 + 2] & 0xFF]);
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------------------------
// AuraMainboardController::SendEffect(effect_channel, AURA_MODE_DIRECT, shutdown_effect = false)
// or AuraAddressableController::SendEffect(channel, AURA_MODE_DIRECT, 0, 0, 0).
function effectReport(zone) {
  const buf = new Array(REPORT_LENGTH).fill(0);
  buf[0x00] = REPORT_ID;
  if (product.kind === MAINBOARD) {
    buf[0x01] = MAINBOARD_CONTROL_MODE_EFFECT;
    buf[0x02] = zone.effectChannel & 0xFF;
    buf[0x03] = 0x00;
    buf[0x04] = 0x00;
    buf[0x05] = MODE_DIRECT;
  } else {
    buf[0x01] = ADDRESSABLE_CONTROL_MODE_EFFECT;
    buf[0x02] = zone.effectChannel & 0xFF;
    buf[0x03] = 0x00;
    buf[0x04] = MODE_DIRECT;
  }
  return buf;
}

// AuraUSBController::SendDirect, byte for byte (unsigned char offset and command, as in C++).
function sendDirect(deviceChannel, colours) {
  const reports = [];
  const ledCount = colours.length;
  let command = deviceChannel & 0xFF;
  let ledsOffset = 0x00;
  let ledsInPacket = LEDS_PER_PACKET;
  let ledsConfigured = 0;
  while (ledsConfigured < ledCount) {
    if (ledsConfigured + ledsInPacket < ledCount) {
      ledsConfigured += ledsInPacket;
    } else {
      command |= DIRECT_CMD_APPLY;
      ledsInPacket = ledCount - ledsConfigured;
      ledsConfigured = ledCount;
    }
    const buf = new Array(REPORT_LENGTH).fill(0);
    buf[0x00] = REPORT_ID;
    buf[0x01] = CONTROL_MODE_DIRECT;
    buf[0x02] = command & 0xFF;
    buf[0x03] = ledsOffset & 0xFF;
    buf[0x04] = ledsInPacket & 0xFF;
    for (let led = 0; led < ledsInPacket; led++) {
      const c = colours[ledsConfigured - ledsInPacket + led];
      buf[0x05 + led * 3] = c[0] & 0xFF;
      buf[0x06 + led * 3] = c[1] & 0xFF;
      buf[0x07 + led * 3] = c[2] & 0xFF;
    }
    reports.push(buf);
    ledsOffset = (ledsOffset + ledsInPacket) & 0xFF;
    if (ledsConfigured > 255) {
      command |= DIRECT_CMD_8BIT_OVERFLOW;
      ledsOffset = (ledsConfigured - 256) & 0xFF;
    }
  }
  return reports;
}

// ---------------------------------------------------------------------------------------------
// I/O
// ---------------------------------------------------------------------------------------------
// Write EC <command> (zero-padded to 65 bytes) and wait for the input report whose byte 1 is
// `replyId`. Returns it (65 bytes, report id first) or null.
function request(command, replyId) {
  device.clearReadBuffer();
  if (!writeReport([REPORT_ID, command])) {
    return null;
  }
  let reports = 0;
  let waited = 0;
  while (reports < REPLY_MAX_REPORTS) {
    const data = device.read([REPORT_ID], REPORT_LENGTH, 0);
    const size = device.getLastReadSize();
    if (size > 0) {
      reports++;
      if (data != null && data.length >= REPORT_LENGTH && (data[1] & 0xFF) === replyId) {
        return Array.from(data.slice(0, REPORT_LENGTH), (b) => b & 0xFF);
      }
    } else {
      if (waited >= REPLY_WAIT_MS) {
        break;
      }
      device.pause(POLL_MS);
      waited += POLL_MS;
    }
  }
  return null;
}

function writeReport(bytes) {
  const buf = new Array(REPORT_LENGTH).fill(0);
  for (let i = 0; i < bytes.length && i < REPORT_LENGTH; i++) {
    buf[i] = bytes[i] & 0xFF;
  }
  return device.write(buf, REPORT_LENGTH) !== -1;
}

function sameReports(a, b) {
  if (a.length !== b.length) {
    return false;
  }
  for (let r = 0; r < a.length; r++) {
    for (let i = 0; i < REPORT_LENGTH; i++) {
      if (a[r][i] !== b[r][i]) {
        return false;
      }
    }
  }
  return true;
}

function ascii(bytes) {
  let s = "";
  for (const b of bytes) {
    if (b === 0) {
      break;
    }
    s += b >= 0x20 && b < 0x7F ? String.fromCharCode(b) : "?";
  }
  return s;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
