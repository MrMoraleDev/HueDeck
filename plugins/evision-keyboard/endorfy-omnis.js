// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the Endorfy Omnis keyboard (3299:0012), an EVision V2 keyboard, per-key
// colours in the keyboard's direct mode.
// Adapted from OpenRGB Controllers/EVisionKeyboardController/EVisionV2KeyboardController/
// EVisionV2KeyboardControllerDetect.cpp, EVisionV2KeyboardController.cpp,
// EVisionV2KeyboardController.h and RGBController_EVisionV2Keyboard.cpp (GPL-2.0-or-later) at
// commit d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Le Philousophe (25 Dec 2022)
//
// Same protocol as plugin.js (evision-v2.js) with the Endorfy key layout (layouts.js). A
// separate module because a plugin module has one vendor id, and the Omnis does not use
// EVision's 0x320F but 0x3299 (SPCGEAR_VID in OpenRGB). Protocol notes:
// docs/hardware/evision-keyboard.md.

/* global device */

import { createKeyboard, validate } from "./evision-v2.js";
import { ENDORFY_LAYOUT as LAYOUT } from "./layouts.js";

const keyboard = createKeyboard(LAYOUT);

export function Name() { return "Endorfy Omnis"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/evision-keyboard"; }
// SPCGEAR_VID.
export function VendorId() { return 0x3299; }
// ENDORFY_OMNIS_PID (untested).
export function ProductId() { return [0x0012]; }
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

export function Shutdown(SystemSuspending) {
  keyboard.shutdown(SystemSuspending);
}
