// SPDX-License-Identifier: GPL-3.0-or-later
// Touches undocumented host members and never returns from Initialize (HueDeck test fixture).
export function Name() { return "Synthetic Gaps"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return 0x2222; }

export function Initialize() {
	device.frobnicate(1, 2, 3);
	if (typeof bus.NotARealMember !== "function") {
		throw new Error("gap fallback must be callable");
	}
	for (;;) {
		device.pause(1);
	}
}
export function Render() {}
export function Shutdown() {}
