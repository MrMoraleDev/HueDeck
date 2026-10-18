// SPDX-License-Identifier: Apache-2.0 OR MIT
// Synthetic SMBus plugin in the SignalRGB compat shape (HueDeck test fixture).
export function Name() { return "Synthetic RAM"; }
export function Publisher() { return "HueDeck"; }
export function Type() { return "SMBUS"; }
export function DeviceType() { return "ram"; }
export function Size() { return [2, 8]; }
export function ConflictingProcesses() { return ["NotARealService.exe"]; }

/** @param {FreeAddressBus} bus */
export function Scan(bus) {
	const found = [];
	if (bus.Type() !== "Free") {
		throw new Error("Scan must receive the free-address wrapper");
	}
	for (const addr of [0x70, 0x71]) {
		if (bus.WriteQuick(addr) === 0) {
			found.push(addr);
		}
	}
	return found;
}

export function Initialize() {
	if (bus.Type() !== "Fixed" || !String(bus).startsWith("I2CBusWrapperFixedAddress")) {
		throw new Error("global bus must be the fixed-address wrapper");
	}
	bus.WriteByte(0x00, 0x01);
	const [code, data] = bus.ReadBlockBytes(0x10, 4);
	if (code !== 0 || data.length !== 4) {
		throw new Error("ReadBlockBytes shape");
	}
}
export function Render() { bus.WriteBlock(0x20, 3, [0, 0, 0]); }
export function Shutdown() {}
