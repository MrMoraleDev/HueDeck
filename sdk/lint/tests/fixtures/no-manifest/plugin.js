// SPDX-License-Identifier: GPL-3.0-or-later
// Lint fixture: a compat-v1 plugin with no manifest (and no Validate).

/* global device */

import { Assert } from "@SignalRGB/Errors.js";

export function Name() { return "No Manifest"; }
export function VendorId() { return 0x1209; }
export function ProductId() { return 0x0005; }
export function Size() { return [1, 1]; }
export function LedNames() { return ["LED"]; }
export function LedPositions() { return [[0, 0]]; }
export function Initialize() { Assert.isOk(true, "never fails"); }
export function Render() { device.write([0x00, ...device.color(0, 0)], 4); }
export function Shutdown() {}
