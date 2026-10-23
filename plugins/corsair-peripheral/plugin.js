// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the lights of the older Corsair peripherals OpenRGB drives with its "Corsair
// Peripheral" controllers: the K55 / K65 / K68 / K70 / K95 / Strafe RGB keyboards, the Glaive,
// Harpoon, Ironclaw, M65, Nightsword, Scimitar and Sabre mice, the MM800 RGB Polaris mousemat and
// the ST100 RGB headset stand (the 0x07 / 0x7F "CUE" protocol), plus the K55 RGB PRO XT and the
// K65 Mini (the 0x08 protocol of the newer Corsair keyboards), per LED.
// Adapted from OpenRGB Controllers/CorsairPeripheralController/CorsairPeripheralControllerDetect.cpp,
// CorsairPeripheralController.cpp and .h, RGBController_CorsairPeripheral.cpp and .h,
// CorsairK55RGBPROXTController.cpp and .h, RGBController_CorsairK55RGBPROXT.cpp and .h,
// CorsairK65MiniController.cpp and .h, RGBController_CorsairK65Mini.cpp and .h (GPL-2.0-or-later)
// at commit d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  09 Jan 2020" (CorsairPeripheralController,
// RGBController_CorsairPeripheral); the detector, K55 RGB PRO XT and K65 Mini files name no author
// ("This file is part of the OpenRGB project"). The tables in devices.js are generated from
// OpenRGB's; the protocol below is written by hand.
//
// Protocol notes: docs/hardware/corsair-peripheral.md. In short, all output reports with report
// id 0x00:
//   CUE (65 bytes): a firmware-info request (0E 01) tells keyboard / mouse / mousepad; then
//     "software lighting" (07 05 02), on some keyboards also 07 04 02 and the 07 40 key packets;
//     per frame, keyboards stream three 168-byte colour planes (7F 01/02/03) each followed by a
//     submit (07 28), the K55 sends three zones (07 25), mice and mousepads one 07 22 report.
//   K55 RGB PRO XT (65 bytes): 08 01 / 08 02 / 08 0D at start, seven reports per frame (08 06,
//     08 07 x 6). K65 Mini (1025 bytes): three start reports, one 08 06 report per frame.
// Never sent: OpenRGB's hardware-effect path (SetHardwareMode: the "lght_00.d" file write, its
// 7F stream and its 07 17 09 / 07 17 08 commit, then 07 04 01, on the CUE keyboards; the
// 08 0D 01 61 6D / 08 09 01 / 08 06 01 78 / 08 07 01 / 08 05 01 01 buffer write and the 00 01 03
// report on the K55 RGB PRO XT): it stores an effect in the device. No DPI, polling rate, macro
// or profile command exists in this plugin.

/* global device */

import {
  VENDOR_ID, PRODUCTS, LOGICAL_LAYOUT, KEYBOARD_LAYOUTS, KEY_MAPPING_K95_PLAT_ANSI,
  KEY_MAPPING_K95_PLAT_ISO, KEY_MAPPING_K70_MK2_PLAT_ISO, ST100_SLOTS, MOUSE_LED_NAMES, K55_PRO_XT,
  K65_MINI,
} from "./devices.js";

const PACKET_LENGTH = 65;             // CORSAIR_PERIPHERAL_PACKET_LENGTH, HID_PACKET_LENGTH
const K65_PACKET_LENGTH = 1025;       // CorsairK65MiniController.h PACKET_LENGTH
const COMMAND_WRITE = 0x07;
const COMMAND_READ = 0x0E;
const COMMAND_STREAM = 0x7F;
const PROPERTY_FIRMWARE_INFO = 0x01;
const PROPERTY_SPECIAL_FUNCTION = 0x04;
const PROPERTY_LIGHTING_CONTROL = 0x05;
const PROPERTY_SUBMIT_MOUSE_COLOR = 0x22;
const PROPERTY_SUBMIT_KBZONES_COLOR_24 = 0x25;
const PROPERTY_SUBMIT_KEYBOARD_COLOR_24 = 0x28;
const LIGHTING_CONTROL_SOFTWARE = 0x02;
const LAYOUT_ISO = 0x01;
const FW_INFO_MAX_ATTEMPTS = 10;      // CORSAIR_FW_INFO_MAX_ATTEMPTS
const FW_INFO_TIMEOUT_MS = 1000;
const ST100_PID = 0x0A34;
const USAGE_PAGE_CUE = 0xFFC2;
const USAGE_PAGE_K55_PRO_XT = 0xFF42;
const K55_PRO_XT_BANK = 137;          // COLOR_BANK_SIZE
// Render() resends unchanged colours after this long. The K55 RGB PRO XT and the K65 Mini go
// back to their own effect when nothing comes for a minute (OpenRGB's keepalive: 50 s).
const KEEPALIVE_MS = 1000;
// Keyboards whose start needs 07 04 02 and the 07 40 key packets (OpenRGB's constructor).
const EXTRA_SETUP = new Set(["K55", "K95_PLAT", "K70_MK2", "K68"]);

const PRODUCT_IDS = PRODUCTS.map((p) => p[0]);

export function Name() { return "Corsair Peripheral"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/corsair-peripheral"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCT_IDS.slice(); }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
// Placeholders; Initialize() sets the product's own geometry.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["LED 1"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// Corsair's own software (iCUE, and the older CUE) drives these devices while it runs; HueDeck
// reports it, never closes it.
export function ConflictingProcesses() { return ["iCUE.exe", "Corsair.Service.exe", "CUE.exe"]; }

let product = null;   // { pid, name, protocol, rule }
let kind = null;      // "keyboard" | "mouse" | "mousemat" | "stand" | "k55xt" | "k65mini"
let layout = null;    // keyboard layout name (CUE keyboards)
let physical = 0;     // CORSAIR_LAYOUT_* from the firmware info
let leds = [];        // [name, x, y] in OpenRGB's LED order
let k55Bank = null;   // the K55 RGB PRO XT colour banks (static in OpenRGB: they persist)
let lastColours = null;
let lastSentAt = 0;

function lookup(pid) {
  const p = PRODUCTS.find((entry) => entry[0] === pid);
  return p ? { pid: p[0], name: p[1], protocol: p[2], rule: p[3] } : null;
}

// The detector macros of CorsairPeripheralControllerDetect.cpp.
export function Validate(endpoint) {
  const p = lookup(device.productId());
  if (!p) {
    return false;
  }
  switch (p.rule) {
    case "ip": return endpoint.interface === 1 && endpoint.usage_page === USAGE_PAGE_CUE;
    case "p": return endpoint.usage_page === USAGE_PAGE_CUE;
    case "i": return endpoint.interface === 1;
    case "ip42": return endpoint.interface === 1 && endpoint.usage_page === USAGE_PAGE_K55_PRO_XT;
    default: return false;
  }
}

export function Initialize() {
  kind = null;
  layout = null;
  physical = 0;
  leds = [];
  k55Bank = null;
  lastColours = null;
  lastSentAt = 0;
  product = lookup(device.productId());
  if (!product) {
    device.log(`Corsair Peripheral: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  if (product.protocol === "k65mini") {
    kind = "k65mini";
    k65LightingControl();
    applyGeometry(K65_MINI.size, K65_MINI.leds);
    return;
  }
  if (product.protocol === "k55xt") {
    kind = "k55xt";
    k55Bank = new Array(3 * K55_PRO_XT_BANK).fill(0);
    k55LightingControl();
    applyGeometry(K55_PRO_XT.size, K55_PRO_XT.leds);
    return;
  }
  // CorsairPeripheralController's constructor.
  if (!readFirmwareInfo()) {
    device.log("Corsair Peripheral: the device did not say whether it is a keyboard, mouse or mousepad; not driven.");
    return;
  }
  if (kind === "keyboard" && EXTRA_SETUP.has(layout)) {
    specialFunctionControl();
  }
  lightingControl();
  if (kind === "keyboard" && EXTRA_SETUP.has(layout)) {
    setupK55AndK95LightingControl();
  }
  if (kind === "keyboard") {
    const l = keyboardLayout();
    applyGeometry(l.size, l.leds);
  } else if (kind === "mouse") {
    const names = MOUSE_LED_NAMES[product.name] ?? MOUSE_LED_NAMES.default;
    applyGeometry([15, 1], names.map((n, i) => [n, i, 0]));
  } else if (kind === "mousemat") {
    applyGeometry([15, 1], Array.from({ length: 15 }, (_, i) => [`Mousemat LED ${i + 1}`, i, 0]));
  } else {
    // ST100: the 8-LED base strip on the bottom row, the logo above its middle.
    const strip = Array.from({ length: 8 }, (_, i) => [`Mousemat LED ${i + 1}`, i, 1]);
    applyGeometry([8, 2], [...strip, ["Mousemat LED 9", 4, 0]]);
  }
}

export function Render() {
  if (!kind) {
    return;
  }
  const colours = leds.map((l) => device.color(l[1], l[2]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (setLeds(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step. Its only way back to a hardware effect is the stored-effect write it
// uses for its hardware modes (never sent here), so the lights are switched off: black, through
// the per-frame path. The K55 RGB PRO XT and the K65 Mini go back to their own effect by
// themselves about a minute after the last update.
export function Shutdown(SystemSuspending) {
  if (!kind) {
    return;
  }
  setLeds(leds.map(() => [0, 0, 0]));
}

// The K68 uses the NORMAL tables (RGBController_CorsairPeripheral and SetLEDsKeyboardFull have
// no K68 case); it differs only in its start.
function keyboardLayout() {
  return KEYBOARD_LAYOUTS[layout === "K68" ? "NORMAL" : layout];
}

function applyGeometry(size, list) {
  leds = list;
  device.setName(product.name);
  device.setSize(size.slice());
  device.setControllableLeds(list.map((l) => l[0]), list.map((l) => [l[1], l[2]]));
}

function setLeds(colours) {
  switch (kind) {
    case "keyboard":
      return layout === "K55" ? submitKeyboardZonesColors(colours) : setLedsKeyboardFull(colours);
    case "mouse": return submitMouseColors(colours);
    case "mousemat": return submitMousematColors(colours);
    case "stand": {
      // SetLEDs(): the logo sits in the middle of the base strip, so LED i goes to slot st100[i].
      const remap = new Array(colours.length);
      for (let i = 0; i < 9; i++) {
        remap[ST100_SLOTS[i]] = colours[i];
      }
      return submitMousematColors(remap);
    }
    case "k55xt": return k55SetLeds(colours);
    case "k65mini": return k65SetLeds(colours);
    default: return false;
  }
}

// ---- CUE protocol (CorsairPeripheralController.cpp) ---------------------------------------

function packet() {
  return new Array(PACKET_LENGTH).fill(0);
}

// A copy goes out: some buffers are changed and sent again (as in OpenRGB).
function write(buf) {
  return device.write(buf.slice(), PACKET_LENGTH) !== -1;
}

// ReadFirmwareInfo(): ask for the firmware info on the input-report path; when nothing comes
// back, ask again with a feature report. Up to ten tries until the type byte says keyboard
// (0xC0), mouse (0xC1) or mousepad / headset stand (0xC2). Read-only.
function readFirmwareInfo() {
  let buf = null;
  let offset = 0;
  for (let attempt = 0; attempt < FW_INFO_MAX_ATTEMPTS; attempt++) {
    offset = 0;
    buf = packet();
    buf[0x01] = COMMAND_READ;
    buf[0x02] = PROPERTY_FIRMWARE_INFO;
    device.write(buf.slice(), PACKET_LENGTH);
    const input = device.read([0x00], PACKET_LENGTH, FW_INFO_TIMEOUT_MS);
    if (device.getLastReadSize() > 0 && Array.isArray(input)) {
      // HueDeck returns the report id first; hidapi's buffer (OpenRGB's) starts after it.
      for (let i = 1; i < input.length && i - 1 < PACKET_LENGTH; i++) {
        buf[i - 1] = input[i] & 0xFF;
      }
    } else {
      buf = packet();
      buf[0x01] = COMMAND_READ;
      buf[0x02] = PROPERTY_FIRMWARE_INFO;
      device.send_report(buf.slice(), PACKET_LENGTH);
      const feature = device.get_report([0x00], PACKET_LENGTH);
      if (Array.isArray(feature)) {
        for (let i = 0; i < feature.length && i < PACKET_LENGTH; i++) {
          buf[i] = feature[i] & 0xFF;
        }
      }
      offset = 1;
    }
    const t = buf[0x14 + offset];
    if (t === 0xC0 || t === 0xC1 || t === 0xC2) {
      break;
    }
  }
  switch (buf[0x14 + offset]) {
    case 0xC0: {
      // OpenRGB takes the product id from bytes 0x0E-0x0F of the reply; HueDeck uses the USB
      // product id (the same table; see the hardware notes).
      layout = LOGICAL_LAYOUT[product.pid] ?? "NORMAL";
      const p = buf[0x17 + offset];
      physical = p <= 0x04 ? p : 0x00;
      kind = "keyboard";
      return true;
    }
    case 0xC1:
      kind = "mouse";
      specialFunctionControl();
      return true;
    case 0xC2:
      kind = product.pid === ST100_PID ? "stand" : "mousemat";
      specialFunctionControl();
      return true;
    default:
      return false;
  }
}

// LightingControl(): software lighting.
function lightingControl() {
  const buf = packet();
  buf[0x01] = COMMAND_WRITE;
  buf[0x02] = PROPERTY_LIGHTING_CONTROL;
  buf[0x03] = LIGHTING_CONTROL_SOFTWARE;
  buf[0x05] = kind === "mouse" ? 0x01 : kind === "mousemat" ? 0x04 : 0x03;
  return write(buf);
}

// SpecialFunctionControl().
function specialFunctionControl() {
  const buf = packet();
  buf[0x01] = COMMAND_WRITE;
  buf[0x02] = PROPERTY_SPECIAL_FUNCTION;
  buf[0x03] = LIGHTING_CONTROL_SOFTWARE;
  return write(buf);
}

// SetupK55AndK95LightingControl(): 07 05 08 00 01, then four 07 40 1E packets of 30
// (key identifier, 0xC0) pairs, skipping the identifiers of the layout's list.
function setupK55AndK95LightingControl() {
  let buf = packet();
  buf[0x01] = COMMAND_WRITE;
  buf[0x02] = PROPERTY_LIGHTING_CONTROL;
  buf[0x03] = 0x08;
  buf[0x05] = 0x01;
  write(buf);

  let skipped = KEY_MAPPING_K95_PLAT_ANSI;
  if (physical === LAYOUT_ISO) {
    skipped = layout === "K70_MK2" || layout === "K70_LUX_RGB" ? KEY_MAPPING_K70_MK2_PLAT_ISO : KEY_MAPPING_K95_PLAT_ISO;
  }
  let identifier = 0;
  for (let i = 0; i < 4; i++) {
    buf = packet();
    buf[0x01] = COMMAND_WRITE;
    buf[0x02] = 0x40;
    buf[0x03] = 0x1E;
    for (let j = 0; j < 30; j++) {
      // As OpenRGB: one pass over the list, in order.
      for (const s of skipped) {
        if (identifier === s) {
          identifier++;
        }
      }
      buf[5 + 2 * j] = identifier & 0xFF;
      identifier++;
      buf[5 + 2 * j + 1] = 0xC0;
    }
    write(buf);
  }
}

// SetLEDsKeyboardFull(): red, green and blue planes, each as three stream packets and a submit.
function setLedsKeyboardFull(colours) {
  const l = keyboardLayout();
  const planes = [new Array(168).fill(0), new Array(168).fill(0), new Array(168).fill(0)];
  colours.forEach((c, i) => {
    const slot = l.keys[i];
    planes[0][slot] = c[0] & 0xFF;
    planes[1][slot] = c[1] & 0xFF;
    planes[2][slot] = c[2] & 0xFF;
  });
  let ok = true;
  planes.forEach((plane, ch) => {
    ok = streamPacket(1, 60, plane, 0) && ok;
    ok = streamPacket(2, 60, plane, 60) && ok;
    ok = streamPacket(3, l.dataSize, plane, 120) && ok;
    ok = submitKeyboardFullColors(ch + 1, 3, ch === 2 ? 2 : 1) && ok;
  });
  return ok;
}

function streamPacket(packetId, size, data, from) {
  const buf = packet();
  buf[0x01] = COMMAND_STREAM;
  buf[0x02] = packetId;
  buf[0x03] = size;
  for (let i = 0; i < size; i++) {
    buf[0x05 + i] = data[from + i];
  }
  return write(buf);
}

function submitKeyboardFullColors(channel, count, finish) {
  const buf = packet();
  buf[0x01] = COMMAND_WRITE;
  buf[0x02] = PROPERTY_SUBMIT_KEYBOARD_COLOR_24;
  buf[0x03] = channel;
  buf[0x04] = count;
  buf[0x05] = finish;
  return write(buf);
}

// SubmitKeyboardZonesColors(): the K55 RGB's three zones.
function submitKeyboardZonesColors(colours) {
  const buf = packet();
  buf[0x01] = COMMAND_WRITE;
  buf[0x02] = PROPERTY_SUBMIT_KBZONES_COLOR_24;
  for (let z = 0; z < 3; z++) {
    buf[0x05 + 3 * z] = colours[z][0] & 0xFF;
    buf[0x06 + 3 * z] = colours[z][1] & 0xFF;
    buf[0x07 + 3 * z] = colours[z][2] & 0xFF;
  }
  return write(buf);
}

// SubmitMouseColors(): <zone> <R> <G> <B> per LED.
function submitMouseColors(colours) {
  const buf = packet();
  buf[0x01] = COMMAND_WRITE;
  buf[0x02] = PROPERTY_SUBMIT_MOUSE_COLOR;
  buf[0x03] = colours.length;
  colours.forEach((c, z) => {
    buf[z * 4 + 5] = z;
    buf[z * 4 + 6] = c[0] & 0xFF;
    buf[z * 4 + 7] = c[1] & 0xFF;
    buf[z * 4 + 8] = c[2] & 0xFF;
  });
  return write(buf);
}

// SubmitMousematColors(): <R> <G> <B> per LED.
function submitMousematColors(colours) {
  const buf = packet();
  buf[0x01] = COMMAND_WRITE;
  buf[0x02] = PROPERTY_SUBMIT_MOUSE_COLOR;
  buf[0x03] = colours.length;
  colours.forEach((c, z) => {
    buf[z * 3 + 5] = c[0] & 0xFF;
    buf[z * 3 + 6] = c[1] & 0xFF;
    buf[z * 3 + 7] = c[2] & 0xFF;
  });
  return write(buf);
}

// ---- K55 RGB PRO XT (CorsairK55RGBPROXTController.cpp) -------------------------------------

function k55LightingControl() {
  let buf = packet();
  buf[0x01] = 0x08; buf[0x02] = 0x01; buf[0x03] = 0x03; buf[0x05] = 0x02;
  write(buf);
  buf = packet();
  buf[0x01] = 0x08; buf[0x02] = 0x02; buf[0x03] = 0x5F;
  write(buf);
  buf = packet();
  buf[0x01] = 0x08; buf[0x02] = 0x0D; buf[0x04] = 0x01;
  write(buf);
}

// SetLEDs(): three 137-byte banks (R, G, B) sent as 53 bytes in an 08 06 report and 6 x 61 bytes
// in 08 07 reports. OpenRGB's last report runs 8 bytes past its 411-byte banks; HueDeck sends
// zeros there.
function k55SetLeds(colours) {
  colours.forEach((c, i) => {
    const slot = K55_PRO_XT.slots[i];
    k55Bank[slot] = c[0] & 0xFF;
    k55Bank[K55_PRO_XT_BANK + slot] = c[1] & 0xFF;
    k55Bank[2 * K55_PRO_XT_BANK + slot] = c[2] & 0xFF;
  });
  const data = k55Bank.concat(new Array(8).fill(0));
  let ptr = 0;
  const buf = packet();
  buf[0x01] = 0x08;
  buf[0x02] = 0x06;
  buf[0x04] = 0x9B;
  buf[0x05] = 0x01;
  for (let i = 0; i < PACKET_LENGTH - 12; i++) {
    buf[12 + i] = data[ptr++];
  }
  let ok = write(buf);
  buf[0x02] = 0x07;
  for (let p = 0; p < 6; p++) {
    for (let i = 0; i < PACKET_LENGTH - 4; i++) {
      buf[4 + i] = data[ptr++];
    }
    ok = write(buf) && ok;
  }
  return ok;
}

// ---- K65 Mini (CorsairK65MiniController.cpp) -----------------------------------------------

function k65Write(buf) {
  return device.write(buf.slice(), K65_PACKET_LENGTH) !== -1;
}

function k65LightingControl() {
  let buf = new Array(K65_PACKET_LENGTH).fill(0);
  buf[0x01] = 0x08; buf[0x02] = 0x01; buf[0x03] = 0x03; buf[0x05] = 0x02;
  k65Write(buf);
  buf = new Array(K65_PACKET_LENGTH).fill(0);
  buf[0x01] = 0x08; buf[0x02] = 0x02; buf[0x03] = 0x6E;
  k65Write(buf);
  buf = new Array(K65_PACKET_LENGTH).fill(0);
  buf[0x01] = 0x08; buf[0x02] = 0x0D; buf[0x03] = 0x01; buf[0x04] = 0x22;
  k65Write(buf);
}

// SetLEDs(): 08 06 01 73 01 00 00 12 00, then R G B at 0x0A + 3 * position.
function k65SetLeds(colours) {
  const buf = new Array(K65_PACKET_LENGTH).fill(0);
  buf[0x01] = 0x08;
  buf[0x02] = 0x06;
  buf[0x03] = 0x01;
  buf[0x04] = 0x73;
  buf[0x05] = 0x01;
  buf[0x08] = 0x12;
  colours.forEach((c, i) => {
    const at = 0x0A + K65_MINI.positions[i] * 3;
    buf[at] = c[0] & 0xFF;
    buf[at + 1] = c[1] & 0xFF;
    buf[at + 2] = c[2] & 0xFF;
  });
  return k65Write(buf);
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
