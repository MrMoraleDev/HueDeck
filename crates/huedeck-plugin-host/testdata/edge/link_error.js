// SPDX-License-Identifier: GPL-3.0-or-later
// Parses, but imports a name its dependency does not export (HueDeck test fixture).
import { missing } from "./lib/dep.js";
export function Name() { return "Edge Link Error"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return missing; }
