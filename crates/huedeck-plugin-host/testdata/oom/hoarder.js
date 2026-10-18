// SPDX-License-Identifier: Apache-2.0 OR MIT
// Allocates until the runtime memory cap stops it (HueDeck test fixture).
const hoard = [];
for (;;) {
	hoard.push(new Array(100000).fill(hoard.length));
}
export function Name() { return "Hoarder"; }
