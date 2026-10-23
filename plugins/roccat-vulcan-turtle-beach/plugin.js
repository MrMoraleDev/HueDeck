// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: Turtle Beach branded Roccat Vulcan keyboards (vendor 10F5): Vulcan II, Vulcan II
// TKL and Vulcan II TKL Pro, per-key colours in direct mode. The Roccat branded models (vendor
// 1E7D) are plugins/roccat-vulcan/; vulcan.js and layouts.js are the same files in both.
// Adapted from OpenRGB Controllers/RoccatController/RoccatVulcanKeyboardController/
// RoccatVulcanKeyboardController.cpp, RoccatVulcanKeyboardController.h,
// RGBController_RoccatVulcanKeyboard.cpp, RoccatVulcanKeyboardLayouts.h and
// Controllers/RoccatController/RoccatControllerDetect.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Mola19 (17 Dec 2021, RoccatVulcanKeyboardController and
// RGBController_RoccatVulcanKeyboard; 29 Sep 2022, RoccatVulcanKeyboardLayouts.h);
// RoccatControllerDetect.cpp names no author.
//
// The protocol is in vulcan.js, the key layouts in layouts.js; protocol notes in
// docs/hardware/roccat-vulcan-turtle-beach.md and docs/hardware/roccat-vulcan.md. Untested on hardware.

/* global device */

import { createVulcan, validate, TURTLE_BEACH_VID } from "./vulcan.js";

const keyboard = createVulcan(TURTLE_BEACH_VID);

export function Name() { return "Turtle Beach Vulcan"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/roccat-vulcan-turtle-beach"; }
export function VendorId() { return TURTLE_BEACH_VID; }
export function ProductId() { return keyboard.products.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
// Placeholders; Initialize() sets the product's layout once the keyboard reported it.
export function Size() { return [24, 6]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Esc"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// Roccat's / Turtle Beach's own software drives the lights while it runs; HueDeck reports it.
export function ConflictingProcesses() {
  return ["ROCCAT_Swarm.exe", "ROCCAT_Swarm_Monitor.exe", "Turtle Beach Swarm II.exe"];
}

// DetectRoccatVulcanKeyboardControllers opens two collections: the control collection (interface
// 1) and the LED collection (interface 3, or 4), matched by interface and usage page.
export function Validate(endpoint) {
  return validate(device.productId(), endpoint);
}

export function Initialize() {
  keyboard.initialize();
}

export function Render() {
  keyboard.render();
}

export function Shutdown(SystemSuspending) {
  keyboard.shutdown();
}
