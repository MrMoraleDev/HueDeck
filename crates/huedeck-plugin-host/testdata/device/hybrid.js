// SPDX-License-Identifier: Apache-2.0 OR MIT
// Synthetic hybrid plugin: device sessions must refuse it until libusb transfers exist.
export function Name() { return "Hybrid probe"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x9999; }
export function Type() { return "Hybrid"; }
export function Validate() { return true; }
export function Initialize() { device.bulk_transfer(0x01, [1], 1); }
export function Render() {}
export function Shutdown() {}
