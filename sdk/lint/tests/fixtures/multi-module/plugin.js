// SPDX-License-Identifier: GPL-3.0-or-later
// Lint fixture: one plugin module per vendor id in one directory (this one 1209:0006).
import { SIZE } from "./shared.js";

export function Name() { return "Multi A"; }
export function VendorId() { return 0x1209; }
export function ProductId() { return [0x0006]; }
export function Size() { return SIZE; }
export function LedNames() { return ["LED"]; }
export function LedPositions() { return [[0, 0]]; }
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
export function Render() {}
export function Shutdown() {}
