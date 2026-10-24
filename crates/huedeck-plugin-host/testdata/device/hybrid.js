// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic hybrid plugin for the raw USB session tests: HID reports on
// interface 0 plus bulk transfers on the WinUSB interface of the same device,
// and the run-time switch device.setProtocol("hid" | "hybrid") the NZXT
// Kraken LCD plugins use.

export function Name() { return "Hybrid probe"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x9999; }
export function Type() { return "Hybrid"; }
export function Size() { return [1, 1]; }

/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }

export function Initialize() {
  device.write([0x00, 0x01], 2);
  const a = device.bulk_transfer(0x02, [0x05, 0x06], 2);
  device.setProtocol("hid");
  const b = device.bulk_transfer(0x02, [0x07], 1);
  device.setProtocol("hybrid");
  const c = device.bulk_transfer(0x02, [0x08], 1);
  device.log(`protocol ${a} ${b} ${c}`);
}

export function Render() {
  device.write([0x00, 0x02], 2);
}

export function Shutdown() {}
