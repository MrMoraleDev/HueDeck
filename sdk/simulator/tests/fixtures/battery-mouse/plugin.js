// SPDX-License-Identifier: GPL-3.0-or-later
// Simulator fixture: a made-up wireless mouse (1209:0008) that reports its battery
// (`device.addFeature("battery")`) on the first frame and lights one LED.

/* global device, battery */

export function Name() { return "Battery Mouse"; }
export function VendorId() { return 0x1209; }
export function ProductId() { return [0x0008]; }
export function Size() { return [1, 1]; }
export function LedNames() { return ["Logo"]; }
export function LedPositions() { return [[0, 0]]; }
export function Validate(endpoint) { return endpoint.interface === 0; }

let frames = 0;

export function Initialize() {
  device.addFeature("battery");
}

export function Render() {
  if (frames++ === 0) {
    battery.setBatteryLevel(42);
    battery.setBatteryState(battery.charging);
  }
  device.send_report([0x05, ...device.color(0, 0)], 4);
}

export function Shutdown() {
  device.send_report([0x05, 0, 0, 0], 4);
}
