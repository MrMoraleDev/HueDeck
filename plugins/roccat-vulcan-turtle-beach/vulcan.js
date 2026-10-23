// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin module: the Roccat / Turtle Beach Vulcan keyboard protocol (Vulcan 100 / 120 Aimo,
// Vulcan TKL, Vulcan Pro, Vulcan TKL Pro, Vulcan II / II Max / II Mini, Pyro, Magma, Magma Mini,
// Turtle Beach Vulcan II / II TKL / II TKL Pro), per-key colours in direct mode. Shared, byte for
// byte the same file, by plugins/roccat-vulcan/ (vendor 1E7D) and
// plugins/roccat-vulcan-turtle-beach/ (vendor 10F5).
// Adapted from OpenRGB Controllers/RoccatController/RoccatVulcanKeyboardController/
// RoccatVulcanKeyboardController.cpp, RoccatVulcanKeyboardController.h,
// RGBController_RoccatVulcanKeyboard.cpp and Controllers/RoccatController/RoccatControllerDetect.cpp
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Mola19 (17 Dec 2021, RoccatVulcanKeyboardController and
// RGBController_RoccatVulcanKeyboard); RoccatControllerDetect.cpp names no author ("This file is
// part of the OpenRGB project").
//
// Protocol notes: docs/hardware/roccat-vulcan.md. In short: two HID collections. The control
// collection (interface 1) takes feature reports: the info read (0x0F or 0x09: firmware version,
// layout), the direct-mode switch (15 00 <on> or 0E 05 <on> 00 00) and the ready poll (0x04). The
// LED collection (interface 3, or 4) takes the colour frame as 65-byte output reports (A1 <n>
// header), after which OpenRGB drains and waits for the keyboard's answer.
//
// Never sent: OpenRGB's SendMode (feature report 0x0D or 0x11, the lighting mode the keyboard keeps
// in its profile, which OpenRGB also sends on the way into Direct). HueDeck only switches direct
// mode on and off; see the hardware notes.

import { LAYOUTS } from "./layouts.js";

// Product ids (RoccatVulcanKeyboardController.h).
export const PID = Object.freeze({
  VULCAN_100_AIMO: 0x307A,
  VULCAN_120_AIMO: 0x3098,
  VULCAN_TKL: 0x2FEE,
  VULCAN_PRO: 0x30F7,
  VULCAN_TKL_PRO: 0x311A,
  VULCAN_II: 0x2F4E,
  VULCAN_II_MINI: 0x2F09,
  VULCAN_II_MAX: 0x2EE2,
  PYRO: 0x314C,
  MAGMA: 0x3124,
  MAGMA_MINI: 0x69A0,
  TB_VULCAN_II: 0x501B,
  TB_VULCAN_II_TKL: 0x5023,
  TB_VULCAN_II_TKL_PRO: 0x5001,
});

export const ROCCAT_VID = 0x1E7D;
export const TURTLE_BEACH_VID = 0x10F5;

// RoccatControllerDetect.cpp, in registration order: name, vendor id, product id.
export const PRODUCTS = Object.freeze([
  { name: "Roccat Magma", vid: ROCCAT_VID, pid: PID.MAGMA },
  { name: "Roccat Magma Mini", vid: ROCCAT_VID, pid: PID.MAGMA_MINI },
  { name: "Roccat Pyro", vid: ROCCAT_VID, pid: PID.PYRO },
  { name: "Roccat Vulcan 100 Aimo", vid: ROCCAT_VID, pid: PID.VULCAN_100_AIMO },
  { name: "Roccat Vulcan 120-Series Aimo", vid: ROCCAT_VID, pid: PID.VULCAN_120_AIMO },
  { name: "Roccat Vulcan TKL", vid: ROCCAT_VID, pid: PID.VULCAN_TKL },
  { name: "Roccat Vulcan Pro", vid: ROCCAT_VID, pid: PID.VULCAN_PRO },
  { name: "Roccat Vulcan TKL Pro", vid: ROCCAT_VID, pid: PID.VULCAN_TKL_PRO },
  { name: "Roccat Vulcan II", vid: ROCCAT_VID, pid: PID.VULCAN_II },
  { name: "Roccat Vulcan II Max", vid: ROCCAT_VID, pid: PID.VULCAN_II_MAX },
  { name: "Roccat Vulcan II Mini", vid: ROCCAT_VID, pid: PID.VULCAN_II_MINI },
  { name: "Turtle Beach Vulcan II", vid: TURTLE_BEACH_VID, pid: PID.TB_VULCAN_II },
  { name: "Turtle Beach Vulcan II TKL", vid: TURTLE_BEACH_VID, pid: PID.TB_VULCAN_II_TKL },
  { name: "Turtle Beach Vulcan II TKL Pro", vid: TURTLE_BEACH_VID, pid: PID.TB_VULCAN_II_TKL_PRO },
]);

// Products of the newer firmware family: the case lists of InitDeviceInfo and EnableDirect.
const NEW_FAMILY = new Set([
  PID.PYRO, PID.MAGMA, PID.MAGMA_MINI, PID.VULCAN_PRO, PID.VULCAN_TKL_PRO, PID.VULCAN_II,
  PID.VULCAN_II_MINI, PID.VULCAN_II_MAX, PID.TB_VULCAN_II, PID.TB_VULCAN_II_TKL_PRO,
]);
// WaitUntilReady: a 4-byte report 0x04 for these, 3 bytes for the others.
const READY_4 = new Set([PID.PYRO, PID.MAGMA, PID.MAGMA_MINI, PID.VULCAN_PRO, PID.VULCAN_TKL_PRO]);

// SendColors: [packet_length, column_length, protocol_version] per product; default 436, 12, 1.
const SEND_COLORS = new Map([
  [PID.MAGMA, [64, 5, 2]],
  [PID.MAGMA_MINI, [64, 5, 2]],
  [PID.PYRO, [378, 1, 2]],
  [PID.VULCAN_PRO, [384, 12, 2]],
  [PID.VULCAN_TKL_PRO, [384, 12, 2]],
  [PID.VULCAN_II, [396, 1, 2]],
  [PID.TB_VULCAN_II, [396, 1, 2]],
  [PID.VULCAN_II_MAX, [567, 1, 2]],
  [PID.VULCAN_II_MINI, [345, 1, 2]],
  [PID.TB_VULCAN_II_TKL_PRO, [320, 1, 2]],
]);
const SEND_COLORS_DEFAULT = [436, 12, 1];

// RGBController_RoccatVulcanKeyboard::SetupZones: the layout table per product.
const LAYOUT_TABLE = new Map([
  [PID.VULCAN_100_AIMO, "RoccatVulcan120AimoLayouts"],
  [PID.VULCAN_120_AIMO, "RoccatVulcan120AimoLayouts"],
  [PID.VULCAN_TKL, "RoccatVulcanTKLLayouts"],
  [PID.VULCAN_TKL_PRO, "RoccatVulcanTKLLayouts"],
  [PID.TB_VULCAN_II_TKL, "RoccatVulcanTKLLayouts"],
  [PID.TB_VULCAN_II_TKL_PRO, "TurtleBeachVulcanIITKLProLayouts"],
  [PID.VULCAN_PRO, "RoccatPyroLayouts"],
  [PID.PYRO, "RoccatPyroLayouts"],
  [PID.VULCAN_II, "RoccatVulcanIILayouts"],
  [PID.TB_VULCAN_II, "RoccatVulcanIILayouts"],
  [PID.VULCAN_II_MAX, "RoccatVulcanIIMaxLayouts"],
  [PID.VULCAN_II_MINI, "RoccatVulcanIIMiniLayouts"],
  [PID.MAGMA, "RoccatMagmaLayouts"],
  [PID.MAGMA_MINI, "RoccatMagmaLayouts"],
]);

// roccat_vulcan_ii_mini_slot_map (RoccatVulcanKeyboardController.cpp).
const MINI_LED_COUNT = 69;
const MINI_SLOT_MAP = Object.freeze([
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
  18, 19, 20, 21, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33,
  34, 35, 36, 37, 38, 39, 40, 42, 43, 45, 46, 48, 49, 50, 51,
  52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 64, 65, 66, 68,
  70, 71, 72, 73, 74, 75, 76,
]);

// ROCCAT_VULCAN_LAYOUT_* values the info report gives in byte 6.
const LAYOUT_DE = 1;
const LAYOUT_UK = 2;
const LAYOUT_FR = 3;
const LAYOUT_JP = 6;

/** Unchanged frames are sent again after this long. */
const KEEPALIVE_MS = 1000;
/** ClearResponses() reads until nothing is left; HueDeck stops after this many reads. */
const MAX_DRAIN = 64;

/** The collections DetectRoccatVulcanKeyboardControllers opens: control and LED. */
export function endpointsFor(pid) {
  switch (pid) {
    case PID.VULCAN_PRO:
    case PID.VULCAN_TKL_PRO:
    case PID.PYRO:
    case PID.MAGMA:
    case PID.MAGMA_MINI:
    case PID.VULCAN_II:
    case PID.VULCAN_II_MINI:
    case PID.VULCAN_II_MAX:
    case PID.TB_VULCAN_II:
      return { ctrl: { iface: 1, page: 0xFF01 }, led: { iface: 3, page: 0xFF00 } };
    case PID.TB_VULCAN_II_TKL_PRO:
      return { ctrl: { iface: 1, page: 0x0001 }, led: { iface: 4, page: 0xFF00 } };
    default:
      return { ctrl: { iface: 1, page: 0x000B }, led: { iface: 3, page: 0x0001 } };
  }
}

function matches(spec, e) {
  return e.interface === spec.iface && e.usage_page === spec.page;
}

/** Validate(): the control and the LED collection of the product (matched as OpenRGB does: interface and usage page). */
export function validate(pid, endpoint) {
  const eps = endpointsFor(pid);
  return matches(eps.ctrl, endpoint) || matches(eps.led, endpoint);
}

/** The layout entry for a product and the layout byte the keyboard reported. */
export function pickLayout(pid, layoutType) {
  const table = LAYOUTS[LAYOUT_TABLE.get(pid) ?? "RoccatVulcan120AimoLayouts"];
  let name;
  switch (layoutType) {
    case LAYOUT_DE:
    case LAYOUT_UK:
    case LAYOUT_FR:
      name = "UK";
      break;
    case LAYOUT_JP:
      name = "JP";
      break;
    default:
      name = "US";
  }
  // OpenRGB indexes its std::map with operator[], which makes an empty layout (no LEDs at all)
  // when the table has none for the reported layout. HueDeck uses US instead, else the table's
  // only layout.
  if (!table[name]) {
    name = table.US ? "US" : Object.keys(table)[0];
  }
  return { name, layout: table[name] };
}

/**
 * RoccatVulcanKeyboardController::SendColors: the output reports (65 bytes, report id 0x00
 * first) for colours = [{ value, rgb }], in LED order.
 */
export function colourPackets(pid, colours, bigEndian) {
  const [packetLength, columnLength, protocolVersion] = SEND_COLORS.get(pid) ?? SEND_COLORS_DEFAULT;
  const packetNum = Math.ceil(packetLength / 64);
  const bufs = Array.from({ length: packetNum }, () => new Array(65).fill(0));
  if (protocolVersion > 1) {
    for (let i = 0; i < packetNum; i++) {
      bufs[i][1] = 0xA1;
      bufs[i][2] = (i + 1) & 0xFF;
    }
  } else {
    bufs[0][1] = 0xA1;
    bufs[0][2] = 0x01;
  }
  const header = packetLength > 255 ? 4 : 3;
  if (header === 3) {
    bufs[0][3] = packetLength & 0xFF;
  } else if (bigEndian) {
    bufs[0][3] = (packetLength >> 8) & 0xFF;
    bufs[0][4] = packetLength & 0xFF;
  } else {
    bufs[0][3] = packetLength & 0xFF;
    bufs[0][4] = (packetLength >> 8) & 0xFF;
  }
  const dataLength = 64 - header;
  // Bounds checks: the C++ checks them only for the TKL Pro; every table HueDeck ships stays
  // inside the buffers (a test checks it), so they never drop a byte the C++ would send.
  const put = (packet, index, value) => {
    if (packet < bufs.length && index < 65) {
      bufs[packet][index] = value & 0xFF;
    }
  };

  if (pid === PID.TB_VULCAN_II_TKL_PRO) {
    let hwLeds = 0;
    for (const c of colours) {
      if (c.value > hwLeds) hwLeds = c.value;
    }
    hwLeds += 1;
    for (const c of colours) {
      if (c.value >= hwLeds) continue;
      c.rgb.forEach((component, k) => {
        const pos = c.value + k * hwLeds;
        put(Math.floor(pos / dataLength), (pos % dataLength) + header + 1, component);
      });
    }
    return bufs;
  }

  for (const c of colours) {
    let ledIndex = c.value;
    if (pid === PID.VULCAN_II_MINI) {
      if (ledIndex >= MINI_LED_COUNT) continue;
      ledIndex = MINI_SLOT_MAP[ledIndex];
    }
    const column = Math.floor(ledIndex / columnLength);
    const row = ledIndex % columnLength;
    if (protocolVersion === 1) {
      let offset = column * 3 * columnLength + row + header;
      for (const component of c.rgb) {
        put(Math.floor(offset / 64), (offset % 64) + 1, component);
        offset += columnLength;
      }
    } else {
      let offset = column * 3 * columnLength + row;
      for (const component of c.rgb) {
        put(Math.floor(offset / dataLength), (offset % dataLength) + header + 1, component);
        offset += columnLength;
      }
    }
  }
  return bufs;
}

/** A keyboard session on the global `device`, for the products of one vendor id. */
export function createVulcan(vendorId) {
  const products = PRODUCTS.filter((p) => p.vid === vendorId);
  let product = null;
  let leds = null; // [{ name, value, cell | null }] in OpenRGB order
  let bigEndian = false;
  let endpoints = null; // { ctrl, led } endpoint tuples
  let lastColours = null;
  let lastSentAt = 0;

  function log(text) {
    device.log(`Roccat Vulcan: ${text}`);
  }

  function useEndpoint(e) {
    device.set_endpoint(e.interface, e.usage, e.usage_page, e.collection);
  }

  // The collections OpenRGB opens. OpenRGB takes the first enumerated match of each; when the LED
  // match is ambiguous, HueDeck prefers the one whose report descriptor declares the 65-byte
  // output report it writes.
  function findEndpoints(pid) {
    if (typeof device.getHidEndpoints !== "function" || typeof device.set_endpoint !== "function") {
      return null;
    }
    const all = device.getHidEndpoints() || [];
    const spec = endpointsFor(pid);
    const ctrl = all.find((e) => matches(spec.ctrl, e));
    const leds = all.filter((e) => matches(spec.led, e));
    if (!ctrl || leds.length === 0) {
      return null;
    }
    let led = leds[0];
    if (leds.length > 1 && typeof device.getHidReportFields === "function") {
      for (const e of leds) {
        useEndpoint(e);
        const fields = device.getHidReportFields();
        const reports = fields && Array.isArray(fields.reports) ? fields.reports : [];
        if (reports.some((r) => r.kind === "output" && r.bytes >= 65)) {
          led = e;
          break;
        }
      }
    }
    return { ctrl, led };
  }

  // RoccatVulcanKeyboardController::InitDeviceInfo: { major, minor, layoutType }.
  function readDeviceInfo(pid) {
    const [reportId, length] = NEW_FAMILY.has(pid) ? [0x09, 9] : [0x0F, 8];
    const first = device.get_report([reportId], length);
    const res = device.getLastReadSize();
    const second = device.get_report([reportId], length);
    // The C++ reads twice into the same buffer and keeps the first call's length.
    const buf = Array.isArray(second) && second.length > 0 ? second : (Array.isArray(first) ? first : []);
    const version = (buf[2] ?? 0) & 0xFF;
    let layoutType = 0;
    if (pid !== PID.MAGMA && pid !== PID.MAGMA_MINI && res >= 7) {
      layoutType = (buf[6] ?? 0) & 0xFF;
    }
    return { major: Math.floor(version / 100), minor: version % 100, layoutType };
  }

  // RoccatVulcanKeyboardController::EnableDirect.
  function enableDirect(on) {
    const flag = on ? 0x01 : 0x00;
    if (NEW_FAMILY.has(product.pid)) {
      device.send_report([0x0E, 0x05, flag, 0x00, 0x00], 5);
    } else {
      device.send_report([0x15, 0x00, flag], 3);
    }
  }

  // RoccatVulcanKeyboardController::WaitUntilReady: poll report 0x04 until byte 1 is 1 (at most
  // 100 reads, 25 ms apart).
  function waitUntilReady() {
    const length = READY_4.has(product.pid) ? 4 : 3;
    let ready = false;
    for (let i = 0; !ready && i < 100; i++) {
      if (i !== 0) device.pause(25);
      const buf = device.get_report([0x04], length);
      ready = Array.isArray(buf) && buf[1] === 1;
    }
    if (!ready) log("the keyboard did not report ready");
  }

  function sendColours(colours) {
    const list = leds.map((led, i) => ({ value: led.value, rgb: colours[i] }));
    let ok = true;
    for (const buf of colourPackets(product.pid, list, bigEndian)) {
      ok = device.write(buf, 65) !== -1 && ok;
    }
    // ClearResponses(), then AwaitResponse(20).
    for (let i = 0; i < MAX_DRAIN; i++) {
      device.read([0x00], 65, 0);
      if (!(device.getLastReadSize() > 0)) break;
    }
    device.read([0x00], 65, 20);
    return ok;
  }

  function initialize() {
    product = null;
    leds = null;
    lastColours = null;
    lastSentAt = 0;
    const pid = device.productId();
    const found = products.find((p) => p.pid === pid);
    if (!found) {
      log(`product id 0x${pid.toString(16).toUpperCase()} is not one this plugin knows.`);
      return;
    }
    endpoints = findEndpoints(pid);
    if (!endpoints) {
      log("the control and LED collections were not both found; nothing is sent.");
      return;
    }
    product = found;
    useEndpoint(endpoints.ctrl);
    const info = readDeviceInfo(pid);
    // IsBigEndianDirectMode(): never on the Vulcan II Mini; else major >= 1 && minor >= 16
    // (the C++ test, kept as is: 2.05 counts as little-endian).
    bigEndian = pid !== PID.VULCAN_II_MINI && info.major >= 1 && info.minor >= 16;
    const { name, layout } = pickLayout(pid, info.layoutType);
    leds = layout.leds.map(([ledName, value, x, y]) => ({
      name: ledName, value, cell: x === null ? null : [x, y],
    }));
    const placed = leds.filter((l) => l.cell !== null);
    device.setName(`${product.name} (${name})`);
    device.setSize([layout.width, layout.height]);
    device.setControllableLeds(placed.map((l) => l.name), placed.map((l) => l.cell.slice()));
    // OpenRGB's switch to Direct (DeviceUpdateMode) is SendMode(Direct), WaitUntilReady,
    // EnableDirect(true), WaitUntilReady. SendMode is left out (see the header).
    enableDirect(true);
    waitUntilReady();
    useEndpoint(endpoints.led);
  }

  function render() {
    if (!product) return;
    const colours = leds.map((l) => (l.cell === null ? [0, 0, 0] : device.color(l.cell[0], l.cell[1])));
    const now = Date.now();
    if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
      return;
    }
    if (sendColours(colours)) {
      lastColours = colours;
      lastSentAt = now;
    }
  }

  // OpenRGB has no exit step. EnableDirect(false) is what OpenRGB sends when it hands the keyboard
  // to a hardware mode: the keyboard shows the lighting of its stored profile again.
  function shutdown() {
    if (!product) return;
    useEndpoint(endpoints.ctrl);
    enableDirect(false);
    waitUntilReady();
  }

  return { initialize, render, shutdown, products };
}

function sameColours(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) return false;
  }
  return true;
}
