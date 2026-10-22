// SPDX-License-Identifier: GPL-3.0-or-later
// Throws a plain object that is not an Error (Lightloom test fixture).
export function Name() { return "Edge Throw Object"; }
throw { message: "not an Error instance" };
