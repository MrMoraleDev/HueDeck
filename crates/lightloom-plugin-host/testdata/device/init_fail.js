// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Synthetic device plugin for the device-manager tests: Initialize() throws,
// so its session ends on its own before the device is ever removed.

export function Name() { return "Failing init"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x3333; }
export function Size() { return [1, 1]; }

export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() { throw new Error("broken on purpose"); }
export function Render() {}
export function Shutdown() {}
