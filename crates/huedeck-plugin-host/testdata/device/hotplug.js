// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic device plugin for the device-manager tests: one output report
// per Render(), so a session's writes show whether its device is reachable.

export function Name() { return "Hotplug probe"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x5555; }
export function Size() { return [1, 1]; }

/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
export function Render() { device.write([0x00, 0x01], 2); }
export function Shutdown() {}
