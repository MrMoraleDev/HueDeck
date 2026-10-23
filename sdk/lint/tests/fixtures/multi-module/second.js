// SPDX-License-Identifier: GPL-3.0-or-later
// Lint fixture: the second plugin module of the directory (16C0:05DF).
import { SIZE } from "./shared.js";

export function Name() { return "Multi B"; }
export function VendorId() { return 0x16C0; }
export function ProductId() { return [0x05DF]; }
export function Size() { return SIZE; }
export function LedNames() { return ["LED"]; }
export function LedPositions() { return [[0, 0]]; }
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
export function Render() {}
export function Shutdown() {}
