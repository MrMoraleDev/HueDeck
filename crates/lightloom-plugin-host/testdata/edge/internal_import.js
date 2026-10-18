// SPDX-License-Identifier: GPL-3.0-or-later
// Imports a host-internal module by its internal name (Lightloom test fixture).
import { BinaryUtils } from "lightloom:prelude/BinaryUtils.js";
export function Name() { return "Edge Internal Import"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return BinaryUtils ? 0x0004 : 0x0005; }
