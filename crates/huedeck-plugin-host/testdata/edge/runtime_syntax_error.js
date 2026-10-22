// SPDX-License-Identifier: GPL-3.0-or-later
// Parses, then throws a SyntaxError at run time (HueDeck test fixture).
const settings = JSON.parse("{");
export function Name() { return "Edge Runtime SyntaxError"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return settings.pid; }
