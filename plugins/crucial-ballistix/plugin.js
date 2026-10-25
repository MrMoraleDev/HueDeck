// SPDX-License-Identifier: GPL-3.0-or-later
// Adapted from OpenRGB Controllers/CrucialController/CrucialController.cpp, CrucialController.h,
// CrucialControllerDetect.cpp and RGBController_Crucial.cpp, and pci_ids/pci_ids.h
// (GPL-2.0-or-later) at commit d4798c12, https://gitlab.com/CalcProgrammer1/OpenRGB
// Copyright (C) 2020 Adam Honse (CalcProgrammer1)
// (pci_ids.h is "part of the OpenRGB project" with no author line: OpenRGB contributors,
// see the OpenRGB git history).
//
// Lightloom first-party plugin: Crucial Ballistix DDR4 RGB modules (8 LEDs per module) on the
// chipset SMBus (AMD FCH "PIIX4" or the Intel SMBus controllers OpenRGB lists for DRAM). The
// controller has an indexed register file: Write Word to command 0x00 sets the 16-bit register
// pointer (byte-swapped), Read Byte 0x81 / Write Byte 0x01 / Block Write 0x03 read and write
// through it. Driven in OpenRGB's "Direct" mode: static effect mode once, then the three colour
// planes per frame.
//
// Protocol notes and sources: docs/hardware/crucial-ballistix.md. Status: tested on 2x BL8G32C16U4WL (2026-10-22).
//
// Safety: only the addresses of SmbusAddresses(); the SPD EEPROM (0x50-0x57) and its
// write-protect addresses (0x30-0x37) are never touched; the controller has no save/EEPROM
// register and none is written. Scan() moves modules still at 0x27 to a free address, as
// OpenRGB does (until the next power cycle), but only once 0x27 has answered the controller's
// identification registers (OpenRGB remaps whatever answers there). Every register access (the
// pointer write and the data transfer after it) runs under TransactionLock, so no other session
// or program moves the pointer in between; when the bus cannot be held, nothing is sent.

/* global device, bus, shutdownColor */

export function Name() { return "Crucial Ballistix DRAM"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/crucial-ballistix"; }
export function Type() { return "SMBUS"; }
export function DeviceType() { return "ram"; }
export function Size() { return [8, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LEDS.map((i) => `DRAM LED ${i}`); }
export function LedPositions() { return LEDS.map((i) => [i, 0]); }
export function ImageUrl() { return ""; }
export function ControllableParameters() {
  return [
    {
      property: "shutdownColor",
      group: "lighting",
      label: "Shutdown Color",
      description: "Colour the modules keep (static mode) after Lightloom stops",
      type: "color",
      default: "#000000",
    },
  ];
}

const LED_COUNT = 8;
const LEDS = Array.from({ length: LED_COUNT }, (_, i) => i);

// Where Crucial modules answer, in OpenRGB's order (CrucialControllerDetect.cpp crucial_addresses).
const ADDRESSES = [0x39, 0x3A, 0x3B, 0x3C, 0x20, 0x21, 0x22, 0x23];
// A module not yet given an address answers here; Scan() moves it.
const REMAP_ADDRESS = 0x27;
const REMAP_SLOTS = 4;

// Lightloom extensions (docs/host-api/smbus.md): every address Scan() may touch, on chipset buses.
export function SmbusAddresses() { return [0x20, 0x21, 0x22, 0x23, REMAP_ADDRESS, 0x39, 0x3A, 0x3B, 0x3C]; }
export function SmbusBuses() { return ["amd", "intel"]; }
// Lightloom extension (docs/host-api/smbus.md, SPD check): Scan() runs only when the SPD of a
// DIMM names one of these module manufacturers (JEP106 bank, id). Crucial Ballistix modules
// carry Crucial Technology (bank 6, 0x9B) or, on some batches, Micron Technology (bank 1,
// 0x2C) as module manufacturer: docs/hardware/crucial-ballistix.md.
export function SpdManufacturers() {
  return [
    { bank: 1, id: 0x2C, name: "Micron Technology" },
    { bank: 6, id: 0x9B, name: "Crucial Technology" },
  ];
}
// Lightloom extension: inside TransactionLock the host may queue this plugin's writes and send them
// as one batch (one I/O broker request per locked sequence); TransactionUnlock() then answers
// whether they all went through (sendDirect() below falls back on it).
export function SmbusBatchWrites() { return true; }

// IF_DRAM_SMBUS (pci_ids.h): the chipset SMBus controllers OpenRGB looks for DRAM on.
const AMD_VEN = 0x1022;
const INTEL_VEN = 0x8086;
const DRAM_SMBUS = [
  [AMD_VEN, 0x790B], // AMD_FCH_SMBUS_DEV
  [INTEL_VEN, 0x3A30], // ICH10
  [INTEL_VEN, 0xA123], // Sunrise Point-H
  [INTEL_VEN, 0x2085], // Skylake-X PCU
  [INTEL_VEN, 0xA2A3], // Kaby Lake-H
  [INTEL_VEN, 0xA323], // Cannon Lake-H
  [INTEL_VEN, 0x06A3], // Comet Lake-H
  [INTEL_VEN, 0xA3A3], // Comet Lake-V
  [INTEL_VEN, 0x43A3], // Tiger Lake
  [INTEL_VEN, 0x7AA3], // Alder Lake
  [INTEL_VEN, 0x7A23], // Raptor Lake
  [INTEL_VEN, 0x7F23], // Arrow Lake
];

// Indexed register file commands (CrucialController.cpp).
const CMD_POINTER = 0x00;
const CMD_WRITE = 0x01;
const CMD_BLOCK = 0x03;
const CMD_READ = 0x81;

// Registers (CrucialController.h / .cpp).
const REG_DEVICE_VERSION = 0x1000;
const REG_MICRON_CHECK_1 = 0x1025;
const REG_MICRON_CHECK_2 = 0x1030;
const REG_MODE = 0x820F;
const REG_EFFECT_LED = 0x82E9;
const REG_EFFECT_ARG1 = 0x82EE;
const REG_EFFECT_ARG2 = 0x82EF;
const REG_EFFECT_APPLY = 0x82F0;
const REG_DIRECT_RED = 0x8300;
const REG_DIRECT_GREEN = 0x8340;
const REG_DIRECT_BLUE = 0x8380;

const MODE_STATIC = 0xCF;
const EFFECT_SPEED = 0x10;
const APPLY_MODE = 0x84;
const APPLY_COLOR = 0x01;
const APPLY_REMAP = 0xF0;

// Transfers on a free bus (Scan) or the device's fixed bus, one shape for the helpers below.
// Lightloom's TransactionLock() answers whether the bus is held; SignalRGB's returns nothing.
function lockOps(b) {
  return {
    lock: () => b.TransactionLock() !== false,
    unlock: () => b.TransactionUnlock(),
    holding: () => b.HasTransactionLock() === true,
  };
}

function freeOps(b, addr) {
  return {
    ...lockOps(b),
    readByte: (reg) => b.ReadByte(addr, reg),
    writeByte: (reg, value) => b.WriteByte(addr, reg, value),
    writeWord: (reg, word) => b.WriteWord(addr, reg, word),
    writeBlock: (reg, data) => b.WriteBlock(addr, reg, data.length, data),
  };
}

function fixedOps(b) {
  return {
    ...lockOps(b),
    readByte: (reg) => b.ReadByte(reg),
    writeByte: (reg, value) => b.WriteByte(reg, value),
    writeWord: (reg, word) => b.WriteWord(reg, word),
    writeBlock: (reg, data) => b.WriteBlock(reg, data.length, data),
  };
}

// Run `fn` with the bus held: inside an outer hold it just runs; otherwise the bus is locked
// around it, and when it cannot be locked nothing is sent and `failed` is returned.
function held(io, fn, failed) {
  if (io.holding()) {
    return fn();
  }
  if (!io.lock()) {
    return failed;
  }
  try {
    return fn();
  } finally {
    io.unlock();
  }
}

// The register pointer goes out byte-swapped: ((reg << 8) & 0xFF00) | ((reg >> 8) & 0x00FF).
function swap16(reg) {
  return ((reg << 8) & 0xFF00) | ((reg >> 8) & 0x00FF);
}

// CrucialRegisterRead / CrucialRegisterWrite / CrucialRegisterWriteBlock, each pointer write and
// its data transfer under one hold. A failed hold reads as -1 / writes nothing.
function regRead(io, reg) {
  return held(io, () => {
    io.writeWord(CMD_POINTER, swap16(reg));
    return io.readByte(CMD_READ);
  }, -1);
}

function regWrite(io, reg, value) {
  held(io, () => {
    io.writeWord(CMD_POINTER, swap16(reg));
    io.writeByte(CMD_WRITE, value & 0xFF);
  });
}

function regWriteBlock(io, reg, data) {
  held(io, () => {
    io.writeWord(CMD_POINTER, swap16(reg));
    if (io.writeBlock(CMD_BLOCK, data) < 0) {
      // As OpenRGB: fall back to byte writes when the block write fails.
      for (const b of data) {
        io.writeByte(CMD_WRITE, b);
      }
    }
  });
}

// 16 registers from `reg` as a C string (up to the first NUL), under one hold.
function regString(io, reg) {
  const bytes = held(io, () => Array.from({ length: 16 }, (_, i) => regRead(io, reg + i)), []);
  const end = bytes.indexOf(0);
  return String.fromCharCode(...(end < 0 ? bytes : bytes.slice(0, end)).map((v) => v & 0xFF));
}

// The controller's identification registers: 0xA0..0xAF read 0..15. Reads only. Every register
// is read even after a mismatch, as TestForCrucialController does.
function identifies(b, addr) {
  let pass = true;
  for (let reg = 0xA0; reg < 0xB0; reg++) {
    if (b.ReadByte(addr, reg) !== reg - 0xA0) {
      pass = false;
    }
  }
  return pass;
}

function isDramBus(b) {
  return b.IsSystemBus() && DRAM_SMBUS.some(([ven, dev]) => b.Vendor() === ven && b.Product() === dev);
}

// DetectCrucialControllers: modules still at 0x27 get the next free address of ADDRESSES,
// one per slot. The list index carries over between slots, as in the C++. Lightloom writes to
// 0x27 only once it has answered the identification registers; anything else there is left
// alone and the remap stops.
function remapModules(b) {
  let index = -1;
  for (let slot = 0; slot < REMAP_SLOTS; slot++) {
    if (b.ReadByteWithoutRegister(REMAP_ADDRESS) < 0) {
      break;
    }
    let res;
    do {
      index++;
      if (index < ADDRESSES.length) {
        res = b.ReadByteWithoutRegister(ADDRESSES[index]);
      } else {
        break;
      }
    } while (res >= 0);
    if (index < ADDRESSES.length) {
      if (!identifies(b, REMAP_ADDRESS)) {
        b.log("Crucial: the device at 0x27 is not a Crucial controller: not remapped");
        break;
      }
      const target = `0x${ADDRESSES[index].toString(16)}`;
      if (dryRun(b)) {
        // The remap writes would only be recorded: the module stays at 0x27, so stop here.
        b.log(`Crucial: a module at 0x27 needs an address: a live scan moves it (slot ${slot}) to ${target} (dry run: not sent)`);
        break;
      }
      b.log(`Crucial: moving the module at 0x27 (slot ${slot}) to ${target} (until the next power cycle)`);
      // 0x82EE = slot, 0x82EF = the new 8-bit address, 0x82F0 = 0xF0 applies it.
      const io = freeOps(b, REMAP_ADDRESS);
      held(io, () => {
        regWrite(io, REG_EFFECT_ARG1, slot);
        regWrite(io, REG_EFFECT_ARG2, ADDRESSES[index] << 1);
        regWrite(io, REG_EFFECT_APPLY, APPLY_REMAP);
      });
    }
    b.pause(1);
  }
}

// Lightloom's --dry-run (bus.IsDryRun(), Lightloom extension): writes are recorded, never sent.
function dryRun(b) {
  return typeof b.IsDryRun === "function" && b.IsDryRun() === true;
}

// TestForCrucialController: something answers, registers 0xA0..0xAF read 0..15, and "Micron"
// is at 0x1025 or 0x1030. The "Micron" string sits behind the register pointer, which takes a
// write: in a dry run that write is not sent, so the module is reported on the identification
// registers alone (reads only), and the log says so.
function isCrucial(b, addr) {
  if (b.ReadByteWithoutRegister(addr) < 0) {
    return false;
  }
  if (!identifies(b, addr)) {
    return false;
  }
  if (dryRun(b)) {
    b.log(`Crucial: 0x${addr.toString(16)} answers the identification registers (dry run: the "Micron" check needs a pointer write, not sent)`);
    return true;
  }
  const io = freeOps(b, addr);
  return regString(io, REG_MICRON_CHECK_1) === "Micron" || regString(io, REG_MICRON_CHECK_2) === "Micron";
}

export function Scan(freeBus) {
  if (!isDramBus(freeBus)) {
    return [];
  }
  remapModules(freeBus);
  const found = [];
  for (const addr of ADDRESSES) {
    if (isCrucial(freeBus, addr)) {
      found.push(addr);
    }
    freeBus.pause(1);
  }
  return found;
}

let lastColors = null;

function ledColors() {
  return LEDS.map((i) => device.color(i, 0));
}

// SendEffectMode(mode, speed), under one hold.
function sendEffectMode(io, mode) {
  held(io, () => {
    regWrite(io, REG_MODE, mode);
    regWrite(io, REG_EFFECT_ARG1, 0x00);
    regWrite(io, REG_EFFECT_ARG2, EFFECT_SPEED);
    regWrite(io, REG_EFFECT_APPLY, APPLY_MODE);
  });
}

// SetAllColorsEffect(colors): SendEffectColor per LED, each under one hold.
function setAllColorsEffect(io, colors) {
  colors.forEach(([r, g, b], led) => {
    held(io, () => {
      regWrite(io, REG_EFFECT_LED, 1 << led);
      regWrite(io, 0x82EA, 0x00);
      regWrite(io, 0x82EB, 0x00);
      regWrite(io, 0x82EC, 0x00);
      regWrite(io, 0x82ED, r);
      regWrite(io, REG_EFFECT_ARG1, g);
      regWrite(io, REG_EFFECT_ARG2, b);
      regWrite(io, REG_EFFECT_APPLY, APPLY_COLOR);
    });
  });
}

export function Initialize() {
  const io = fixedOps(bus);
  const version = regString(io, REG_DEVICE_VERSION).replace(/[^\x20-\x7E]/g, "").trim();
  const where = `0x${bus.GetAddress().toString(16).toUpperCase()}`;
  device.setName(version ? `Crucial DRAM ${where} (${version})` : `Crucial DRAM ${where}`);
  // RGBController_Crucial::DeviceUpdateMode() for "Direct": static mode, then the effect colours.
  sendEffectMode(io, MODE_STATIC);
  setAllColorsEffect(io, ledColors());
  lastColors = null;
}

const PLANES = [REG_DIRECT_RED, REG_DIRECT_GREEN, REG_DIRECT_BLUE];

// SendDirectColors(): the red, green and blue planes, one Block Write each. Lightloom holds the
// bus for the whole frame so no other program's transfer lands between pointer and data; when
// the bus cannot be held the frame is not sent (false) and goes out with the next Render().
// With SmbusBatchWrites() Lightloom sends the frame as one batch and TransactionUnlock() answers a
// negative status when a write failed; then, as OpenRGB does for a failed block write, the planes
// go out again byte by byte.
function sendDirect(colors) {
  const io = fixedOps(bus);
  if (!io.lock()) {
    return false;
  }
  let status;
  try {
    PLANES.forEach((reg, plane) => regWriteBlock(io, reg, colors.map((c) => c[plane])));
  } finally {
    status = io.unlock();
  }
  if (typeof status !== "number" || status >= 0) {
    return true;
  }
  return held(io, () => {
    PLANES.forEach((reg, plane) => {
      io.writeWord(CMD_POINTER, swap16(reg));
      for (const c of colors) {
        io.writeByte(CMD_WRITE, c[plane]);
      }
    });
    return true;
  }, false);
}

export function Render() {
  const colors = ledColors();
  if (lastColors && colors.every((c, i) => c[0] === lastColors[i][0] && c[1] === lastColors[i][1] && c[2] === lastColors[i][2])) {
    return;
  }
  if (sendDirect(colors)) {
    lastColors = colors;
  }
}

function hexColor(text) {
  const m = /^#?([0-9a-f]{6})$/i.exec(String(text ?? ""));
  const v = m ? parseInt(m[1], 16) : 0;
  return [(v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF];
}

// Lightloom extension (docs/host-api/save-shutdown.md): the exit modes Shutdown() carries out.
export function ShutdownModes() { return ["restore", "keep", "off"]; }

// Leave the modules in static mode, in DeviceUpdateLEDs() order for "Static": the effect colours,
// then the mode that applies them. Which colours:
//   "restore" (and SignalRGB's call without a mode): the shutdown colour (default black). The
//     modules' own lighting cannot be given back: OpenRGB reads no state from the controller (its
//     mode is CRUCIAL_MODE_UNKNOWN until set), the effect colours go in through a write-only
//     mailbox (0x82E9..0x82F0), and there is no save register, so nothing was read at start to put
//     back. Their own effect returns at the next power cycle.
//   "keep": the last frame, one effect colour per LED (volatile: static mode stores nothing).
//   "off": black.
//   Suspend: black, whatever the mode.
export function Shutdown(suspending, mode) {
  const io = fixedOps(bus);
  let colors;
  if (!suspending && mode === "keep") {
    colors = ledColors().map(([r, g, b]) => [r & 0xFF, g & 0xFF, b & 0xFF]);
  } else {
    const color = suspending || mode === "off"
      ? [0, 0, 0]
      : hexColor(typeof shutdownColor === "string" ? shutdownColor : "#000000");
    colors = LEDS.map(() => color);
  }
  setAllColorsEffect(io, colors);
  sendEffectMode(io, MODE_STATIC);
  lastColors = null;
}
