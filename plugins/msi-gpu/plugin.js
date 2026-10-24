// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from OpenRGB Controllers/MSIGPUController/MSIGPUController/MSIGPUController.cpp,
// MSIGPUController.h, RGBController_MSIGPU.cpp and MSIGPUControllerDetect.cpp, and
// pci_ids/pci_ids.h (GPL-2.0-or-later) at commit d4798c12, https://gitlab.com/CalcProgrammer1/OpenRGB
// Copyright (C) OpenRGB contributors, see the OpenRGB git history
// (these files are "part of the OpenRGB project" and carry no author line).
//
// HueDeck first-party plugin: the RGB controller of MSI graphics cards at I2C address 0x68 on
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

/* global device, bus, shutdownColor */

export function Name() { return "MSI GPU"; }
export function Publisher() { return "HueDeck"; }
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
      description: "Colour shown after HueDeck stops when the card's own state could not be read back",
      type: "color",
      default: "#000000",
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
// HueDeck extensions (docs/host-api/smbus.md): the only address Scan() may look at, on NVIDIA buses.
export function SmbusAddresses() { return [ADDRESS]; }
export function SmbusBuses() { return ["nvidia"]; }
// Registers the host never lets this plugin write, except inside SaveToDevice(request):
// MSI_GPU_REG_SAVE.
export function SmbusDeniedRegisters() { return [REG_SAVE]; }
// HueDeck extensions (docs/host-api/save-shutdown.md): what the card can store, and the exit modes
// Shutdown(SystemSuspending, mode) carries out.
export function SaveCapabilities() { return { static: true, effects: ["spectrum", "breathing"] }; }
export function ShutdownModes() { return ["restore", "keep", "off"]; }

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
let lastColor = null;
let failureLogged = false;

function write(reg, value) {
  if (reg === REG_SAVE) {
    return;
  }
  const status = bus.WriteByte(reg, value & 0xFF);
  if (status < 0 && !failureLogged) {
    failureLogged = true;
    device.log(`MSI GPU: WriteByte(0x${reg.toString(16)}) failed with ${status} (further failures are not logged)`);
  }
}

function readStartupState() {
  const values = {};
  for (const reg of STARTUP_REGS) {
    const v = bus.ReadByte(reg);
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

export function Initialize() {
  const gpu = GPUS.find((g) => matches(bus, g));
  if (gpu) {
    device.setName(gpu.Name);
  }
  saved = readStartupState();
  // RGBController_MSIGPU::DeviceUpdateMode() for "Direct": brightness, then the mode.
  write(REG_BRIGHTNESS, BRIGHTNESS_FULL);
  write(REG_MODE, MODE_STATIC);
  lastColor = null;
}

// RGBController_MSIGPU::DeviceUpdateLEDs(): register 0x26 cleared, then SetRGB1.
function setColor(r, g, b) {
  write(REG_UNKNOWN, 0x00);
  write(REG_R1, r);
  write(REG_G1, g);
  write(REG_B1, b);
}

export function Render() {
  const [r, g, b] = device.color(0, 0);
  if (lastColor && lastColor[0] === r && lastColor[1] === g && lastColor[2] === b) {
    return;
  }
  setColor(r, g, b);
  lastColor = [r, g, b];
}

function hexColor(text) {
  const m = /^#?([0-9a-f]{6})$/i.exec(String(text ?? ""));
  const v = m ? parseInt(m[1], 16) : 0;
  return [(v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF];
}

// "Save to devices" (HueDeck extension): store the frame's colour, or a built-in effect, in the
// card's flash. As OpenRGB does it: DeviceUpdateMode() (brightness, the speed for modes that have
// one, the mode), DeviceUpdateLEDs() (0x26, then colour 1) and DeviceSaveMode() (0x3F <- 1). The
// card then goes back to Direct (static at full brightness) for the live effect, and a later
// Shutdown(false, "restore") puts back what was stored. A preview only describes it.
export function SaveToDevice(request) {
  const effect = request && request.effect ? String(request.effect) : null;
  const plan = SAVE_MODES[effect ?? "static"];
  if (!plan) {
    return { unsupported: `the card has no "${effect}" effect HueDeck can store` };
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
  write(REG_BRIGHTNESS, BRIGHTNESS_FULL);
  if (plan.speed) {
    write(REG_SPEED, SPEED_MID);
  }
  write(REG_MODE, plan.mode);
  if (plan.color) {
    setColor(r, g, b);
  }
  commitToFlash();
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

// Shutdown(SystemSuspending, mode), mode a HueDeck extension:
//   "restore" (and SignalRGB, which passes no mode): the card as it was at start (or as last
//     stored by SaveToDevice): colours, speed and brightness back, the mode last (it applies
//     them). When the startup state could not be read, the shutdown colour in static mode.
//   "keep": the last colour HueDeck showed, in static mode (not stored: gone at the next boot).
//   "off": static black (not stored).
export function Shutdown(SystemSuspending, mode) {
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
  } else {
    const [r, g, b] = hexColor(typeof shutdownColor === "string" ? shutdownColor : "#000000");
    setColor(r, g, b);
    write(REG_MODE, MODE_STATIC);
  }
  lastColor = null;
}
