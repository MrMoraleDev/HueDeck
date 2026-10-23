// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: per-key lighting of the Logitech G813 / G815 (wired) and G915 / G915 TKL
// (wired and through their LIGHTSPEED receivers) keyboards, over HID++ 2.0 per-key frames.
// Adapted from OpenRGB Controllers/LogitechController/LogitechControllerDetect.cpp,
// LogitechG815Controller/LogitechG815Controller.cpp and .h,
// LogitechG815Controller/RGBController_LogitechG815.cpp, LogitechG915Controller/
// LogitechG915Controller.cpp and .h, LogitechG915Controller/RGBController_LogitechG915.cpp
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Cheerpipe  20 Mar 2021" (LogitechG815Controller, RGBController_LogitechG815,
// LogitechG915Controller, RGBController_LogitechG915); LogitechControllerDetect.cpp names no
// author ("This file is part of the OpenRGB project"). The key tables in layouts.js are generated
// from OpenRGB's tables.
//
// Protocol notes: docs/hardware/logitech-keyboard.md. In short: 20-byte HID++ long reports
// (report id 0x11) on the vendor collection; per frame only the keys whose colour changed, as
// "big" frames (function 0x6F: one colour, up to 13 key codes) and "little" frames (0x1F: up to
// four key/colour pairs), then a commit (0x7F); each report is followed by a read of the reply.
// Left out on purpose (see the notes): OpenRGB's direct-mode initialisation writes to feature
// 0x4522 (key disabling, not lighting) and its two 0x8071 cluster-effect writes, which carry the
// byte OpenRGB itself labels "Persistence" set to 1; and OpenRGB's hardware effects (SetMode),
// which carry that byte too. The G213 is held back: its only colour command is the 0x8070 zone
// effect that OpenRGB also uses for its (automatically saved) hardware modes, sent without first
// turning saving off.

/* global device */

import { G815_LAYOUT, G915_LAYOUT, G915_TKL_LAYOUT } from "./layouts.js";

const VENDOR_ID = 0x046D;

// LogitechControllerDetect.cpp, in registration order.
const PRODUCTS = [
  { pid: 0xC232, name: "Logitech G813 RGB Mechanical Gaming Keyboard", family: "G815" },
  { pid: 0xC33F, name: "Logitech G815 RGB Mechanical Gaming Keyboard", family: "G815" },
  { pid: 0xC541, name: "Logitech G915 Wireless RGB Mechanical Gaming Keyboard", family: "G915", wired: false, tkl: false },
  { pid: 0xC547, name: "Logitech G915 Wireless RGB Mechanical Gaming Keyboard (Receiver 2)", family: "G915", wired: false, probe: true },
  { pid: 0xC33E, name: "Logitech G915 Wireless RGB Mechanical Gaming Keyboard (Wired)", family: "G915", wired: true, tkl: false },
  { pid: 0xC545, name: "Logitech G915TKL Wireless RGB Mechanical Gaming Keyboard", family: "G915", wired: false, tkl: true },
  { pid: 0xC343, name: "Logitech G915TKL Wireless RGB Mechanical Gaming Keyboard (Wired)", family: "G915", wired: true, tkl: true },
];

// REGISTER_HID_DETECTOR_IP(..., 1, 0xFF43) for the G815 family; OpenRGB opens the usage 0x0602
// (20-byte report) collection and sends everything there. REGISTER_HID_DETECTOR_IPU(..., 2,
// 0xFF00, 2) for the G915 family.
const ENDPOINTS = {
  G815: { interface: 1, usage_page: 0xFF43, usage: 0x0602 },
  G915: { interface: 2, usage_page: 0xFF00, usage: 0x0002 },
};

const LONG_REPORT = 0x11;
const MESSAGE_LEN = 20;
const PROBE_LEN = 64;
// LOGITECH_READ_TIMEOUT. OpenRGB also uses it for the replies it waits for without a timeout
// (hid_read); HueDeck never waits without one.
const READ_TIMEOUT_MS = 300;
const FRAME_LITTLE = 0x1F;
const FRAME_BIG = 0x6F;
const COMMIT = 0x7F;
const DATA_FRAME_SIZE = 16;
const BIG_FRAME_MAX_KEYS = 13;
const LITTLE_FRAME_MAX_KEYS = 4;
const ESCAPE_CODE = 0x29;

// Feature indices LogitechG915Controller hard-codes (wired / through the receiver).
const G915_WIRED = { deviceIndex: 0xFF, feature8081: 0x0A };
const G915_WIRELESS = { deviceIndex: 0x01, feature8081: 0x0B };
// The G815 family: device index 0xFF, per-key feature at index 0x10.
const G815_ADDRESS = { deviceIndex: 0xFF, feature8081: 0x10 };

export function Name() { return "Logitech G815 / G915"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/logitech-keyboard"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
// Placeholders (the G815's); Initialize() sets the product's own.
export function Size() { return [G815_LAYOUT.width, G815_LAYOUT.height]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return G815_LAYOUT.leds.map((l) => l[0]); }
export function LedPositions() { return G815_LAYOUT.leds.map((l) => [l[1], l[2]]); }
export function ImageUrl() { return ""; }
// Logitech G HUB and Logitech Gaming Software drive the lights while they run; HueDeck reports
// them, never closes them.
export function ConflictingProcesses() { return ["lghub.exe", "lghub_agent.exe", "lghub_updater.exe", "LCore.exe"]; }

let product = null;
let layout = null;
let address = null;
let current = null;

export function Validate(endpoint) {
  const p = PRODUCTS.find((q) => q.pid === device.productId());
  const want = p ? ENDPOINTS[p.family] : null;
  return Boolean(want)
    && endpoint.interface === want.interface
    && endpoint.usage_page === want.usage_page
    && endpoint.usage === want.usage;
}

export function Initialize() {
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  layout = null;
  current = null;
  if (!product) {
    device.log(`Logitech keyboard: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  let tkl = product.tkl;
  if (product.probe) {
    // DetectLogitechKeyboardG915Receiver2: 0xC547 is shared by several keyboards; a HID++ name
    // probe picks the controller. "G915 X" belongs to OpenRGB's unified HID++ 2.0 controller,
    // which HueDeck does not port; anything else that is not a G915 is left alone too.
    const name = probeReceiverName();
    if (name === null) {
      device.log("Logitech G915 receiver (0xC547): the name probe got no answer; nothing is driven.");
      return;
    }
    if (name.includes("G915 X")) {
      device.log(`Logitech receiver 0xC547 reports "${name}" (G915 X): not driven by this plugin.`);
      return;
    }
    if (name.includes("G915 TKL")) {
      tkl = true;
    } else if (name.includes("G915")) {
      tkl = false;
    } else {
      device.log(`Logitech receiver 0xC547 reports "${name}": not a G915, nothing is driven.`);
      return;
    }
  }
  if (product.family === "G815") {
    layout = G815_LAYOUT;
    address = G815_ADDRESS;
  } else {
    layout = tkl ? G915_TKL_LAYOUT : G915_LAYOUT;
    address = product.wired ? G915_WIRED : G915_WIRELESS;
  }
  device.setName(product.name);
  device.setSize([layout.width, layout.height]);
  device.setControllableLeds(layout.leds.map((l) => l[0]), layout.leds.map((l) => [l[1], l[2]]));
  // SetupColors(): every LED starts black, so DeviceUpdateLEDs() skips keys that stay black.
  current = layout.leds.map(() => 0);
  // DeviceUpdateMode() for Direct, without InitializeDirect() (see the header): "Set one key to
  // get direct mode engaged" (Escape, black), then commit.
  sendFrame(FRAME_LITTLE, [ESCAPE_CODE, 0, 0, 0]);
  commit();
}

export function Render() {
  if (!layout) {
    return;
  }
  updateLeds(layout.leds.map((l) => device.color(l[1], l[2])));
}

// OpenRGB has no exit step and no command that hands these keyboards back to their onboard
// effect without the persistent cluster-effect write HueDeck leaves out: every key goes black.
export function Shutdown(SystemSuspending) {
  if (!layout) {
    return;
  }
  updateLeds(layout.leds.map(() => [0, 0, 0]));
}

// RGBController_LogitechG815/G915::DeviceUpdateLEDs: the keys whose colour changed, grouped by
// colour in ascending RGBColor order (std::map<RGBColor, ...>, RGBColor = 0x00BBGGRR).
function updateLeds(colours) {
  const groups = new Map();
  colours.forEach(([r, g, b], i) => {
    const value = ((b & 0xFF) << 16) | ((g & 0xFF) << 8) | (r & 0xFF);
    if (value === current[i]) {
      return;
    }
    if (!groups.has(value)) {
      groups.set(value, []);
    }
    groups.get(value).push(layout.leds[i][3]);
  });
  if (groups.size === 0) {
    return;
  }
  const ordered = [...groups.entries()].sort((a, b) => a[0] - b[0]);
  if (product.family === "G815") {
    sendG815Frames(ordered);
  } else {
    sendG915Frames(ordered);
  }
  commit();
  colours.forEach(([r, g, b], i) => {
    current[i] = ((b & 0xFF) << 16) | ((g & 0xFF) << 8) | (r & 0xFF);
  });
}

// RGBController_LogitechG815::DeviceUpdateLEDs. A colour with more than four keys goes out in
// big frames of up to 13 keys; the others fill little frames of four pairs, carried over from
// one colour to the next. HueDeck deviation (docs): a last little frame keeps its last blue byte
// and is zero after its 0xFF end byte (OpenRGB's memset there starts one byte early and leaves
// byte 15 as it was).
function sendG815Frames(ordered) {
  let little = [];
  for (const [value, codes] of ordered) {
    const rgb = [value & 0xFF, (value >> 8) & 0xFF, (value >> 16) & 0xFF];
    if (codes.length > LITTLE_FRAME_MAX_KEYS) {
      for (let bi = 0; bi < codes.length; bi += BIG_FRAME_MAX_KEYS) {
        sendFrame(FRAME_BIG, bigFrame(rgb, codes.slice(bi, bi + BIG_FRAME_MAX_KEYS)));
      }
    } else {
      for (const code of codes) {
        little.push(code, ...rgb);
        if (little.length === DATA_FRAME_SIZE) {
          sendFrame(FRAME_LITTLE, little);
          little = [];
        }
      }
    }
  }
  if (little.length > 0) {
    sendFrame(FRAME_LITTLE, terminated(little));
  }
}

// RGBController_LogitechG915::DeviceUpdateLEDs: per colour, chunks of 13 keys; a chunk of more
// than four keys is a big frame, a smaller one goes into the little frames. All big frames go
// first, then the little ones.
function sendG915Frames(ordered) {
  const bigs = [];
  const littles = [];
  let little = [];
  for (const [value, codes] of ordered) {
    const rgb = [value & 0xFF, (value >> 8) & 0xFF, (value >> 16) & 0xFF];
    for (let bi = 0; bi < codes.length; bi += BIG_FRAME_MAX_KEYS) {
      const chunk = codes.slice(bi, bi + BIG_FRAME_MAX_KEYS);
      if (chunk.length > LITTLE_FRAME_MAX_KEYS) {
        bigs.push(bigFrame(rgb, chunk));
      } else {
        for (const code of chunk) {
          little.push(code, ...rgb);
          if (little.length === DATA_FRAME_SIZE) {
            littles.push(little);
            little = [];
          }
        }
      }
    }
  }
  if (little.length > 0) {
    littles.push(terminated(little));
  }
  for (const frame of bigs) {
    sendFrame(FRAME_BIG, frame);
  }
  for (const frame of littles) {
    sendFrame(FRAME_LITTLE, frame);
  }
}

// [R, G, B, key 0 .. key 12], then 0xFF and zeros when there are fewer than 13 keys.
function bigFrame(rgb, codes) {
  return codes.length < BIG_FRAME_MAX_KEYS ? terminated([...rgb, ...codes]) : [...rgb, ...codes];
}

function terminated(data) {
  const out = data.concat([0xFF]);
  while (out.length < DATA_FRAME_SIZE) {
    out.push(0);
  }
  return out;
}

// SendDirectFrame / SendSingleLed: 11 <device> <8081 index> <function> <16 data bytes>.
function sendFrame(fn, data) {
  const pkt = new Array(MESSAGE_LEN).fill(0);
  pkt[0] = LONG_REPORT;
  pkt[1] = address.deviceIndex;
  pkt[2] = address.feature8081;
  pkt[3] = fn;
  for (let i = 0; i < data.length && i < DATA_FRAME_SIZE; i++) {
    pkt[4 + i] = data[i] & 0xFF;
  }
  device.write(pkt, MESSAGE_LEN);
  device.read([LONG_REPORT], MESSAGE_LEN, READ_TIMEOUT_MS);
}

// SendCommit: 11 <device> <8081 index> 7F.
function commit() {
  sendFrame(COMMIT, []);
}

// ProbeG915ReceiverName: HID++ short requests 10 01 03 0E (name length) and 10 01 03 1E (name);
// the answer 11 01 03 1E carries the name from byte 4. Read-only.
function probeReceiverName() {
  device.write([0x10, 0x01, 0x03, 0x0E, 0x00, 0x00, 0x00], 7);
  device.read([LONG_REPORT], PROBE_LEN, 100);
  device.write([0x10, 0x01, 0x03, 0x1E, 0x00, 0x00, 0x00], 7);
  for (let attempt = 0; attempt < 3; attempt++) {
    const resp = device.read([LONG_REPORT], PROBE_LEN, 200);
    const rd = Math.min(device.getLastReadSize(), resp.length);
    if (rd < 8) {
      continue;
    }
    if (resp[0] === 0x11 && resp[1] === 0x01 && resp[2] === 0x03 && resp[3] === 0x1E) {
      let name = "";
      for (let i = 4; i < rd && resp[i] !== 0x00; i++) {
        name += String.fromCharCode(resp[i]);
      }
      return name;
    }
  }
  return null;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
