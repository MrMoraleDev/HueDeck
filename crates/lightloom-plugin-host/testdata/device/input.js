// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic device plugin for the device-session tests: the `keyboard` and
// `mouse` features. Initialize reports a macro key, a button, a real key and
// a real button through them and logs what it sees of the globals.

export function Name() { return "Input probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x2223; }
export function Size() { return [1, 1]; }

/* global device, keyboard, mouse */
export function Validate(endpoint) { return endpoint.interface === 0; }

export function Initialize() {
  device.addFeature("keyboard");
  device.addFeature("mouse");
  keyboard.sendEvent({ key: "G1", keyCode: 0, released: false }, "Key Press");
  mouse.sendEvent({ buttonCode: 0, released: true, name: "DPI Up" }, "Button Press");
  keyboard.sendHid(0xaf, { released: false });
  mouse.sendHid(5, { released: true, XButton: 1 });
  let threw = "";
  try {
    keyboard.sendHid(0xaf, {});
  } catch (e) {
    threw = e.message;
  }
  device.log(`probe ${JSON.stringify({ sendKey: typeof keyboard.sendKey, threw })}`);
}

export function Render() {
  const c = device.color(0, 0);
  device.write([0x01, c[0], c[1], c[2]], 4);
}

export function Shutdown() {}
