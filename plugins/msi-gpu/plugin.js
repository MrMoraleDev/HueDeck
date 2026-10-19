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
// Protocol notes and sources: docs/hardware/msi-gpu.md. Status: untested on hardware.
//
// Safety, as in OpenRGB: one write per register, no retries; register 0x3F (MSI_GPU_REG_SAVE,
// which stores the current state in the card's flash) is never written (the plugin skips it,
// and the host refuses it through SmbusDeniedRegisters()); only address 0x68. The host renders
// SMBus devices at most every 33 ms, OpenRGB's TimeToSend for this controller.

/* global device, bus, shutdownColor */

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
// Registers the host never lets this plugin write: MSI_GPU_REG_SAVE.
export function SmbusDeniedRegisters() { return [REG_SAVE]; }

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
// MSI_GPU_REG_SAVE: writes the current state to the card's flash. Never written.
const REG_SAVE = 0x3F;

const MODE_STATIC = 0x13;
// OpenRGB "Direct" brightness: MSI_GPU_BRIGHTNESS_MAX (5) * MSI_GPU_BRIGHTNESS_MULTI (20).
const BRIGHTNESS_FULL = 5 * 20;

// What the RGBController constructor reads, in its order: SetupZones() reads the three colours,
// then GetDeviceMode(), GetSpeed() and the brightness. Kept to restore the card on Shutdown().
const COLOR_REGS = [REG_R1, REG_G1, REG_B1, REG_R2, REG_G2, REG_B2, REG_R3, REG_G3, REG_B3];
const STARTUP_REGS = [...COLOR_REGS, REG_MODE, REG_SPEED, REG_BRIGHTNESS];

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

// Leave the card as it was: colours, speed and brightness back, the mode last (it applies them).
// When the startup state could not be read, show the shutdown colour in static mode instead.
export function Shutdown() {
  if (saved) {
    write(REG_UNKNOWN, 0x00);
    for (const reg of COLOR_REGS) {
      write(reg, saved[reg]);
    }
    write(REG_SPEED, saved[REG_SPEED]);
    write(REG_BRIGHTNESS, saved[REG_BRIGHTNESS]);
    write(REG_MODE, saved[REG_MODE]);
  } else {
    const [r, g, b] = hexColor(typeof shutdownColor === "string" ? shutdownColor : "#000000");
    setColor(r, g, b);
    write(REG_MODE, MODE_STATIC);
  }
  lastColor = null;
}
