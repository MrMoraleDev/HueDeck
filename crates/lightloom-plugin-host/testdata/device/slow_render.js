// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Synthetic device plugin for the device-session tests: Render() blocks in
// device.pause() for a long time, the way plugins set devices up lazily from
// Render(). Paused time is not JavaScript time, and a stop request must cut
// it short so Shutdown() runs promptly.

export function Name() { return "Slow render"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x6666; }
export function Size() { return [1, 1]; }

/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }

export function Initialize() {}

export function Render() {
  for (let i = 0; i < 40; i++) {
    device.pause(900);
  }
}

export function Shutdown() {
  device.write([0x00, 0xff], 2);
}
