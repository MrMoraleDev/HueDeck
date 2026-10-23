// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: Wooting keyboards with the Wooting vendor id 0x31E3 (One, Two, Two Lekker
// Edition, Two HE, 60HE / 60HE+ / 60HEv2, 80HE, UwU RGB, every gamepad-emulation product id), per
// key, over firmware V2 (usage page 0x1337) or V3 (usage page 0xFF55) reports. The legacy-firmware
// ids on the Atmel vendor id (0x03EB) are plugins/wooting-legacy, which shares ./family.js and
// ./layouts.js byte for byte.
// Adapted from OpenRGB Controllers/WootingKeyboardController/ (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB; the
// adapted files and their authors are listed in ./family.js and docs/hardware/wooting.md.

/* global device */

import { PRODUCTS, productFor, validate, start, render, shutdown } from "./family.js";

const VENDOR_ID = 0x31E3;
const OWN = PRODUCTS.filter((p) => p.vid === VENDOR_ID);

export function Name() { return "Wooting"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/wooting"; }
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
// Wootility drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["Wootility.exe"]; }

let session = null;

export function Validate(endpoint) {
  return validate(productFor(VENDOR_ID, Number(device.productId())), endpoint);
}

export function Initialize() {
  const product = productFor(VENDOR_ID, Number(device.productId()));
  if (!product) {
    device.log(`Wooting: product id 0x${(Number(device.productId()) & 0xFFFF).toString(16)} is not one this plugin knows.`);
  }
  session = start(device, product);
}

export function Render() {
  render(device, session);
}

export function Shutdown(SystemSuspending) {
  shutdown(device, session);
}
