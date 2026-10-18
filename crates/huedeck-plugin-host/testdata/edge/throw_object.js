// SPDX-License-Identifier: Apache-2.0 OR MIT
// Throws a plain object that is not an Error (HueDeck test fixture).
export function Name() { return "Edge Throw Object"; }
throw { message: "not an Error instance" };
