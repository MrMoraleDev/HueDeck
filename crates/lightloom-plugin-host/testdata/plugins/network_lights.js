// SPDX-License-Identifier: Apache-2.0 OR MIT
// Synthetic network plugin in the SignalRGB compat shape (Lightloom test fixture).
import udp from "@SignalRGB/udp";

export function Name() { return "Synthetic LAN Lights"; }
export function Publisher() { return "Lightloom"; }
export function Type() { return "network"; }
export function Version() { return "1.0.0"; }
export function DeviceType() { return "lightingcontroller"; }
export function SubdeviceController() { return true; }
export function DefaultComponentBrand() { return "Lightloom"; }

export function Initialize() {
	device.setName(controller.name);
	device.addChannel("Strip", 100);
	const [r, g, b] = device.channel("Strip").getColors("Separate");
	if (![r, g, b].every(Array.isArray)) {
		throw new TypeError("getColors(\"Separate\") must return three arrays");
	}
	// Plugin-defined field of the announced controller (the stub does not carry it).
	const port = controller.streamingPort ?? 4003;
	udp.send(controller.ip, port, [1, 2, 3]);
}
export function Render() {
	const ch = device.channel("Strip");
	const colors = ch.shouldPulseColors()
		? device.createColorArray(device.getChannelPulseColor("Strip"), ch.LedLimit(), "Inline")
		: ch.getColors("Inline", "GRB");
	globalThis.lastColors = colors;
}
export function Shutdown() {}

export function DiscoveryService() {
	this.IconUrl = "https://example.invalid/icon.png";
	this.UdpBroadcastPort = 4001;
	this.UdpListenPort = 4002;
	this.Initialize = function () {
		this.socket = udp.createSocket();
		this.socket.on("message", (msg) => this.Discovered(msg));
		this.socket.bind(0);
	};
	this.Update = function () { service.broadcast("hello"); };
	this.Discovered = function (value) { service.log(value); };
	this.Removal = function () {};
}
