// SPDX-License-Identifier: GPL-3.0-or-later
// Initialize() returns, but a promise job it queued never ends (HueDeck test fixture).
export function Name() { return "Edge Job Spin"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return 0x0001; }
export function Initialize() {
	Promise.resolve().then(function spin() {
		for (;;) {
			device.pause(1);
		}
	});
}
export function Render() {}
export function Shutdown() {}
