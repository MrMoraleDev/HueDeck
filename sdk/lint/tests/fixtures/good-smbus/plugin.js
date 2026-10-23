// SPDX-License-Identifier: GPL-3.0-or-later
// Lint and simulator fixture: a made-up one-LED RAM controller at 0x39 (register 0x10 = R,
// 0x11 = G, 0x12 = B, 0x20 = apply; register 0x00 reads 0xA5 when the controller is there).

/* global bus, device */

const ADDRESS = 0x39;

export function Name() { return "Good SMBus"; }
export function Version() { return "0.1.0"; }
export function Type() { return "SMBUS"; }
export function DeviceType() { return "ram"; }
export function Size() { return [1, 1]; }
export function LedNames() { return ["DIMM"]; }
export function LedPositions() { return [[0, 0]]; }
export function SmbusAddresses() { return [ADDRESS]; }
export function SmbusBuses() { return ["amd", "intel"]; }
export function SmbusDeniedRegisters() { return [0xFF]; }
export function SpdManufacturers() { return [{ bank: 1, id: 44, name: "Micron" }]; }

export function Scan(freeBus) {
  if (!freeBus.IsSystemBus()) return [];
  return freeBus.ReadByte(ADDRESS, 0x00) === 0xA5 ? [ADDRESS] : [];
}

export function Initialize() { device.setName("Good SMBus DIMM"); }
export function Render() { send(device.color(0, 0)); }
export function Shutdown() { send([0, 0, 0]); }

function send([r, g, b]) {
  if (bus.TransactionLock() === false) return;
  bus.WriteByte(0x10, r);
  bus.WriteByte(0x11, g);
  bus.WriteByte(0x12, b);
  bus.WriteByte(0x20, 0x01);
  bus.TransactionUnlock();
}
