// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the EVision V2 keyboard 320F:5041 (BYGG CSB ICL01), per-key colours in the
// keyboard's direct mode.
// Adapted from OpenRGB Controllers/EVisionKeyboardController/EVisionV2KeyboardController/
// EVisionV2KeyboardControllerDetect.cpp, EVisionV2KeyboardController.cpp,
// EVisionV2KeyboardController.h and RGBController_EVisionV2Keyboard.cpp (GPL-2.0-or-later) at
// commit d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Le Philousophe (25 Dec 2022)
//
// The protocol is in evision-v2.js, the key layout in layouts.js; protocol notes in
// docs/hardware/evision-keyboard.md. The Endorfy Omnis, the other V2 keyboard OpenRGB knows,
// has its own vendor id and so its own module, endorfy-omnis.js.
//
// Not driven: the older EVision keyboards OpenRGB handles with its first EVision controller
// (0C45:5004, 5104, 5204, 652F, 7672, 7698, 8520 and 320F:5000, 502A, 505B, 5064, 5078, 5084).
// OpenRGB knows no direct mode for them: it writes per-key colours into the keyboard's stored
// custom colour table, which the keyboard saves, on every update. See the hardware notes.

/* global device */

import { createKeyboard, validate } from "./evision-v2.js";
import { EVISION_V2_LAYOUT as LAYOUT } from "./layouts.js";

const keyboard = createKeyboard(LAYOUT);

export function Name() { return "EVision Keyboard 320F:5041"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/evision-keyboard"; }
export function VendorId() { return 0x320F; }
// BYGG_CSB_ICL01_PID (untested).
export function ProductId() { return [0x5041]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
export function Size() { return [LAYOUT.width, LAYOUT.height]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LAYOUT.leds.map((led) => led[0]); }
export function LedPositions() { return LAYOUT.leds.map((led) => [led[1], led[2]]); }
export function ImageUrl() { return ""; }

export function Validate(endpoint) {
  return validate(endpoint);
}

export function Initialize() {
  keyboard.initialize();
}

export function Render() {
  keyboard.render();
}

// Exit modes (Lightloom extension, docs/host-api/save-shutdown.md): only "restore". The keyboard
// does not hold direct colours without the host refreshing them (see evision-v2.js, shutdown()),
// and the only lights-off or static colour it keeps are its stored modes, which Lightloom never
// writes; so "keep" and "off" end as "restore" does, and the dashboard says so.
export function ShutdownModes() { return ["restore"]; }

export function Shutdown(SystemSuspending) {
  keyboard.shutdown(SystemSuspending);
}
