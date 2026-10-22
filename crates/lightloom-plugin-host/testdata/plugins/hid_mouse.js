// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic HID plugin in the SignalRGB compat shape (Lightloom test fixture).
import { Assert } from "@SignalRGB/Errors.js";

export function Name() { return "Synthetic Mouse"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return [0x0001, 0x0002]; }
export function Publisher() { return "Lightloom"; }
export function Size() { return [3, 3]; }
export function DefaultPosition() { return [10, 100]; }
export const DefaultScale = 8.0;
export function DeviceType() { return "mouse"; }
/* global shutdownColor:readonly LightingMode:readonly dpi:readonly battery:readonly */
export function ControllableParameters() {
	return [
		{ property: "shutdownColor", group: "lighting", label: "Shutdown Color", type: "color", default: "#000000" },
		{ property: "LightingMode", group: "lighting", label: "Lighting Mode", type: "combobox", values: ["Canvas", "Forced"], default: "Canvas" },
		{ property: "forcedColor", group: "lighting", label: "Forced Color", type: "color", default: "#009bde" },
		{ property: "dpi", label: "DPI", type: "number", min: "200", max: "16000", step: "50", default: "800" },
	];
}
const names = ["Logo", "Wheel"];
const positions = [[1, 0], [1, 2]];
export function LedNames() { return names; }
export function LedPositions() { return positions; }

export function Validate(endpoint) { return endpoint.interface === 0 || endpoint.interface === -1; }

let initialized = false;
export function Initialize() {
	Assert.isOk(device.productId() === 0x0001, "productId is the first pid");
	Assert.isOk(typeof dpi === "number" && dpi === 800, "number parameter coerced");
	const sent = device.write([0x00, 0x01, dpi & 0xff], 65);
	Assert.isOk(sent === 65, "write returns length");
	const reply = device.read([0x00], 65, 10);
	Assert.isOk(reply.length === 65, "read returns zeroed buffer");
	device.setName(`Synthetic Mouse ${device.productId()}`);
	device.addFeature("battery");
	battery.setBatteryLevel(100);
	console.log("initialized", { dpi });
	initialized = true;
}

export function Render() {
	const packet = [0x00, 0x02];
	for (let i = 0; i < positions.length; i++) {
		const [x, y] = positions[i];
		const c = LightingMode === "Forced" ? [0, 155, 222] : device.color(x, y);
		packet.push(...c);
	}
	device.write(packet, 65);
}

export function Shutdown(SystemSuspending) {
	if (!initialized) {
		throw new Error("Shutdown before Initialize");
	}
	device.log(`shutdown ${SystemSuspending ? "#000000" : shutdownColor}`);
}

export function ondpiChanged() { device.write([0x00, 0x03, dpi & 0xff], 65); }
export function onBrightnessChanged() {}
export function ImageUrl() { return "https://example.invalid/mouse.png"; }
