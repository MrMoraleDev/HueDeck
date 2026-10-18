// SPDX-License-Identifier: Apache-2.0 OR MIT
// Helper module (no ProductId) imported by class_keyboard.js (HueDeck test fixture).
export const PIDLibrary = {
	0x0DA0: { name: "Synthetic Keyboard A", size: [21, 6] },
	0x0DA1: { name: "Synthetic Keyboard B", size: [17, 6] },
};

export function checksum(bytes) {
	return bytes.reduce((a, b) => (a + b) & 0xff, 0);
}
