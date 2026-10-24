// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from OpenRGB Controllers/SapphireGPUController/SapphireNitroGlowV3Controller/
// SapphireNitroGlowV3Controller.cpp, SapphireNitroGlowV3Controller.h (K900, 03 Feb 2021),
// RGBController_SapphireNitroGlowV3.cpp, SapphireGPUController/SapphireGPUControllerDetect.cpp,
// and pci_ids/pci_ids.h (GPL-2.0-or-later) at commit d4798c12, https://gitlab.com/CalcProgrammer1/OpenRGB
// Copyright (C) K900 and OpenRGB contributors, see the OpenRGB git history.
//
// HueDeck first-party plugin: the "Nitro Glow V3" RGB controller of Sapphire Radeon Nitro+ /
// Pure / Toxic cards at I2C address 0x28 on the GPU's own bus (AMD: ADL, line 1). One LED,
// driven in OpenRGB's "Static" mode (the custom colour): external control off, mode 0x06, then
// the colour registers 0x1A/0x1B/0x1C per frame.
//
// Protocol notes and sources: docs/hardware/sapphire-nitro-glow.md. Status: untested (no AMD
// card on the reference machine); every card stays out of `tested` in manifest.json.
//
// Safety, as in OpenRGB: lighting registers only (0x0F external control, 0x10 mode, 0x1A-0x1C
// colour), one write per register, no retries; the controller has no save-to-flash command and
// none is sent. Only address 0x28. The host renders SMBus devices at most every 33 ms.

/* global device, bus, shutdownColor */

export function Name() { return "Sapphire Nitro Glow"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/sapphire-nitro-glow"; }
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

// SAPPHIRE_NITRO_GLOW_V3_ADDR (SapphireGPUControllerDetect.cpp).
const ADDRESS = 0x28;

// Registers (SapphireNitroGlowV3Controller.h).
const REG_EXTERNAL_CONTROL = 0x0F;
const REG_MODE = 0x10;
const REG_RED = 0x1A;
const REG_GREEN = 0x1B;
const REG_BLUE = 0x1C;
// SAPPHIRE_NITRO_GLOW_V3_MODE_CUSTOM: the static colour of 0x1A-0x1C.
const MODE_CUSTOM = 0x06;

const AMD = 0x1002;
const SAPPHIRE = 0x1DA2;
// Every Nitro Glow V3 entry of SapphireGPUControllerDetect.cpp, ids from pci_ids.h. The V1
// controller (RX 470-590 and Vega Nitro+, address 0x55) has another register map and is not here.
const GPUS = [
  [0x7340, 0xE423, "Sapphire Radeon RX 5500 XT Nitro+"],
  [0x731F, 0xE409, "Sapphire Radeon RX 5700 XT Nitro+"],
  [0x731F, 0xE410, "Sapphire Radeon RX 5700 XT Nitro+"],
  [0x731F, 0x426E, "Sapphire Radeon RX 5700 XT Nitro+"],
  [0x73FF, 0xE448, "Sapphire Radeon RX 6600 XT Nitro+"],
  [0x73EF, 0xE448, "Sapphire Radeon RX 6650 XT Nitro+"],
  [0x73DF, 0xE445, "Sapphire Radeon RX 6700 XT Nitro+"],
  [0x73DF, 0xE465, "Sapphire Radeon RX 6750 XT Nitro+"],
  [0x73BF, 0xE439, "Sapphire Radeon RX 6800 Nitro+"],
  [0x73BF, 0x438E, "Sapphire Radeon RX 6800 XT Nitro+ SE"],
  [0x73BF, 0xE438, "Sapphire Radeon RX 6800 XT/6900 XT Nitro+"],
  [0x73BF, 0x440E, "Sapphire Radeon RX 6900 XT Nitro+ SE"],
  [0x73AF, 0xF440, "Sapphire Radeon RX 6900 XT Toxic"],
  [0x73BF, 0x440F, "Sapphire Radeon RX 6900 XT Toxic"],
  [0x73BF, 0xF441, "Sapphire Radeon RX 6900 XT Toxic Limited Edition"],
  [0x73A5, 0xD440, "Sapphire Radeon RX 6950 XT Toxic"],
  [0x73A5, 0x441D, "Sapphire Radeon RX 6950 XT Nitro+"],
  [0x73A5, 0x440D, "Sapphire Radeon RX 6950 XT Nitro+ Pure"],
  [0x747E, 0xF475, "Sapphire Radeon RX 7700 XT Nitro+"],
  [0x747E, 0xD475, "Sapphire Radeon RX 7800 XT Nitro+"],
  [0x744C, 0xE475, "Sapphire Radeon RX 7900 GRE Nitro+"],
  [0x744C, 0xE471, "Sapphire Radeon RX 7900 XTX Nitro+"],
  [0x7590, 0x493E, "Sapphire Radeon RX 9060 XT Pure"],
  [0x7590, 0xE493, "Sapphire Radeon RX 9060 XT Nitro+"],
  [0x7550, 0x4499, "Sapphire Radeon RX 9070 Pure"],
  [0x7550, 0xE489, "Sapphire Radeon RX 9070 XT Nitro+"],
  [0x7550, 0x4892, "Sapphire Radeon RX 9070 XT Nitro+"],
  [0x7550, 0x3490, "Sapphire Radeon RX 9070 XT Pure"],
].map(([Device, SubDevice, Name]) => ({ Vendor: AMD, Device, SubVendor: SAPPHIRE, SubDevice, Address: ADDRESS, Name }));

export function BrandGPUList() { return GPUS.map((g) => ({ ...g })); }
// HueDeck extensions (docs/host-api/smbus.md): the only address Scan() may look at, on AMD GPU buses.
export function SmbusAddresses() { return [ADDRESS]; }
export function SmbusBuses() { return ["amdgpu"]; }

// What the RGBController constructor reads, kept to restore the card on Shutdown().
const STARTUP_REGS = [REG_RED, REG_GREEN, REG_BLUE, REG_EXTERNAL_CONTROL, REG_MODE];

function matches(b, gpu) {
  return b.Vendor() === gpu.Vendor && b.Product() === gpu.Device &&
    b.SubVendor() === gpu.SubVendor && b.SubDevice() === gpu.SubDevice;
}

// Like OpenRGB's TestForSapphireGPUController(): an id match on an AMD GPU bus, then one read
// at 0x28 (OpenRGB's i2c_smbus_read_byte, which its ADL bus sends as a read at offset 0).
export function Scan(freeBus) {
  if (!freeBus.IsAMDGpuBus()) {
    return [];
  }
  const gpu = GPUS.find((g) => matches(freeBus, g));
  if (!gpu) {
    return [];
  }
  return freeBus.ReadByte(gpu.Address, 0x00) >= 0 ? [gpu.Address] : [];
}

let saved = null;
let lastColor = null;
let failureLogged = false;

function write(reg, value) {
  const status = bus.WriteByte(reg, value & 0xFF);
  if (status < 0 && !failureLogged) {
    failureLogged = true;
    device.log(`Sapphire Nitro Glow: WriteByte(0x${reg.toString(16)}) failed with ${status} (further failures are not logged)`);
  }
}

function readStartupState() {
  const values = {};
  for (const reg of STARTUP_REGS) {
    const v = bus.ReadByte(reg);
    if (v < 0) {
      device.log(`Sapphire Nitro Glow: ReadByte(0x${reg.toString(16)}) failed with ${v}; Shutdown() will set the shutdown colour`);
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
  // RGBController_SapphireNitroGlowV3::DeviceUpdateMode() for "Static": external control off,
  // then the custom-colour mode.
  write(REG_EXTERNAL_CONTROL, 0);
  write(REG_MODE, MODE_CUSTOM);
  lastColor = null;
}

// SapphireNitroGlowV3Controller::SetColor().
function setColor(r, g, b) {
  write(REG_RED, r);
  write(REG_GREEN, g);
  write(REG_BLUE, b);
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

// Leave the card as it was: the colour, then the mode (applied while external control is still
// off, as OpenRGB sets modes), then external control last (it hands the LED back to whatever
// drove it). When the startup state could not be read, show the shutdown colour in the custom
// mode instead.
export function Shutdown() {
  if (saved) {
    setColor(saved[REG_RED], saved[REG_GREEN], saved[REG_BLUE]);
    write(REG_MODE, saved[REG_MODE]);
    write(REG_EXTERNAL_CONTROL, saved[REG_EXTERNAL_CONTROL]);
  } else {
    const [r, g, b] = hexColor(typeof shutdownColor === "string" ? shutdownColor : "#000000");
    setColor(r, g, b);
    write(REG_MODE, MODE_CUSTOM);
  }
  lastColor = null;
}
