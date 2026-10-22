// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic device plugin for the device-session tests: Render() blocks in
// host calls for a long time, the way plugins set devices up lazily from
// Render(). Blocked time is not JavaScript time; the wall-clock cap and stop
// requests must still end such a call (the `mode` parameter picks the loop).

export function Name() { return "Slow render"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x6666; }
export function Size() { return [1, 1]; }
export function ControllableParameters() {
  return [{ property: "mode", group: "", label: "Mode", type: "textfield", default: "pauses" }];
}

/* global device, mode */
export function Validate(endpoint) { return endpoint.interface === 0; }

export function Initialize() {}

export function Render() {
  if (mode === "forever") {
    for (;;) {
      device.pause(900);
    }
  }
  if (mode === "read") {
    while (!device.read([0x00], 64, 5000).length) {
      // waits for a report that never comes
    }
    return;
  }
  for (let i = 0; i < 40; i++) {
    device.pause(900);
  }
}

export function Shutdown() {
  device.write([0x00, 0xff], 2);
}
