// SPDX-License-Identifier: GPL-3.0-or-later
// Allocates until the runtime memory cap stops it (Lightloom test fixture).
const hoard = [];
for (;;) {
	hoard.push(new Array(100000).fill(hoard.length));
}
export function Name() { return "Hoarder"; }
