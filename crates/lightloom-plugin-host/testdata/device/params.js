// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Synthetic device plugin for the device-session tests: the change hooks
// write an output report, so a test sees when and with what they ran.

export function Name() { return "Parameter probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4444; }
export function Size() { return [1, 1]; }
export function ControllableParameters() {
  return [{ property: "mode", group: "", label: "Mode", type: "combobox", values: ["a", "b"], default: "a" }];
}

/* global device, mode */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
export function Render() {}
export function Shutdown() {}

export function onmodeChanged() {
  device.write([0x00, mode === "b" ? 0x02 : 0x01], 2);
}

export function onBrightnessChanged() {
  device.write([0x00, 0x10 + device.getBrightness() / 10], 2);
}
