// SPDX-License-Identifier: Apache-2.0 OR MIT
// Parses, then throws a SyntaxError at run time (Lightloom test fixture).
const settings = JSON.parse("{");
export function Name() { return "Edge Runtime SyntaxError"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return settings.pid; }
