// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: HyperX keyboards with the HP vendor id 0x03F0 (Alloy Elite 2, Alloy Origins,
// Alloy Origins 60, Alloy Origins 65, Eve 1800, Origins 2 65), per LED. The Kingston ids (0x0951)
// are plugins/hyperx-keyboard, which shares ./family.js and ./layouts.js byte for byte.
// Adapted from OpenRGB Controllers/HyperXKeyboardController/ (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB; the
// adapted files and their authors are listed in ./family.js and docs/hardware/hyperx-keyboard.md.
//
// Held back: Alloy Origins Core (HP) (03F0:098F): OpenRGB builds its LED layout at run time with
// its KeyboardLayoutManager from a layout-variant query; not ported in this batch (see the notes).

/* global device */

import { PRODUCTS, productFor, validate, start, render, shutdown } from "./family.js";

const VENDOR_ID = 0x03F0;
const OWN = PRODUCTS.filter((p) => p.vid === VENDOR_ID);

export function Name() { return "HyperX Keyboard (HP)"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/hyperx-keyboard-hp"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return OWN.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
// Placeholders; Initialize() sets the product's own name and LED grid.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// HyperX NGENUITY drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["NGenuity2.exe", "HyperX NGENUITY.exe"]; }

let session = null;

export function Validate(endpoint) {
  return validate(productFor(VENDOR_ID, Number(device.productId())), endpoint);
}

export function Initialize() {
  const product = productFor(VENDOR_ID, Number(device.productId()));
  if (!product) {
    device.log(`HyperX keyboard: product id 0x${(Number(device.productId()) & 0xFFFF).toString(16)} is not one this plugin knows.`);
  }
  session = start(device, product);
}

export function Render() {
  render(device, session);
}

export function Shutdown(SystemSuspending) {
  shutdown(device, session);
}
