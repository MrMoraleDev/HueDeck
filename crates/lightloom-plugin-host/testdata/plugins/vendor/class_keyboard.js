// SPDX-License-Identifier: Apache-2.0 OR MIT
// Synthetic plugin with `export default class` and a relative import (Lightloom test fixture).
import { PIDLibrary, checksum } from "./lib/protocol.js";

export function Name() { return "Synthetic Keyboard"; }
export function VendorId() { return 0x3434; }
export function ProductId() { return Object.keys(PIDLibrary); }
export function Publisher() { return "Lightloom"; }
export function Size() { return [21, 6]; }
export function DeviceType() { return "keyboard"; }
export function Validate(endpoint) { return endpoint.interface === 1; }

export default class SyntheticKeyboard {
	constructor() {
		this.config = PIDLibrary[device.productId()];
	}
	hello() {
		const packet = [0x00, 0x21];
		packet.push(checksum(packet));
		return device.write(packet, 33);
	}
}

let kb;
export function Initialize() {
	kb = new SyntheticKeyboard();
	if (!kb.config) {
		throw new Error(`no config for pid ${device.productId()}`);
	}
	device.setSize(kb.config.size);
	kb.hello();
}
export function Render() {}
export function Shutdown() {}
