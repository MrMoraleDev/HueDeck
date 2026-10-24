// SPDX-License-Identifier: GPL-3.0-or-later
// Network plugin fixture for tests/network.rs: a "lamp" on 127.0.0.1 answers
// scans over UDP; the device context streams colours to it and reads an HTTP
// endpoint. Ports come from the plugin settings / the announced controller.
import udp from "@SignalRGB/udp";

export function Name() { return "Loopback Lights"; }
export function Publisher() { return "HueDeck"; }
export function Type() { return "network"; }
export function Size() { return [2, 1]; }
export function DeviceType() { return "lightingcontroller"; }

class Lamp {
	constructor(id, ip, port, httpPort) {
		this.id = id;
		this.name = `Lamp ${id}`;
		this.ip = ip;
		this.port = port;
		this.httpPort = httpPort;
		this.status = "found";
		this.paired = false;
	}
	startLink() {
		this.paired = true;
		this.status = "paired";
		service.saveSetting(this.id, "paired", "yes");
		service.updateController(this);
		service.announceController(this);
	}
}

export function DiscoveryService() {
	this.IconUrl = "";
	this.Initialize = function () {
		this.lampPort = Number(service.getSetting("test", "lampPort"));
		this.httpPort = Number(service.getSetting("test", "httpPort"));
		this.socket = udp.createSocket();
		this.socket.on("message", (msg) => {
			const id = msg.data.split(":")[1];
			if (service.getController(id) === undefined) {
				service.addController(new Lamp(id, msg.address, msg.port, this.httpPort));
			}
		});
		this.socket.bind(0);
		this.ticks = 0;
	};
	this.Update = function () {
		this.ticks += 1;
		this.socket.write("scan", "127.0.0.1", this.lampPort);
		service.saveSetting("test", "ticks", String(this.ticks));
	};
	this.forceDiscover = function (ip) {
		service.addController(new Lamp(`manual-${ip}`, ip, 1, 1));
	};
	this.forgetController = function (id) {
		const c = service.getController(id);
		if (c !== undefined) {
			service.suppressController(c);
			service.removeController(c);
		}
	};
	this.Discovered = function () {};
	this.Removal = function () {};
}

let info = "";
export function Initialize() {
	device.setName(controller.name);
	device.setControllableLeds(["A", "B"], [[0, 0], [1, 0]]);
	device.addFeature("udp");
	const x = new XMLHttpRequest();
	x.open("GET", `http://${controller.ip}:${controller.httpPort}/info`, false);
	x.send();
	info = x.status === 200 ? x.responseText : `failed ${x.status}`;
}

export function Render() {
	const a = device.color(0, 0);
	const b = device.color(1, 0);
	udp.send(controller.ip, controller.port, [0xaa, ...a, ...b, info.length]);
}

export function Shutdown() {
	udp.send(controller.ip, controller.port, [0xbb]);
}
