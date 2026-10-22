// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: NZXT "Hue 2" family lighting in per-LED (Direct) mode: Hue 2 and Hue 2 Ambient,
// RGB & Fan Controllers, RGB Controllers, Smart Device V2, the NZXT USB lighting controllers of
// Hue 2 / B650E motherboards, and the Kraken X3 and Kraken 2024 Elite RGB coolers.
// Adapted from OpenRGB Controllers/NZXTHue2Controller/NZXTHue2Controller.cpp, NZXTHue2Controller.h,
// NZXTHue2ControllerDetect.cpp, RGBController_NZXTHue2.cpp and RGBController_NZXTHue2.h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Adam Honse (calcprogrammer1@gmail.com)      29 Dec 2019
//
// Protocol (64-byte output reports, the first byte is the report id; docs/hardware/nzxt-hue2.md):
//   * Initialize: the two read-only requests OpenRGB's NZXTHue2Controller constructor sends,
//     firmware version (10 01 -> reply 11 01) and attached accessories (20 03 -> reply 21 03).
//   * Every frame, for every OpenRGB zone i in order (RGBController_NZXTHue2::DeviceUpdateLEDs ->
//     NZXTHue2Controller::SetChannelLEDs): 22 10 <1<<i> 00 + up to 20 LEDs as G R B, 22 11 <1<<i> 00
//     + LEDs 21..40 only when the zone has more than 20 LEDs, then the apply report 22 A0 <1<<i> ...
//   * Shutdown: the same reports with every LED black (OpenRGB has no exit step).
// Zones: OpenRGB makes one linear zone per RGB channel. On the Kraken X3 ids zone 0 is the RGB header
// on the pump, zone 1 the pump ring (8 LEDs), zone 2 the logo (1 LED); Lightloom puts the ring (on a
// circle) and the logo (in its centre) on the device's own LED grid and every other zone is an ARGB channel
// "Channel <i+1>" whose LED count comes from the user's components (at most 40, the zone's leds_max).
// Left out: hardware effects (0x28 / 0x2A), fan speed (0x62) and the fan status read; Lightloom never
// changes fan or pump speeds.
// Tested on hardware (2026-10-22): Kraken X3 (1E71:2007), pump ring and logo.

/* global device */

// ---------------------------------------------------------------------------------------------
// Products (NZXTHue2ControllerDetect.cpp): REGISTER_HID_DETECTOR(name, Detect*, 0x1E71, pid) and the
// number of RGB channels each Detect* function hands to the controller (spawn_hue rgb_channels).
// Fan channel counts and the "2023 effects" flag of the Kraken 2024 Elite only matter to fan control
// and hardware effects, which this plugin does not port.
// ---------------------------------------------------------------------------------------------
const NZXT_VID = 0x1E71;
const PRODUCTS = [
  { pid: 0x2009, name: "NZXT RGB & Fan Controller", channels: 2 },          // DetectNZXTFanController
  { pid: 0x2010, name: "NZXT RGB & Fan Controller", channels: 2 },
  { pid: 0x200E, name: "NZXT RGB & Fan Controller", channels: 2 },
  { pid: 0x2011, name: "NZXT RGB & Fan Controller", channels: 6 },          // DetectNZXTFanController6Channel
  { pid: 0x2019, name: "NZXT RGB & Fan Controller", channels: 6 },
  { pid: 0x2020, name: "NZXT RGB & Fan Controller", channels: 6 },
  { pid: 0x201F, name: "NZXT RGB & Fan Controller", channels: 6 },
  { pid: 0x2022, name: "NZXT RGB & Fan Controller 2024", channels: 6 },
  { pid: 0x201B, name: "NZXT B650E Motherboard", channels: 6 },
  { pid: 0x2001, name: "NZXT Hue 2", channels: 4 },                         // DetectNZXTHue2
  { pid: 0x2002, name: "NZXT Hue 2 Ambient", channels: 2 },                 // DetectNZXTHue2Ambient
  { pid: 0x2005, name: "NZXT Hue 2 Motherboard", channels: 2 },             // DetectNZXTHue2Motherboard
  { pid: 0x200B, name: "NZXT Hue 2 Motherboard", channels: 2 },
  { pid: 0x2007, name: "NZXT Kraken X3 Series", channels: 3, krakenX3: true },      // DetectNZXTKrakenX3
  { pid: 0x2014, name: "NZXT Kraken X3 Series RGB", channels: 3, krakenX3: true },
  { pid: 0x3012, name: "NZXT Kraken 2024 ELITE Series RGB", channels: 2 },  // DetectNZXTKrakenElite
  { pid: 0x2012, name: "NZXT RGB Controller", channels: 3 },                // DetectNZXTRGBController
  { pid: 0x2021, name: "NZXT RGB Controller", channels: 3 },
  { pid: 0x2006, name: "NZXT Smart Device V2", channels: 2 },               // DetectNZXTSmartDeviceV2
  { pid: 0x200D, name: "NZXT Smart Device V2", channels: 2 },
  { pid: 0x200F, name: "NZXT Smart Device V2", channels: 2 },
];

// Accessory ids in the device-list reply: LEDs (NZXTHue2Controller::UpdateDeviceList) and name
// (RGBController_NZXTHue2::SetupZones segments). Any other id counts 0 LEDs, as in OpenRGB.
const ACCESSORIES = {
  0x01: [10, "Hue 1 strip"],
  0x02: [8, "Aer 1 fan"],
  0x04: [10, "Hue 2 strip (10 LEDs)"],
  0x05: [8, "Hue 2 strip (8 LEDs)"],
  0x06: [6, "Hue 2 strip (6 LEDs)"],
  0x08: [14, "Hue 2 Cable Comb (14 LEDs)"],
  0x09: [15, "Hue 2 Underglow (300mm) (15 LEDs)"],
  0x0A: [10, "Hue 2 Underglow (200mm) (10 LEDs)"],
  0x0B: [8, "Aer 2 fan (120mm)"],
  0x0C: [8, "Aer 2 fan (140mm)"],
  0x10: [8, "Kraken X3 ring"],
  0x11: [1, "Kraken X3 logo"],
  0x13: [18, "F120 fan (120mm)"],
  0x14: [18, "F140 fan (140mm)"],
  0x15: [20, "F120 Duo fan (120mm)"],
  0x16: [20, "F140 Duo fan (140mm)"],
  0x17: [8, "F120 Core fan (120mm)"],
  0x18: [8, "F140 Core fan (140mm)"],
  0x19: [8, "F120 Core fan case version (120mm)"],
  0x1C: [16, "F280 Core fan case version (280mm)"],
  0x1D: [24, "F360 Core fan case version (360mm)"],
  0x1E: [24, "Kraken Elite Ring"],
  0x1F: [24, "F420 Core fan case version"],
};

// ---------------------------------------------------------------------------------------------
// Metadata. The family has several products, so the static geometry is a placeholder (no LEDs of
// its own, as on every product but the Kraken X3); Initialize() sets the real one.
// ---------------------------------------------------------------------------------------------
export function Name() { return "NZXT Hue 2"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/nzxt-hue2"; }
export function VendorId() { return NZXT_VID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// OpenRGB names no vendor software for this family.
export function ConflictingProcesses() { return []; }

// ---------------------------------------------------------------------------------------------
// Protocol constants (NZXTHue2Controller.cpp / .h)
// ---------------------------------------------------------------------------------------------
const REPORT_LENGTH = 64;                  // unsigned char usb_buf[64]
const ZONE_LED_LIMIT = 40;                 // zone leds_max; SetChannelLEDs' color_data[120]
const LEDS_PER_GROUP = 20;                 // SetChannelLEDs: group 0 = LEDs 1..20, group 1 = the rest

const FIRMWARE_REQUEST = [0x10, 0x01];     // SendFirmwareRequest
const FIRMWARE_REPLY = [0x11, 0x01];       // version at bytes 0x11, 0x12, 0x13
const DEVICE_LIST_REQUEST = [0x20, 0x03];  // UpdateDeviceList
const DEVICE_LIST_REPLY = [0x21, 0x03];
const DEVICE_LIST_FIRST = 0x0F;            // accessory ids of channel c: bytes 0x0F + 6c .. 0x0F + 6c + 5
const DEVICES_PER_CHANNEL = 6;

// Waiting for a reply. OpenRGB reads the device list with a 1000 ms timeout per read
// (NZXT_HUE_2_DEVICE_LIST_READ_TIMEOUT_MS) and gives up after 10 reads
// (NZXT_HUE_2_DEVICE_LIST_MAX_RETRIES), so up to 10 x 1000 ms; its firmware request loops without a
// bound. Lightloom bounds both requests more tightly: at most 10 input reports that are not the reply,
// and at most 1000 ms in total without any input report, polled every 10 ms. When the device did
// not send a single input report to the firmware request, the device-list request is still sent
// (as OpenRGB does) but not waited for: a silent device costs about 1 s, not 2 s.
const REPLY_WAIT_MS = 1000;
const REPLY_MAX_REPORTS = 10;
const POLL_MS = 10;

// An unchanged frame is sent again only after this long (OpenRGB itself sends only on changes).
const KEEPALIVE_MS = 1000;

// Kraken X3 (0x2007, 0x2014): zone 1 "Ring" and zone 2 "Logo" (RGBController_NZXTHue2::SetupZones),
// 8 and 1 LEDs (accessory ids 0x10 and 0x11). OpenRGB's zones are linear; Lightloom places the ring
// LEDs on a circle, clockwise from the top (the layout of the retired nzxt-kraken-x3 plugin), with
// the logo in the centre. The wire order is unchanged: ring LED n is cell n of RING_CELLS.
const RING_CELLS = [[3, 0], [4, 1], [4, 3], [3, 4], [1, 4], [0, 3], [0, 1], [1, 0]];
const LOGO_CELLS = [[2, 2]];
const KRAKEN_X3_SIZE = [5, 5];

// ---------------------------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------------------------
let zones = [];            // OpenRGB zone order: { channel, label } or { cells: [[x, y], ...], label }
let lastFrame = null;      // reports of the last frame sent completely
let lastFrameAt = 0;       // Date.now() when it was sent
let lastRequestHeard = false; // request() saw at least one input report

// OpenRGB registers these ids with REGISTER_HID_DETECTOR, which has no interface, usage page or
// usage (HID_INTERFACE_ANY, HID_USAGE_PAGE_ANY, HID_USAGE_ANY): it runs the detector on every HID
// collection, so on a device with several collections one controller ends up on the right one.
// Lightloom instead binds one instance to the first collection Validate accepts, so this plugin
// accepts only vendor-defined collections (usage page 0xFF00..0xFFFF), where the 22 xx reports go.
// The one known collection (1E71:2007, S5) is of that kind: interface 0, usage page 0xFF00, usage
// 0x01, collection 0. The other products' descriptors are unverified.
export function Validate(endpoint) {
  return endpoint != null && Number(endpoint.usage_page) >= 0xFF00 && Number(endpoint.usage_page) <= 0xFFFF;
}

export function Initialize() {
  zones = [];
  lastFrame = null;
  lastFrameAt = 0;

  const pid = device.productId();
  const product = PRODUCTS.find((p) => p.pid === pid);
  if (product === undefined) {
    device.log(`NZXT Hue 2: product id 0x${hex16(pid)} is not one of this plugin's; nothing is sent.`);
    return;
  }
  device.setName(product.name);

  // NZXTHue2Controller constructor: SendFirmwareRequest(), UpdateDeviceList(). UpdateStatus()
  // reads fan status only (and is disabled in OpenRGB); it is not ported.
  const firmware = request(FIRMWARE_REQUEST, FIRMWARE_REPLY, REPLY_WAIT_MS);
  device.log(firmware === null
    ? "NZXT Hue 2: no firmware version reply."
    : `NZXT Hue 2: firmware ${firmware[0x11]}.${firmware[0x12]}.${firmware[0x13]}.`);
  const listWait = firmware === null && !lastRequestHeard ? 0 : REPLY_WAIT_MS;
  const list = request(DEVICE_LIST_REQUEST, DEVICE_LIST_REPLY, listWait);
  const detected = list === null ? null : accessories(list, product.channels);
  if (detected === null) {
    device.log("NZXT Hue 2: no accessory list reply; channels start with Lightloom's default LED count.");
  }

  if (product.krakenX3) {
    // OpenRGB zone names: "RGB Header", "Ring", "Logo"; LED names "Ring LED n", "Logo LED".
    zones = [
      { channel: "Channel 1", label: "RGB Header (Channel 1)" },
      { cells: RING_CELLS, label: "Ring" },
      { cells: LOGO_CELLS, label: "Logo" },
    ];
    device.setSize(KRAKEN_X3_SIZE);
    device.setControllableLeds(
      [...RING_CELLS.map((_, i) => `Ring LED ${i + 1}`), "Logo LED"],
      [...RING_CELLS, ...LOGO_CELLS],
    );
  } else {
    // OpenRGB zone names: "Hue 2 Channel n".
    for (let i = 0; i < product.channels; i++) {
      zones.push({ channel: `Channel ${i + 1}`, label: `Channel ${i + 1}` });
    }
    device.setSize([1, 1]);
    device.setControllableLeds([], []);
  }

  const channelZones = zones.filter((zone) => zone.channel !== undefined);
  device.SetLedLimit(channelZones.length * ZONE_LED_LIMIT);
  zones.forEach((zone, index) => {
    const found = detected === null ? null : detected[index];
    if (found !== null) {
      const what = found.names.length > 0 ? found.names.join(", ") : "nothing detected";
      device.log(`NZXT Hue 2: ${zone.label}: ${what} (${found.leds} LEDs).`);
    }
    if (zone.channel === undefined) {
      return;
    }
    if (found === null) {
      device.addChannel(zone.channel, ZONE_LED_LIMIT);
    } else {
      // OpenRGB sizes the zone with what the controller detected; Lightloom uses it as the LED count
      // of a channel the user has not set components up for.
      device.addChannel(zone.channel, ZONE_LED_LIMIT, Math.min(found.leds, ZONE_LED_LIMIT));
    }
  });
}

export function Render() {
  if (zones.length === 0) {
    return;
  }
  const frame = frameReports(false);
  const now = Date.now();
  if (lastFrame !== null && sameReports(frame, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
    return;
  }
  if (send(frame)) {
    lastFrame = frame;
    lastFrameAt = now;
  } else {
    lastFrame = null;
  }
}

// OpenRGB has no exit step and keeps no copy of what the device showed before (the protocol has
// hardware effects, but no read-back of the active one), so every zone is set to black with the same
// reports as a frame, on exit and on suspend alike.
export function Shutdown(SystemSuspending) {
  if (zones.length === 0) {
    return;
  }
  send(frameReports(true));
  lastFrame = null;
}

// ---------------------------------------------------------------------------------------------
// Frames (RGBController_NZXTHue2::DeviceUpdateLEDs -> NZXTHue2Controller::SetChannelLEDs)
// ---------------------------------------------------------------------------------------------
function frameReports(black) {
  const reports = [];
  zones.forEach((zone, index) => {
    const grb = zoneColors(zone);
    if (black) {
      grb.fill(0);
    }
    reports.push(...setChannelLeds(index, grb, grb.length / 3));
  });
  return reports;
}

// The zone's colours as OpenRGB's color_data: G, R, B per LED, at most 40 LEDs.
function zoneColors(zone) {
  const out = [];
  if (zone.cells !== undefined) {
    for (const [x, y] of zone.cells) {
      pushGrb(out, device.color(x, y), 0);
    }
    return out;
  }
  const channel = device.channel(zone.channel);
  const rgb = channel ? channel.getColors("Inline", "RGB") : [];
  const count = Math.min(Math.floor((rgb ? rgb.length : 0) / 3), ZONE_LED_LIMIT);
  for (let led = 0; led < count; led++) {
    pushGrb(out, rgb, led * 3);
  }
  return out;
}

function pushGrb(out, rgb, offset) {
  out.push(rgb[offset + 1] & 0xFF, rgb[offset] & 0xFF, rgb[offset + 2] & 0xFF);
}

function setChannelLeds(channel, colorData, numColors) {
  const reports = [];
  const firstColorCount = numColors > LEDS_PER_GROUP ? LEDS_PER_GROUP : numColors;
  reports.push(directReport(channel, 0, colorData, 0, firstColorCount));
  if (numColors > LEDS_PER_GROUP) {
    reports.push(directReport(channel, 1, colorData, LEDS_PER_GROUP, numColors - LEDS_PER_GROUP));
  }
  reports.push(applyReport(channel));
  return reports;
}

// NZXTHue2Controller::SendDirect
function directReport(channel, group, colorData, firstLed, colorCount) {
  const report = new Array(REPORT_LENGTH).fill(0);
  report[0x00] = 0x22;
  report[0x01] = 0x10 | group;
  report[0x02] = (1 << channel) & 0xFF;
  report[0x03] = 0x00;
  for (let i = 0; i < colorCount * 3; i++) {
    report[0x04 + i] = colorData[firstLed * 3 + i];
  }
  return report;
}

// NZXTHue2Controller::SendApply
function applyReport(channel) {
  const report = new Array(REPORT_LENGTH).fill(0);
  report[0x00] = 0x22;
  report[0x01] = 0xA0;
  report[0x02] = (1 << channel) & 0xFF;
  report[0x04] = 0x01;
  report[0x07] = 0x28;
  report[0x0A] = 0x80;
  report[0x0C] = 0x32;
  report[0x0F] = 0x01;
  return report;
}

// ---------------------------------------------------------------------------------------------
// Requests and replies
// ---------------------------------------------------------------------------------------------
// Send the two-byte `command` (zero-padded to 64 bytes) and wait for the input report that starts
// with the two bytes of `reply` and is 64 bytes long, as OpenRGB checks. Returns it, or null.
// The read is non-blocking and polled, so a quiet device costs at most `waitMs` (0: one read only).
function request(command, reply, waitMs) {
  lastRequestHeard = false;
  device.clearReadBuffer();
  const report = new Array(REPORT_LENGTH).fill(0);
  report[0x00] = command[0];
  report[0x01] = command[1];
  if (device.write(report, REPORT_LENGTH) < 0) {
    return null;
  }
  let reports = 0;
  let waited = 0;
  while (reports < REPLY_MAX_REPORTS) {
    const data = device.read([reply[0]], REPORT_LENGTH, 0);
    const size = device.getLastReadSize();
    if (size > 0) {
      reports++;
      lastRequestHeard = true;
      if (size === REPORT_LENGTH && data != null && data.length >= REPORT_LENGTH &&
          (data[0] & 0xFF) === reply[0] && (data[1] & 0xFF) === reply[1]) {
        return Array.from(data, (b) => b & 0xFF);
      }
    } else {
      if (waited >= waitMs) {
        break;
      }
      device.pause(POLL_MS);
      waited += POLL_MS;
    }
  }
  return null;
}

// NZXTHue2Controller::UpdateDeviceList: LEDs and accessory names per RGB channel.
function accessories(list, channels) {
  const out = [];
  for (let chan = 0; chan < channels; chan++) {
    const start = DEVICE_LIST_FIRST + DEVICES_PER_CHANNEL * chan;
    let leds = 0;
    const names = [];
    for (let dev = 0; dev < DEVICES_PER_CHANNEL; dev++) {
      const accessory = ACCESSORIES[list[start + dev]];
      if (accessory !== undefined) {
        leds += accessory[0];
        names.push(accessory[1]);
      }
    }
    out.push({ leds, names });
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------------------------
// Every report of a frame, as OpenRGB does (it ignores hid_write errors); true when all were written.
function send(reports) {
  let ok = true;
  for (const report of reports) {
    if (device.write(report, REPORT_LENGTH) < 0) {
      ok = false;
    }
  }
  return ok;
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

function hex16(value) {
  return (Number(value) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
