// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the five ARGB fan headers of the Thermaltake Riing, Riing Quad and Riing Trio
// USB lighting controllers (264A), per LED.
// Adapted from OpenRGB Controllers/ThermaltakeRiingController/ThermaltakeRiingControllerDetect.cpp,
// ThermaltakeRiingController/ThermaltakeRiingController.cpp and .h,
// ThermaltakeRiingController/RGBController_ThermaltakeRiing.cpp and .h,
// ThermaltakeRiingQuadController/ThermaltakeRiingQuadController.cpp and .h,
// ThermaltakeRiingQuadController/RGBController_ThermaltakeRiingQuad.cpp and .h,
// ThermaltakeRiingTrioController/ThermaltakeRiingTrioController.cpp and .h,
// ThermaltakeRiingTrioController/RGBController_ThermaltakeRiingTrio.cpp and .h
// (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  07 Feb 2020" (ThermaltakeRiingController,
// ThermaltakeRiingControllerDetect.cpp), "Adam Honse (CalcProgrammer1)  09 Feb 2020"
// (RGBController_ThermaltakeRiing), "Chris M (Dr_No)  15 Feb 2021" (ThermaltakeRiingQuadController,
// RGBController_ThermaltakeRiingQuad, ThermaltakeRiingControllerDetect.cpp), "Sam B (4rcheria)
// 24 Nov 2024" (ThermaltakeRiingTrioController, RGBController_ThermaltakeRiingTrio,
// ThermaltakeRiingControllerDetect.cpp).
//
// Protocol notes: docs/hardware/thermaltake-riing.md. In short: output reports with report id
// 0x00; init `FE 33`; per channel `32 52 <port> <mode> G R B ...`. Riing: 65 bytes, up to 20 LEDs,
// Direct mode 0x18, an input report read after every write. Riing Quad: 193 bytes, up to 60 LEDs,
// mode 0x24, needs a packet every few seconds. Riing Trio: 65-byte chunks of 19 LEDs, up to 54
// LEDs, mode 0x24. No fan speed, no save: OpenRGB declares SendFan()/SendSave() but never
// implements or calls them, and neither does this plugin.

/* global device */

const VENDOR_ID = 0x264A;

const RIING = "RIING";
const QUAD = "QUAD";
const TRIO = "TRIO";

function range(first, last) {
  const out = [];
  for (let p = first; p <= last; p++) {
    out.push(p);
  }
  return out;
}

// ThermaltakeRiingControllerDetect.cpp, in registration order.
const PRODUCTS = [
  ...range(0x1FA5, 0x1FB5).map((pid) => ({ pid, variant: RIING, name: "Thermaltake Riing" })),
  ...[...range(0x2260, 0x2270), 0x232B].map((pid) => ({ pid, variant: QUAD, name: "Thermaltake Riing Quad" })),
  ...[...range(0x2135, 0x213F), ...range(0x2141, 0x2145)].map((pid) => ({ pid, variant: TRIO, name: "Thermaltake Riing Trio" })),
];

// Per variant: report length, the most LEDs a header takes (RGBController_*::SetupZones leds_max),
// the Direct mode byte (mode + (speed & 0x03), speed 0), the init read timeout.
const VARIANTS = {
  [RIING]: { length: 65, maxLeds: 20, directMode: 0x18, initTimeout: 100 },
  [QUAD]: { length: 193, maxLeds: 60, directMode: 0x24, initTimeout: 250 },
  [TRIO]: { length: 65, maxLeds: 54, directMode: 0x24, initTimeout: 250 },
};

const NUM_CHANNELS = 5;
// ThermaltakeRiingController: hid_read_timeout after every RGB packet.
const RIING_READ_TIMEOUT_MS = 100;
// Riing only: THERMALTAKE_MODE_SPECTRUM and THERMALTAKE_SPEED_NORMAL, the hardware effect the
// controller is handed back to on Shutdown.
const RIING_MODE_SPECTRUM = 0x04;
const RIING_SPEED_NORMAL = 0x02;
// Trio: THERMALTAKE_TRIO_CHUNK_ID, _DATA_BYTE, _CHUNK_LENGTH.
const TRIO_CHUNK_ID = 6;
const TRIO_DATA_BYTE = 8;
const TRIO_CHUNK_LENGTH = 19;
// Unchanged frames are sent again after this long (the Quad wants a packet within 3 s).
const KEEPALIVE_MS = 1000;

export function Name() { return "Thermaltake Riing"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/thermaltake-riing"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "lightingcontroller"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// Thermaltake's own software drives the controllers while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["TT RGB Plus.exe", "TTRGBPlus.exe", "TT RGB Plus 2.0.exe", "TTRGBPLUSService.exe"]; }

let product = null;
let variant = null;
// Quad and Trio: OpenRGB's persistent per-channel buffers (bytes of a longer earlier frame stay).
let buffers = null;
let lastFrame = null;
let lastFrameAt = 0;

// REGISTER_HID_DETECTOR (no interface, usage page or usage): any collection of the device.
export function Validate(endpoint) {
  return endpoint != null;
}

export function Initialize() {
  lastFrame = null;
  lastFrameAt = 0;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  variant = product ? VARIANTS[product.variant] : null;
  if (!variant) {
    device.log(`Thermaltake Riing: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize([1, 1]);
  device.setControllableLeds([], []);
  // OpenRGB zone names: "Thermaltake Fan Header n", 0..leds_max LEDs each.
  device.SetLedLimit(NUM_CHANNELS * variant.maxLeds);
  for (let ch = 0; ch < NUM_CHANNELS; ch++) {
    device.addChannel(channelName(ch), variant.maxLeds);
  }
  buffers = [];
  for (let ch = 0; ch < NUM_CHANNELS; ch++) {
    const buf = new Array(variant.length).fill(0);
    buf[0] = 0x00;
    buf[1] = 0x32;
    buf[2] = 0x52;
    buffers.push(buf);
  }
  // SendInit(): FE 33, then one input report read.
  const init = new Array(variant.length).fill(0);
  init[0x01] = 0xFE;
  init[0x02] = 0x33;
  device.write(init, variant.length);
  device.read([0x00], variant.length, variant.initTimeout);
}

export function Render() {
  if (!variant) {
    return;
  }
  const frame = [];
  for (let ch = 0; ch < NUM_CHANNELS; ch++) {
    frame.push(channelGrb(ch));
  }
  const now = Date.now();
  if (lastFrame !== null && sameFrame(frame, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
    return;
  }
  if (sendFrame(frame, variant.directMode)) {
    lastFrame = frame;
    lastFrameAt = now;
  } else {
    lastFrame = null;
  }
}

// OpenRGB has no exit step. The Riing has hardware effects that OpenRGB selects with the same
// report (no save involved): it is handed back to Spectrum at Normal speed, with black colour
// data. The Quad and Trio have only Direct in OpenRGB: their LEDs are set black.
export function Shutdown(SystemSuspending) {
  if (!variant) {
    return;
  }
  const frame = [];
  for (let ch = 0; ch < NUM_CHANNELS; ch++) {
    frame.push(new Array(channelGrb(ch).length).fill(0));
  }
  if (product.variant === RIING) {
    sendFrame(frame, RIING_MODE_SPECTRUM + (RIING_SPEED_NORMAL & 0x03));
  } else {
    sendFrame(frame, variant.directMode);
  }
  lastFrame = null;
}

// RGBController_*::DeviceUpdateLEDs: SetChannelLEDs for each of the five zones in order.
function sendFrame(frame, mode) {
  let ok = true;
  frame.forEach((grb, ch) => {
    if (product.variant === RIING) {
      ok = sendRiing(ch, mode, grb) && ok;
    } else if (product.variant === QUAD) {
      ok = sendQuad(ch, mode, grb) && ok;
    } else {
      ok = sendTrio(ch, grb) && ok;
    }
  });
  return ok;
}

// ThermaltakeRiingController::SendRGB: a fresh zeroed 65-byte buffer, then a read.
function sendRiing(ch, mode, grb) {
  const buf = new Array(variant.length).fill(0);
  buf[0x01] = 0x32;
  buf[0x02] = 0x52;
  buf[0x03] = ch + 1;
  buf[0x04] = mode & 0xFF;
  for (let i = 0; i < grb.length; i++) {
    buf[0x05 + i] = grb[i];
  }
  const ok = device.write(buf, variant.length) !== -1;
  device.read([0x00], variant.length, RIING_READ_TIMEOUT_MS);
  return ok;
}

// ThermaltakeRiingQuadController::SetChannelLEDs: the channel's persistent 193-byte buffer.
// Sent even for a header with no LEDs, as OpenRGB does.
function sendQuad(ch, mode, grb) {
  const buf = buffers[ch];
  buf[0x03] = ch + 1;
  buf[0x04] = mode & 0xFF;
  for (let i = 0; i < grb.length; i++) {
    buf[0x05 + i] = grb[i];
  }
  return device.write(buf.slice(), variant.length) !== -1;
}

// ThermaltakeRiingTrioController::SetChannelLEDs: nothing for a header with no LEDs; otherwise
// chunks of 19 LEDs (at least two), bytes 6.. cleared before each.
function sendTrio(ch, grb) {
  const numColors = grb.length / 3;
  if (numColors === 0) {
    return true;
  }
  const buf = buffers[ch];
  buf[0x03] = ch + 1;
  buf[0x04] = 0x24;
  buf[0x05] = 0x03;
  buf[0x07] = 0x00;
  let ok = true;
  for (let i = 0; numColors > TRIO_CHUNK_LENGTH * i || i < 2; i++) {
    const transmitted = TRIO_CHUNK_LENGTH * i;
    const toTransmit = numColors > transmitted ? numColors - transmitted : 0;
    buf.fill(0, TRIO_CHUNK_ID);
    buf[TRIO_CHUNK_ID] = (i + 1) & 0xFF;
    const count = Math.min(toTransmit, TRIO_CHUNK_LENGTH);
    for (let k = 0; k < count * 3; k++) {
      buf[TRIO_DATA_BYTE + k] = grb[transmitted * 3 + k];
    }
    ok = device.write(buf.slice(), variant.length) !== -1 && ok;
  }
  return ok;
}

// The header's colours as OpenRGB's color_data: G, R, B per LED, at most leds_max LEDs.
function channelGrb(ch) {
  const channel = device.channel(channelName(ch));
  const rgb = channel ? channel.getColors("Inline", "RGB") : [];
  const count = Math.min(Math.floor((rgb ? rgb.length : 0) / 3), variant.maxLeds);
  const out = [];
  for (let led = 0; led < count; led++) {
    out.push(rgb[led * 3 + 1] & 0xFF, rgb[led * 3] & 0xFF, rgb[led * 3 + 2] & 0xFF);
  }
  return out;
}

function channelName(ch) {
  return `Fan Header ${ch + 1}`;
}

function sameFrame(a, b) {
  for (let ch = 0; ch < a.length; ch++) {
    if (a[ch].length !== b[ch].length) {
      return false;
    }
    for (let i = 0; i < a[ch].length; i++) {
      if (a[ch][i] !== b[ch][i]) {
        return false;
      }
    }
  }
  return true;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
