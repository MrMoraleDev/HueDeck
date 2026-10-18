// SPDX-License-Identifier: Apache-2.0 OR MIT
// Async Initialize() awaiting a promise nothing can settle (HueDeck test fixture).
export function Name() { return "Edge Async Never"; }
export function VendorId() { return 0x1111; }
export function ProductId() { return 0x0002; }
export async function Initialize() {
	await new Promise(() => {});
}
export function Render() {}
export function Shutdown() {}
