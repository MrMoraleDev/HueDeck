// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic rawusb plugin for the raw USB session tests (no HID endpoint, no
// Validate): vendor control requests like the Lian Li Uni Fan hubs, bulk IN
// and OUT like the Aqua Computer / LCD plugins. The `probe` parameter adds
// calls the host must refuse.

export function Name() { return "Raw USB probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x7701; }
export function Type() { return "rawusb"; }
export function Size() { return [1, 1]; }
export function ControllableParameters() {
  return [{ property: "probe", group: "", label: "Probe", type: "textfield", default: "none" }];
}

/* global device, probe */
const hex = (bytes) => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

export function Initialize() {
  const sent = device.control_transfer(0x40, 0x80, 0, 0x0010, [0x01, 0x02], 2, 1000);
  const rpm = device.control_transfer(0xc0, 0x81, 0, 0xd800, [], 4, 1000);
  device.log(`control ${sent} ${hex(rpm)} ${device.getLastReadSize()}`);
  const status = device.bulk_transfer(0x82, new Array(64).fill(0), 64, 500);
  device.log(`bulk-in ${hex(status)} ${device.getLastReadSize()}`);
  const none = device.bulk_transfer(0x82, new Array(64).fill(0), 64, 50);
  device.log(`timeout ${none.length} ${device.getLastReadSize()}`);
  if (probe === "refusals") {
    // A class request to the HID interface, a standard SET_CONFIGURATION, the HID IN endpoint.
    const hid = device.control_transfer(0x21, 0x09, 0x0200, 0x0000, [1], 1, 100);
    const std = device.control_transfer(0x00, 0x09, 0x0001, 0x0000, [], 0, 100);
    const ep = device.bulk_transfer(0x81, [], 8, 100);
    device.log(`refused ${hid} ${std} ${ep.length}`);
  }
}

export function Render() {
  const [r, g, b] = device.color(0, 0);
  device.bulk_transfer(0x02, [0xa0, r, g, b], 4);
}

export function Shutdown() {
  device.bulk_transfer(0x02, [0xa0, 0, 0, 0], 4);
}
