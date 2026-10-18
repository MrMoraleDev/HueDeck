// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: the light bar of the Sony DualSense controller, connected by
// USB (054C:0CE6; the DualSense Edge 054C:0DF2 is untested).
//
// Protocol facts are written up in docs/hardware/sony-dualsense.md: the field layout of output
// report 0x02 as documented in the Linux kernel's hid-playstation driver, matched against the
// report another plugin sent to this controller (Lightloom dry-run, 2026-10-18). In short: every
// output report carries "valid" flags that say which of its fields the controller applies.
// Lightloom sets only the light bar flags, so volume, audio routing, vibration, adaptive triggers,
// the microphone LED and the player LEDs are never touched.

/* global device */

export function Name() { return "Sony DualSense"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/sony-dualsense"; }
export function VendorId() { return 0x054C; }
// DualSense, DualSense Edge (untested).
export function ProductId() { return [0x0CE6, 0x0DF2]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Light bar"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }

// Output report 0x02 over USB: the report id and 47 bytes.
const REPORT_ID = 0x02;
const REPORT_LENGTH = 48;
// Byte offsets, counting the report id as byte 0.
const VALID_FLAG1 = 2;
const VALID_FLAG2 = 39;
const LIGHTBAR_SETUP = 42;
const LIGHTBAR_RED = 45;
// valid_flag1 bit 2: apply the light bar colour.
const LIGHTBAR_CONTROL = 0x04;
// valid_flag2 bit 1: apply lightbar_setup; lightbar_setup 0x02 fades the controller's own light
// bar out so the host's colour takes over.
const LIGHTBAR_SETUP_CONTROL = 0x02;
const LIGHTBAR_LIGHT_OUT = 0x02;

export function Validate(endpoint) {
  return endpoint.interface === 3 && endpoint.usage_page === 0x0001 && endpoint.usage === 0x0005;
}

export function Initialize() {
  const report = emptyReport();
  report[VALID_FLAG2] = LIGHTBAR_SETUP_CONTROL;
  report[LIGHTBAR_SETUP] = LIGHTBAR_LIGHT_OUT;
  device.write(report, REPORT_LENGTH);
}

export function Render() {
  const [r, g, b] = device.color(0, 0);
  sendColour(r, g, b);
}

export function Shutdown() {
  sendColour(0, 0, 0);
}

function sendColour(r, g, b) {
  const report = emptyReport();
  report[VALID_FLAG1] = LIGHTBAR_CONTROL;
  report[LIGHTBAR_RED] = r & 0xFF;
  report[LIGHTBAR_RED + 1] = g & 0xFF;
  report[LIGHTBAR_RED + 2] = b & 0xFF;
  device.write(report, REPORT_LENGTH);
}

function emptyReport() {
  const report = new Array(REPORT_LENGTH).fill(0);
  report[0] = REPORT_ID;
  return report;
}
