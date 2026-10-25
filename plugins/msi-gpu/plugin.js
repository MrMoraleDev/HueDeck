// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from OpenRGB Controllers/MSIGPUController/MSIGPUController/MSIGPUController.cpp,
// MSIGPUController.h, RGBController_MSIGPU.cpp and MSIGPUControllerDetect.cpp, and
// pci_ids/pci_ids.h (GPL-2.0-or-later) at commit d4798c12, https://gitlab.com/CalcProgrammer1/OpenRGB
// Copyright (C) OpenRGB contributors, see the OpenRGB git history
// (these files are "part of the OpenRGB project" and carry no author line).
//
// Lightloom first-party plugin: the RGB controller of MSI graphics cards at I2C address 0x68 on
// the GPU's own bus (NVIDIA: NvAPI, port 1). One zone, driven in OpenRGB's "Direct" mode:
// static mode at full brightness, then colour 1 (R1/G1/B1) per frame.
//
// Protocol notes and sources: docs/hardware/msi-gpu.md. Status: tested on 10DE:2504 / 1462:3903 (2026-10-19).
//
// Safety, as in OpenRGB: one write per register, no retries; only address 0x68. Register 0x3F
// (MSI_GPU_REG_SAVE, which stores the current state in the card's flash) is written only by
// SaveToDevice(request), the "Save to devices" button after the user confirmed it: write() skips
// it everywhere else, and the host refuses it (SmbusDeniedRegisters()) outside that one call.
// The host renders SMBus devices at most every 33 ms, OpenRGB's TimeToSend for this controller.
//
// Hardware modes (Lightloom extension, docs/host-api/hardware-modes.md): the card's own effects of
// RGBController_MSIGPU (every mode but "Direct", which is the canvas, and "Off", which OpenRGB
// never writes), set live with DeviceUpdateMode() and DeviceUpdateLEDs(): never register 0x3F, so
// the card forgets the mode at the next power cycle.
//
// Per-LED mode (experimental, UNVERIFIED on any card, off by default: the "Per-LED (experimental)"
// parameter). The sequence MSI's LEDKeeper2 sends to non-"NV5000" NVIDIA cards, read from MSI's
// installed files (facts only, docs/research/msi-gpu-perled.md): identify with reads of 0xEC (4
// bytes) and 0xED (2 bytes), enter with 0x2E <- 1, then per frame one raw I2C write of 0x49 and 4
// bytes per LED for 34 slots, and leave with 0x2E <- 0. In this mode the plugin writes only 0x2E
// and 0x49 until Shutdown(), which leaves first and then restores as usual. Which slot is which
// LED, and the colour order (B, G, R is the best guess), are unknown: `lightloom probe
// msi-gpu-perled` exists to find out with the user watching.

/* global device, bus, shutdownColor, perLed */

export function Name() { return "MSI GPU"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/msi-gpu"; }
export function Type() { return "SMBUS"; }
export function DeviceType() { return "gpu"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["GPU"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
export function ControllableParameters() {
  return [
    {
      property: "shutdownColor",
      group: "lighting",
      label: "Shutdown Color",
      description: "Colour shown after Lightloom stops when the card's own state could not be read back",
      type: "color",
      default: "#000000",
    },
    {
      property: "perLed",
      group: "lighting",
      label: "Per-LED (experimental)",
      description: "Colour the card's LEDs one by one with MSI's per-LED stream (registers 0x2E/0x49). Unverified: the LED order and colour order are guesses. Off: one colour for the whole card.",
      type: "boolean",
      default: "false",
    },
  ];
}

// The controller address (MSIGPUControllerDetect.cpp).
const ADDRESS = 0x68;

// Cards this plugin drives: MSIGPUControllerDetect.cpp line 70, "MSI GeForce RTX 3060 Gaming X
// Trio LHR" = NVIDIA_VEN 0x10DE, NVIDIA_RTX3060_LHR_DEV 0x2504, MSI_SUB_VEN 0x1462,
// MSI_RTX3070_GAMING_X_TRIO_SUB_DEV 0x3903 (pci_ids.h). Other OpenRGB entries can be added
// from the same list; leave them out of `tested` in manifest.json until run on the card.
const GPUS = [
  { Vendor: 0x10DE, Device: 0x2504, SubVendor: 0x1462, SubDevice: 0x3903, Address: ADDRESS, Name: "MSI GeForce RTX 3060 Gaming X Trio LHR" },
];

export function BrandGPUList() { return GPUS.map((g) => ({ ...g })); }
// Lightloom extensions (docs/host-api/smbus.md): the only address Scan() may look at, on NVIDIA buses.
export function SmbusAddresses() { return [ADDRESS]; }
export function SmbusBuses() { return ["nvidia"]; }
// Registers the host never lets this plugin write, except inside SaveToDevice(request):
// MSI_GPU_REG_SAVE.
export function SmbusDeniedRegisters() { return [REG_SAVE]; }
// Lightloom extensions (docs/host-api/save-shutdown.md): what the card can store, and the exit modes
// Shutdown(SystemSuspending, mode) carries out.
export function SaveCapabilities() { return { static: true, effects: ["spectrum", "breathing"] }; }
export function ShutdownModes() { return ["restore", "keep", "off"]; }
// The card's built-in effects (see HW_MODES), set live by SetHardwareMode(request).
export function HardwareModes() {
  return HW_MODES.map((m) => ({ id: m.id, name: m.name, colors: m.colors, speed: m.speed, brightness: true }));
}

// Per-LED stream (docs/research/msi-gpu-perled.md section 1.3): 0x2E = 1 on, 0 off; 0x49 the frame.
const REG_DIRECT = 0x2E;
const REG_FRAME = 0x49;
// Identification reads (read only): firmware info (LED type, module type 2 bytes, firmware
// version) and chip id. "NV5000" cards take a different protocol: never driven per LED here.
const REG_FW_INFO = 0xEC;
const REG_CHIP_ID = 0xED;
const NV5000_MODULE = 0x5000;
const NV5000_LED_TYPE = 0x81;
// LED controllers that take the stream on the bus but show nothing of it. 0x0A: the RTX 3060
// Gaming X Trio (1462:3903, firmware 0x11, chip id 0xFFFF), tried 2026-10-25 with and without
// static mode first: every write acknowledged, the card stayed on its own colour.
const IGNORES_STREAM_LED_TYPES = [0x0A];
// MSI always streams 34 records of 4 bytes; laid out as a 34 x 1 strip until the mapping is known.
const PER_LED_SLOTS = 34;

// Registers (MSIGPUController.h).
const REG_BRIGHTNESS = 0x36;
const REG_SPEED = 0x38;
const REG_UNKNOWN = 0x26;
const REG_R1 = 0x30;
const REG_G1 = 0x31;
const REG_B1 = 0x32;
const REG_R2 = 0x27;
const REG_G2 = 0x28;
const REG_B2 = 0x29;
const REG_R3 = 0x2A;
const REG_G3 = 0x2B;
const REG_B3 = 0x2C;
const REG_MODE = 0x22;
// MSI_GPU_REG_SAVE: writes the current state to the card's flash. Written only by commitToFlash().
const REG_SAVE = 0x3F;
// MSIGPUController::Save(): the value written to MSI_GPU_REG_SAVE.
const SAVE_VALUE = 0x01;

const MODE_STATIC = 0x13;
const MODE_RAINBOW = 0x08;
const MODE_BREATHING = 0x04;
// speed_values[MSI_GPU_SPEED_MID], the default speed of OpenRGB's modes (RGBController_MSIGPU.cpp).
const SPEED_MID = 0x02;
// What SaveToDevice() can store: OpenRGB's "Direct"/static (the colour), "Rainbow" (no colour) and
// "Breathing" (colour 1) modes.
const SAVE_MODES = {
  static: { mode: MODE_STATIC, speed: false, color: true },
  spectrum: { mode: MODE_RAINBOW, speed: true, color: false },
  breathing: { mode: MODE_BREATHING, speed: true, color: true },
};
// OpenRGB "Direct" brightness: MSI_GPU_BRIGHTNESS_MAX (5) * MSI_GPU_BRIGHTNESS_MULTI (20).
const BRIGHTNESS_FULL = 5 * 20;
const BRIGHTNESS_MULTI = 20;
const BRIGHTNESS_MAX = 5;
// speed_values (RGBController_MSIGPU.cpp), indexed by MSI_GPU_SPEED_MIN / _MID / _MAX.
const SPEED_VALUES = [0x04, 0x02, 0x01];

// The modes of RGBController_MSIGPU with their MSI_GPU_MODE_* values, minus "Direct" (the canvas)
// and "Off" (DeviceUpdateMode() writes nothing for it). colors: 1 = colour 1 (R1/G1/B1, the modes
// with MODE_FLAG_HAS_PER_LED_COLOR); "Fadein" takes colours 2 and 3 instead, as DeviceUpdateLEDs()
// sends them. Every mode has a brightness; all but "Static" a speed.
const HW_MODES = [
  { id: "static", name: "Static", value: MODE_STATIC, colors: 1, speed: false },
  { id: "rainbow", name: "Rainbow", value: MODE_RAINBOW, colors: 0, speed: true },
  { id: "raindrop", name: "Raindrop", value: 0x1A, colors: 1, speed: true },
  { id: "magic", name: "Magic", value: 0x07, colors: 0, speed: true },
  { id: "patrolling", name: "Patrolling", value: 0x05, colors: 1, speed: true },
  { id: "streaming", name: "Streaming", value: 0x06, colors: 0, speed: true },
  { id: "lightning", name: "Lightning", value: 0x15, colors: 0, speed: true },
  { id: "wave", name: "Wave", value: 0x1F, colors: 1, speed: true },
  { id: "meteor", name: "Meteor", value: 0x16, colors: 1, speed: true },
  { id: "stack", name: "Stack", value: 0x0D, colors: 1, speed: true },
  { id: "rhythm", name: "Rhythm", value: 0x0B, colors: 1, speed: true },
  { id: "flowing", name: "Flowing", value: 0x09, colors: 1, speed: true },
  { id: "whirling", name: "Whirling", value: 0x0F, colors: 1, speed: true },
  { id: "twisting", name: "Twisting", value: 0x11, colors: 1, speed: true },
  { id: "laminating", name: "Laminating", value: 0x1D, colors: 1, speed: true },
  { id: "fadein", name: "Fade in", value: 0x14, colors: 2, speed: true },
  { id: "breathing", name: "Breathing", value: MODE_BREATHING, colors: 1, speed: true },
  { id: "flashing", name: "Flashing", value: 0x02, colors: 1, speed: true },
  { id: "doubleflashing", name: "Double flashing", value: 0x03, colors: 1, speed: true },
];

// What the RGBController constructor reads, in its order: SetupZones() reads the three colours,
// then GetDeviceMode(), GetSpeed() and the brightness. Kept to restore the card on Shutdown().
const COLOR_REGS = [REG_R1, REG_G1, REG_B1, REG_R2, REG_G2, REG_B2, REG_R3, REG_G3, REG_B3];
const STARTUP_REGS = [...COLOR_REGS, REG_UNKNOWN, REG_MODE, REG_SPEED, REG_BRIGHTNESS];

function matches(b, gpu) {
  return b.Vendor() === gpu.Vendor && b.Product() === gpu.Device &&
    b.SubVendor() === gpu.SubVendor && b.SubDevice() === gpu.SubDevice;
}

// Like OpenRGB's detector: an id match on an NVIDIA bus is enough, the card is not probed.
export function Scan(freeBus) {
  if (!freeBus.IsNvidiaBus()) {
    return [];
  }
  const gpu = GPUS.find((g) => matches(freeBus, g));
  return gpu ? [gpu.Address] : [];
}

let saved = null;
// What the last "Save to devices" stored, for a restore when the startup state could not be read.
let baked = null;
let lastColor = null;
let framesSinceSend = 0;
let failureLogged = false;
// The hardware mode the card runs (null: Direct, the canvas).
let hwMode = null;
// Per-LED stream on (0x2E <- 1 sent), the last frame sent, and the identification result
// (null: not read yet; true: the card takes the v1 stream; false: refused).
let perLedOn = false;
let lastFrame = null;
let perLedOk = null;

// The colour is sent again after this many unchanged frames (about 2 s at 30 fps), so a register
// the card dropped, or another program changed, does not stay wrong until the colour changes.
const RESEND_FRAMES = 60;

// One register write, tried a second time when NvAPI reports a failure (single writes do fail now
// and then on a busy bus). True when the card took it.
function write(reg, value) {
  if (reg === REG_SAVE) {
    return false;
  }
  let status = bus.WriteByte(reg, value & 0xFF);
  if (status < 0) {
    status = bus.WriteByte(reg, value & 0xFF);
  }
  if (status < 0 && !failureLogged) {
    failureLogged = true;
    device.log(`MSI GPU: WriteByte(0x${reg.toString(16)}) failed with ${status} (further failures are not logged)`);
  }
  return status >= 0;
}

// Each register is read up to three times: single NvAPI reads fail now and then.
function readStartupState() {
  const values = {};
  for (const reg of STARTUP_REGS) {
    let v = bus.ReadByte(reg);
    for (let attempt = 1; v < 0 && attempt < 3; attempt++) {
      v = bus.ReadByte(reg);
    }
    if (v < 0) {
      device.log(`MSI GPU: ReadByte(0x${reg.toString(16)}) failed with ${v}; Shutdown() will set the shutdown colour`);
      return null;
    }
    values[reg] = v;
  }
  return values;
}

// Put back one saved register; read it back and write it once more if the card did not take it
// (a single NvAPI write can fail, and a missed register leaves the card's effect changed).
function restore(reg) {
  write(reg, saved[reg]);
  if (bus.ReadByte(reg) !== saved[reg]) {
    write(reg, saved[reg]);
  }
}

// RGBController_MSIGPU::DeviceUpdateMode() for "Direct": brightness, then the mode.
function enterDirect() {
  write(REG_BRIGHTNESS, BRIGHTNESS_FULL);
  write(REG_MODE, MODE_STATIC);
  lastColor = null;
}

export function Initialize() {
  const gpu = GPUS.find((g) => matches(bus, g));
  if (gpu) {
    device.setName(gpu.Name);
  }
  saved = readStartupState();
  lastColor = null;
  hwMode = null;
  perLedOn = false;
  perLedOk = null;
  if (perLedWanted() && startPerLed()) {
    return;
  }
  enterDirect();
}

// --- Per-LED mode (experimental) ------------------------------------------------------------

function truthy(value) {
  return value === true || value === 1 || value === "true" || value === "1";
}

function perLedWanted() {
  return typeof perLed !== "undefined" && truthy(perLed);
}

const hex2 = (v) => `0x${v.toString(16).padStart(2, "0")}`;

// The two read-only identification reads; true when the card is not "NV5000" (MSI's v1 stream).
function identifyPerLed() {
  if (perLedOk !== null) {
    return perLedOk;
  }
  const [s1, info] = bus.ReadBlockBytes(REG_FW_INFO, 4);
  const [s2, chip] = bus.ReadBlockBytes(REG_CHIP_ID, 2);
  if (s1 < 0 || s2 < 0 || info.length < 4 || chip.length < 2) {
    device.log(`MSI GPU per-LED: identification reads failed (${s1}, ${s2}); staying in one-colour mode`);
    perLedOk = false;
    return false;
  }
  const ledType = info[0];
  const moduleType = (info[1] << 8) | info[2];
  device.log(`MSI GPU per-LED: LED type ${hex2(ledType)}, module type 0x${moduleType.toString(16)}, firmware ${hex2(info[3])}, chip id 0x${((chip[0] << 8) | chip[1]).toString(16)}`);
  if (IGNORES_STREAM_LED_TYPES.includes(ledType)) {
    device.log(`MSI GPU per-LED: LED type ${hex2(ledType)} ignores the per-LED stream (tested); staying in one-colour mode`);
    perLedOk = false;
    return false;
  }
  perLedOk = moduleType !== NV5000_MODULE && ledType !== NV5000_LED_TYPE;
  if (!perLedOk) {
    device.log("MSI GPU per-LED: an NV5000 controller (another protocol); staying in one-colour mode");
  }
  return perLedOk;
}

function layout(strip) {
  const n = strip ? PER_LED_SLOTS : 1;
  const names = [];
  const positions = [];
  for (let i = 0; i < n; i++) {
    names.push(strip ? `LED ${i + 1}` : "GPU");
    positions.push([i, 0]);
  }
  device.setSize([n, 1]);
  device.setControllableLeds(names, positions);
}

// Identify, lay out the strip and enter the stream (0x2E <- 1). False: one-colour mode stays.
function startPerLed() {
  if (!identifyPerLed()) {
    return false;
  }
  layout(true);
  // Left on Shutdown() even if this write failed: the card may have taken it.
  perLedOn = true;
  lastFrame = null;
  if (!write(REG_DIRECT, 0x01)) {
    device.log("MSI GPU per-LED: entering the stream (0x2E <- 1) failed");
  }
  return true;
}

// 0x2E <- 0: the card goes back to its own effect. Nothing else is written here.
function leavePerLed() {
  if (!perLedOn) {
    return;
  }
  write(REG_DIRECT, 0x00);
  perLedOn = false;
  lastFrame = null;
}

// One 0x49 frame: 34 records of [B, G, R, 0] (MSI reverses the colour bytes; R, G, B input is the
// likely reading, unverified), sampled along the strip.
function perLedFrame() {
  const frame = [REG_FRAME];
  for (let i = 0; i < PER_LED_SLOTS; i++) {
    const [r, g, b] = device.color(i, 0);
    frame.push(b & 0xFF, g & 0xFF, r & 0xFF, 0x00);
  }
  return frame;
}

function renderPerLed() {
  const frame = perLedFrame();
  const same = lastFrame && lastFrame.every((v, i) => v === frame[i]);
  if (same && ++framesSinceSend < RESEND_FRAMES) {
    return;
  }
  framesSinceSend = 0;
  let status = bus.WriteBlockWithoutRegister(frame.length, frame);
  if (status < 0) {
    status = bus.WriteBlockWithoutRegister(frame.length, frame);
  }
  if (status < 0 && !failureLogged) {
    failureLogged = true;
    device.log(`MSI GPU per-LED: frame write failed with ${status} (further failures are not logged)`);
  }
  lastFrame = status >= 0 ? frame : null;
}

// The parameter changed while running: switch modes now (in a hardware mode, on the way back).
export function onperLedChanged() {
  if (hwMode !== null) {
    return;
  }
  if (perLedWanted() && !perLedOn) {
    startPerLed();
  } else if (!perLedWanted() && perLedOn) {
    leavePerLed();
    layout(false);
    enterDirect();
  }
}

// RGBController_MSIGPU::DeviceUpdateLEDs(): register 0x26 cleared, then SetRGB1. True when every
// write went through.
function setColor(r, g, b) {
  let ok = write(REG_UNKNOWN, 0x00);
  ok = write(REG_R1, r) && ok;
  ok = write(REG_G1, g) && ok;
  ok = write(REG_B1, b) && ok;
  return ok;
}

// MSIGPUController::SetRGB2() / SetRGB3().
function setColorAt(regs, [r, g, b]) {
  write(regs[0], r & 0xFF);
  write(regs[1], g & 0xFF);
  write(regs[2], b & 0xFF);
}

const percent = (v, fallback) => (Number.isFinite(Number(v)) ? Math.min(100, Math.max(0, Number(v))) : fallback);

// Hardware modes (Lightloom extension): put the card in one of its own effects, live, the way
// OpenRGB switches modes: DeviceUpdateMode() (brightness, the speed for modes that have one, the
// mode), then DeviceUpdateLEDs() for modes with colours (0x26, colour 1, or colours 2 and 3 for
// Fade in). request = null goes back to Direct for the canvas. Never touches register 0x3F.
//   request = { id, colors: [[r, g, b], ...], speed: 0-100, brightness: 0-100 }
export function SetHardwareMode(request) {
  if (!request) {
    hwMode = null;
    if (perLedWanted() && startPerLed()) {
      return true;
    }
    enterDirect();
    return true;
  }
  const mode = HW_MODES.find((m) => m.id === request.id);
  if (!mode) {
    throw new Error(`MSI GPU: no hardware mode "${request.id}"`);
  }
  leavePerLed();
  const colors = Array.isArray(request.colors) ? request.colors : [];
  const color = (i) => (Array.isArray(colors[i]) ? colors[i] : [255, 255, 255]);
  // modes[].brightness 0-5 times MSI_GPU_BRIGHTNESS_MULTI; speed index MIN / MID / MAX.
  const level = Math.round((percent(request.brightness, 100) / 100) * BRIGHTNESS_MAX);
  write(REG_BRIGHTNESS, level * BRIGHTNESS_MULTI);
  if (mode.speed) {
    write(REG_SPEED, SPEED_VALUES[Math.round(percent(request.speed, 50) / 50)]);
  }
  write(REG_MODE, mode.value);
  if (mode.colors === 2) {
    write(REG_UNKNOWN, 0x00);
    setColorAt([REG_R2, REG_G2, REG_B2], color(0));
    setColorAt([REG_R3, REG_G3, REG_B3], color(1));
  } else if (mode.colors === 1) {
    const [r, g, b] = color(0);
    setColor(r & 0xFF, g & 0xFF, b & 0xFF);
  }
  lastColor = null;
  hwMode = mode.id;
  return true;
}

export function Render() {
  // The host stops calling Render() in a hardware mode; nothing to send then anyway.
  if (hwMode !== null) {
    return;
  }
  if (perLedOn) {
    renderPerLed();
    return;
  }
  const [r, g, b] = device.color(0, 0);
  const same = lastColor && lastColor[0] === r && lastColor[1] === g && lastColor[2] === b;
  if (same && ++framesSinceSend < RESEND_FRAMES) {
    return;
  }
  framesSinceSend = 0;
  // A colour the card did not fully take is not remembered: the next frame sends it again.
  lastColor = setColor(r, g, b) ? [r, g, b] : null;
}

function hexColor(text) {
  const m = /^#?([0-9a-f]{6})$/i.exec(String(text ?? ""));
  const v = m ? parseInt(m[1], 16) : 0;
  return [(v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF];
}

// "Save to devices" (Lightloom extension): store the frame's colour, or a built-in effect, in the
// card's flash. As OpenRGB does it: DeviceUpdateMode() (brightness, the speed for modes that have
// one, the mode), DeviceUpdateLEDs() (0x26, then colour 1) and DeviceSaveMode() (0x3F <- 1). The
// card then goes back to Direct (static at full brightness) for the live effect, and a later
// Shutdown(false, "restore") puts back what was stored. A preview only describes it.
export function SaveToDevice(request) {
  const effect = request && request.effect ? String(request.effect) : null;
  const plan = SAVE_MODES[effect ?? "static"];
  if (!plan) {
    return { unsupported: `the card has no "${effect}" effect Lightloom can store` };
  }
  const first = request && Array.isArray(request.colors) && request.colors.length > 0 ? request.colors[0] : device.color(0, 0);
  const [r, g, b] = [first[0] & 0xFF, first[1] & 0xFF, first[2] & 0xFF];
  const result = {
    effect: effect ?? "static",
    zones: plan.color ? [{ name: "GPU", color: hex(r, g, b) }] : [],
    note: plan.color ? "One colour for the whole card." : "The card's own colour cycle.",
  };
  if (!request || request.action !== "save") {
    return result;
  }
  const resumePerLed = perLedOn;
  leavePerLed();
  write(REG_BRIGHTNESS, BRIGHTNESS_FULL);
  if (plan.speed) {
    write(REG_SPEED, SPEED_MID);
  }
  write(REG_MODE, plan.mode);
  if (plan.color) {
    setColor(r, g, b);
  }
  commitToFlash();
  baked = { mode: plan.mode, speed: plan.speed, color: plan.color ? [r, g, b] : null };
  if (saved) {
    saved[REG_BRIGHTNESS] = BRIGHTNESS_FULL;
    saved[REG_MODE] = plan.mode;
    if (plan.speed) {
      saved[REG_SPEED] = SPEED_MID;
    }
    if (plan.color) {
      saved[REG_UNKNOWN] = 0x00;
      saved[REG_R1] = r;
      saved[REG_G1] = g;
      saved[REG_B1] = b;
    }
  }
  // Back to Direct (DeviceUpdateMode() for Direct); the next frame is sent whole.
  write(REG_BRIGHTNESS, BRIGHTNESS_FULL);
  write(REG_MODE, MODE_STATIC);
  lastColor = null;
  if (resumePerLed) {
    startPerLed();
  }
  // (A hardware mode the card ran is set again by the host after the save.)
  hwMode = null;
  return result;
}

// MSIGPUController::Save(). The only write of register 0x3F; called from SaveToDevice() only.
function commitToFlash() {
  const status = bus.WriteByte(REG_SAVE, SAVE_VALUE);
  if (status < 0) {
    throw new Error(`MSI GPU: writing the save register failed with ${status}`);
  }
}

function hex(r, g, b) {
  return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

// Shutdown(SystemSuspending, mode), mode a Lightloom extension:
//   "restore" (and SignalRGB, which passes no mode): the card as it was at start (or as last
//     stored by SaveToDevice): colours, speed and brightness back, the mode last (it applies
//     them). When the startup state could not be read, the shutdown colour in static mode.
//   "keep": the last colour Lightloom showed, in static mode (not stored: gone at the next boot);
//     in a hardware mode, that mode stays (it is not stored either).
//   "off": static black (not stored).
export function Shutdown(SystemSuspending, mode) {
  // Per-LED mode: leave the stream (0x2E <- 0) before anything else.
  leavePerLed();
  if (mode === "keep" && hwMode !== null) {
    return;
  }
  if (mode === "keep") {
    const [r, g, b] = lastColor ?? device.color(0, 0);
    setColor(r & 0xFF, g & 0xFF, b & 0xFF);
    write(REG_MODE, MODE_STATIC);
    lastColor = null;
    return;
  }
  if (mode === "off") {
    setColor(0, 0, 0);
    write(REG_MODE, MODE_STATIC);
    lastColor = null;
    return;
  }
  if (saved) {
    restore(REG_UNKNOWN);
    for (const reg of COLOR_REGS) {
      restore(reg);
    }
    restore(REG_SPEED);
    restore(REG_BRIGHTNESS);
    restore(REG_MODE);
  } else if (baked) {
    // The startup state is unknown, but the user stored this in the card: show it again.
    write(REG_BRIGHTNESS, BRIGHTNESS_FULL);
    if (baked.speed) {
      write(REG_SPEED, SPEED_MID);
    }
    if (baked.color) {
      setColor(baked.color[0], baked.color[1], baked.color[2]);
    }
    write(REG_MODE, baked.mode);
  } else {
    const [r, g, b] = hexColor(typeof shutdownColor === "string" ? shutdownColor : "#000000");
    setColor(r, g, b);
    write(REG_MODE, MODE_STATIC);
  }
  lastColor = null;
}
