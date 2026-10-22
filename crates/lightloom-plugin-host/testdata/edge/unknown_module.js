// SPDX-License-Identifier: GPL-3.0-or-later
// Imports a host module that does not exist (Lightloom test fixture).
import nope from "@SignalRGB/nope";
export function Name() { return "Edge Unknown Module"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return 0x0003; }
export function Initialize() { return nope; }
