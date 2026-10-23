// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: Corsair peripherals on the "V2" protocol (K55 RGB PRO, K57 RGB wired, K60 RGB
// PRO family, K70 Core / RGB PRO / RGB TKL, K95 RGB PLATINUM XT, K100; Dark Core RGB SE / Pro SE,
// Harpoon / Ironclaw Wireless, Katar Pro, M55, M65 RGB Ultra, M75, Scimitar Elite RGB; MM700;
// and the mice and keyboards OpenRGB knows behind a Slipstream wireless receiver), per LED.
// Adapted from OpenRGB Controllers/CorsairPeripheralV2Controller/CorsairPeripheralV2Controller.cpp
// and .h, CorsairPeripheralV2HardwareController.cpp and .h, CorsairPeripheralV2SoftwareController.cpp
// and .h, RGBController_CorsairV2Hardware.cpp and .h, RGBController_CorsairV2Software.cpp and .h,
// CorsairPeripheralV2ControllerDetect.cpp, CorsairPeripheralV2Devices.cpp and .h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Chris M (Dr_No)  07 Aug 2022" (CorsairPeripheralV2Controller);
// "Chris M (Dr_No)  07 Dec 2022" (CorsairPeripheralV2HardwareController); "Chris M (Dr_No)
// 11 Aug 2022" (CorsairPeripheralV2SoftwareController, RGBController_CorsairV2Software);
// "Chris M (Dr_No)  10 Dec 2022" (RGBController_CorsairV2Hardware); the detector and device list
// name no author ("This file is part of the OpenRGB project"). The product and LED tables in
// devices.js are generated from OpenRGB's tables; the protocol below is written by hand.
//
// Protocol notes: docs/hardware/corsair-peripheral-v2.md. In short: interface 1, usage page
// 0xFF42, 65-byte output reports [0x00, write_cmd, command, ...] answered by 64-byte input
// reports. Start: read the product id (address 0x12; a wireless receiver answers its own id, then
// write_cmd 0x09 reaches the device behind it), the vendor id, probe the lighting endpoint, set
// software render mode (0x01 0x03 = 2), read the keyboard layout (0x41). Every frame: start a
// transaction, a block write of the colour buffer (0x06 first packet with the length, 0x07 for
// the rest), stop the transaction. Shutdown: hardware render mode (0x01 0x03 = 1), as OpenRGB's
// destructor does, which hands the lights back to the device's own lighting.
// Never sent: OpenRGB's hardware-effect block write (commented out in OpenRGB as well), anything
// that is not a lighting command (no DPI, polling rate, profile, macro or power setting).

/* global device */

import { DETECTORS, DEVICES, TABLES, VENDOR_ID } from "./devices.js";

const PRODUCT_IDS = DETECTORS.map((d) => d[1]);

// CorsairPeripheralV2Controller.h
const TIMEOUT = 50;
const TIMEOUT_SHORT = 3;
const VALUE_MODE = 0x03;
const WRITE_WIRED_ID = 0x08;
const WRITE_WIRELESS_ID = 0x09;
const WRITE_SIZE = 65;
const PACKET_SIZE = 1024;
const LIGHT_CTRL1 = 0x01;
const LIGHT_CTRL2 = 0x22;
const WRITE_PACE = 2;
const PID_RETRIES = 8;
const PID_RETRY_DELAY = 120;
const NA = 0xFFFFFFFF;

const CMD_SET = 0x01;
const CMD_GET = 0x02;
const CMD_STOP_TX = 0x05;
const CMD_BLK_W1 = 0x06;
const CMD_BLK_WN = 0x07;
const CMD_START_TX = 0x0D;

const MODE_DIRECT = 0x0012;
const RENDER_HW = 0x01;
const RENDER_SW = 0x02;
// CorsairPeripheralV2HardwareController.h CORSAIR_V2HW_DATA_OFFSET
const HW_DATA_OFFSET = 2;

// CorsairPeripheralV2Devices.h
const SLIPSTREAM_RECEIVERS = new Set([0x1BA6, 0x1B66, 0x1B65, 0x1BDC, 0x2B00]);
const K57_RGB_WIRED = 0x1B6E;
const K100 = new Set([0x1B7C, 0x1BC5, 0x1B7D]);

/** Unchanged frames are sent again after this long (OpenRGB's keepalive resends after 30 s). */
const RESEND_MS = 1000;
/** Lightloom bound on the read-buffer drain (OpenRGB loops while reads return data). */
const MAX_DRAIN_READS = 64;

export function Name() { return "Corsair Peripheral V2"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/corsair-peripheral-v2"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCT_IDS.slice(); }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
// Placeholders; Initialize() sets the real geometry once the device has said what it is.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["LED"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// iCUE holds these devices while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["iCUE.exe", "Corsair.Service.exe", "CueLLAccessService.exe"]; }

let writeCmd = WRITE_WIRED_ID;
let lightCtrl = LIGHT_CTRL2;
let skipReads = false;
let hardwareController = false;
let table = null;
let lastBuffer = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_IP(..., 1, 0xFF42) for every product: interface 1, usage page 0xFF42.
export function Validate(endpoint) {
  return endpoint.interface === 1 && endpoint.usage_page === 0xFF42;
}

export function Initialize() {
  writeCmd = WRITE_WIRED_ID;
  lightCtrl = LIGHT_CTRL2;
  skipReads = false;
  table = null;
  lastBuffer = null;
  lastSentAt = 0;
  const detector = DETECTORS.find((d) => d[1] === device.productId());
  if (!detector) {
    device.log(`Corsair V2: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  hardwareController = detector[2] === "HW";

  // CorsairPeripheralV2Controller constructor.
  let pid = getAddressRetry(0x12);
  if (SLIPSTREAM_RECEIVERS.has(pid)) {
    writeCmd = WRITE_WIRELESS_ID;
    pid = getAddressRetry(0x12);
  } else if (pid === K57_RGB_WIRED) {
    writeCmd = 0x80;
    lightCtrl = LIGHT_CTRL1;
    skipReads = true;
  }
  const entry = DEVICES[pid];
  if (!entry) {
    // OpenRGB drops the device here (after a vendor id query and a switch to software mode and
    // back); Lightloom sends nothing more.
    device.log(`Corsair V2: the device answered product id 0x${hex4(pid)}, which OpenRGB's device list does not have; nothing is driven.`);
    return;
  }

  // Vendor id query: OpenRGB never clears this buffer; Lightloom sends zeros after the header.
  const vid = new Array(WRITE_SIZE).fill(0);
  vid[1] = writeCmd;
  vid[2] = CMD_GET;
  vid[3] = 0x11;
  device.write(vid, WRITE_SIZE);
  device.read([0x00], PACKET_SIZE, TIMEOUT);

  if (lightCtrl === LIGHT_CTRL2 && !K100.has(pid)) {
    const result = startTransaction(0);
    if (result > 0) {
      lightCtrl = LIGHT_CTRL1;
      startTransaction(0);
    }
    stopTransaction(0);
  }

  // CorsairPeripheralV2HWController / SWController constructors.
  setRenderMode(RENDER_SW);
  lightingControl(0x5F);

  // RGBController_CorsairV2HW / SW SetupZones(): the layout decides the key table.
  const layout = getAddress(0x41);
  const code = layout === 2 || layout === 4 ? layout : 1;
  table = TABLES[entry.layouts[code]];

  device.setName(SLIPSTREAM_RECEIVERS.has(device.productId())
    ? `${detector[0]}: ${entry.name ?? `device 0x${hex4(pid)}`}`
    : detector[0]);
  device.setSize(table.size.slice());
  device.setControllableLeds(table.leds.map((l) => l[0]), table.leds.map((l) => [l[2], l[3]]));
}

export function Render() {
  if (!table) {
    return;
  }
  const buffer = colourBuffer((x, y) => device.color(x, y));
  const now = Date.now();
  if (lastBuffer !== null && sameBuffer(buffer, lastBuffer) && now - lastSentAt < RESEND_MS) {
    return;
  }
  setLedsDirect(buffer);
  lastBuffer = buffer;
  lastSentAt = now;
}

// OpenRGB's ~CorsairPeripheralV2Controller(): SetRenderMode(CORSAIR_V2_MODE_HW), which hands the
// lights back to the device's own (hardware) lighting.
export function Shutdown(SystemSuspending) {
  if (!table) {
    return;
  }
  setRenderMode(RENDER_HW);
}

/** RGBController_CorsairV2*::SetupZones() buffer_map: LED colours at their led.value, black elsewhere. */
function colourBuffer(colourAt) {
  const buffer = Array.from({ length: table.bufferSize }, () => [0, 0, 0]);
  for (const [, value, x, y] of table.leds) {
    buffer[value] = colourAt(x, y);
  }
  return buffer;
}

// CorsairPeripheralV2SWController::SetLedsDirect and CorsairPeripheralV2HWController::SetLedsDirect.
function setLedsDirect(colours) {
  const count = colours.length;
  let data;
  if (hardwareController && lightCtrl === LIGHT_CTRL2) {
    // SetLedsDirectTriplets: Direct mode id, then R G B per LED.
    data = new Array(count * 3 + HW_DATA_OFFSET).fill(0);
    data[0] = MODE_DIRECT & 0xFF;
    data[1] = MODE_DIRECT >> 8;
    colours.forEach(([r, g, b], i) => {
      const idx = i * 3 + HW_DATA_OFFSET;
      data[idx] = r & 0xFF;
      data[idx + 1] = g & 0xFF;
      data[idx + 2] = b & 0xFF;
    });
  } else {
    // SetLedsDirectColourBlocks / the SW controller: all reds, all greens, all blues.
    data = new Array(count * 3).fill(0);
    colours.forEach(([r, g, b], i) => {
      data[i] = r & 0xFF;
      data[count + i] = g & 0xFF;
      data[count * 2 + i] = b & 0xFF;
    });
  }
  setLeds(data);
}

// CorsairPeripheralV2Controller::SetLEDs.
function setLeds(data) {
  const offset1 = 8;
  const offset2 = 4;
  const size = data.length;
  let remaining = size;

  clearPacketBuffer();
  startTransaction(0);

  const buffer = new Array(WRITE_SIZE).fill(0);
  buffer[1] = writeCmd;
  buffer[2] = CMD_BLK_W1;
  buffer[4] = size & 0xFF;
  buffer[5] = (size >> 8) & 0xFF;
  let copy = Math.min(WRITE_SIZE - offset1, remaining);
  for (let i = 0; i < copy; i++) {
    buffer[offset1 + i] = data[i];
  }
  device.write(buffer.slice(), WRITE_SIZE);
  if (!skipReads) {
    // Wait for the answer before the next packet; without one, pace the transfer instead.
    device.read([0x00], WRITE_SIZE, TIMEOUT_SHORT);
    if (device.getLastReadSize() <= 0) {
      device.pause(WRITE_PACE);
    }
  }

  remaining -= copy;
  buffer[2] = CMD_BLK_WN;
  copy = WRITE_SIZE - offset2;
  while (remaining > 0) {
    const index = size - remaining;
    if (remaining < copy) {
      buffer.fill(0, offset2);
      copy = remaining;
    }
    for (let i = 0; i < copy; i++) {
      buffer[offset2 + i] = data[index + i];
    }
    device.write(buffer.slice(), WRITE_SIZE);
    device.pause(WRITE_PACE);
    remaining -= copy;
  }

  stopTransaction(0);
}

// CorsairPeripheralV2Controller::ClearPacketBuffer: read until nothing is pending.
function clearPacketBuffer() {
  if (skipReads) {
    return;
  }
  for (let i = 0; i < MAX_DRAIN_READS; i++) {
    device.read([0x00], WRITE_SIZE, TIMEOUT_SHORT);
    if (device.getLastReadSize() <= 0) {
      break;
    }
  }
}

function command(cmd, a3, a4, a5) {
  const buffer = new Array(WRITE_SIZE).fill(0);
  buffer[1] = writeCmd;
  buffer[2] = cmd;
  buffer[3] = a3 ?? 0;
  buffer[4] = a4 ?? 0;
  buffer[5] = a5 ?? 0;
  device.write(buffer, WRITE_SIZE);
}

/** The answer's bytes as OpenRGB's buffer sees them (hidapi data, without the report id). */
function readAnswer(length, timeout) {
  const reply = device.read([0x00], length, timeout);
  if (device.getLastReadSize() <= 0 || !reply || reply.length < 2) {
    return null;
  }
  return reply.slice(1);
}

// StartTransaction: returns the status byte; a request that is not answered leaves OpenRGB's
// buffer as written, so the "status" is the command byte (0x0D).
function startTransaction(opt) {
  command(CMD_START_TX, opt, lightCtrl);
  if (skipReads) {
    return CMD_START_TX;
  }
  const answer = readAnswer(WRITE_SIZE, TIMEOUT);
  return answer ? (answer[2] ?? 0) : CMD_START_TX;
}

function stopTransaction(opt) {
  command(CMD_STOP_TX, 0x01, opt);
  if (!skipReads) {
    device.read([0x00], WRITE_SIZE, TIMEOUT);
  }
}

// SetRenderMode: SET value 0x03 (render mode) = 1 hardware / 2 software.
function setRenderMode(mode) {
  command(CMD_SET, VALUE_MODE, 0x00, mode);
  if (!skipReads) {
    device.read([0x00], WRITE_SIZE, TIMEOUT);
  }
}

// LightingControl: a GET of the given address (OpenRGB sends 0x5F).
function lightingControl(opt) {
  command(CMD_GET, opt, 0x00, 0x00);
  if (!skipReads) {
    device.read([0x00], WRITE_SIZE, TIMEOUT);
  }
}

// GetAddress: always reads the answer (even on the K57); NA on an error status, 0 when unanswered.
function getAddress(address) {
  command(CMD_GET, address);
  const answer = readAnswer(WRITE_SIZE, TIMEOUT);
  if (!answer) {
    return 0;
  }
  if ((answer[2] ?? 0) > 0) {
    return NA;
  }
  return ((answer[6] ?? 0) << 24 | (answer[5] ?? 0) << 16 | (answer[4] ?? 0) << 8 | (answer[3] ?? 0)) >>> 0;
}

function getAddressRetry(address) {
  let value = NA;
  for (let attempt = 0; attempt < PID_RETRIES; attempt++) {
    value = getAddress(address);
    if (value !== NA && value !== 0) {
      return value;
    }
    if (attempt + 1 < PID_RETRIES) {
      device.pause(PID_RETRY_DELAY);
    }
  }
  return value;
}

function sameBuffer(a, b) {
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
