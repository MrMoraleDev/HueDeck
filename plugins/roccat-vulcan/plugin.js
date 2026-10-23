// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: Roccat Vulcan family keyboards (vendor 1E7D): Vulcan 100 / 120-Series Aimo,
// Vulcan TKL, Vulcan Pro, Vulcan TKL Pro, Vulcan II, Vulcan II Max, Vulcan II Mini, Pyro, Magma and
// Magma Mini, per-key colours in direct mode. The Turtle Beach branded Vulcan II models (vendor
// 10F5) are plugins/roccat-vulcan-turtle-beach/, with the same vulcan.js and layouts.js.
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
// docs/hardware/roccat-vulcan.md. Untested on hardware.

/* global device */

import { createVulcan, validate, ROCCAT_VID } from "./vulcan.js";

const keyboard = createVulcan(ROCCAT_VID);

export function Name() { return "Roccat Vulcan"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/roccat-vulcan"; }
export function VendorId() { return ROCCAT_VID; }
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
// Roccat's / Turtle Beach's own software drives the lights while it runs; Lightloom reports it.
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
