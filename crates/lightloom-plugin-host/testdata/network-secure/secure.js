// SPDX-License-Identifier: GPL-3.0-or-later
// Network plugin fixture for tests/network_secure.rs: the device context reads
// an https endpoint with a self-signed certificate (accepted by its expected
// common name) and streams colours over a DTLS-PSK connection, both on 127.0.0.1.

export function Name() { return "Secure Loopback"; }
export function Publisher() { return "Lightloom"; }
export function Type() { return "network"; }
export function Size() { return [1, 1]; }
export function DeviceType() { return "lightingcontroller"; }

export function DiscoveryService() {
	this.IconUrl = "";
	this.Initialize = function () {};
	this.Update = function () {};
	this.Discovered = function () {};
}

let info = "";
let fingerprint = "";

export function Initialize() {
	device.setName(controller.name);
	device.setControllableLeds(["A"], [[0, 0]]);
	device.addFeature("dtls");
	const x = new XMLHttpRequest();
	x.open("GET", `https://${controller.ip}:${controller.httpsPort}/info`, false);
	x.tlsName = controller.certName;
	x.send();
	info = x.status === 200 ? x.responseText : `failed ${x.status}`;
	fingerprint = x.tlsFingerprint;
	dtls.onConnectionEstablished(() => device.log("dtls: established"));
	dtls.onConnectionError((message) => device.log(`dtls: error ${message}`));
	dtls.onConnectionClosed(() => device.log("dtls: closed"));
	dtls.createConnection(controller.ip, controller.dtlsPort, controller.identity, controller.key);
}

export function Render() {
	if (dtls.hasEncryptedConnection()) {
		const a = device.color(0, 0);
		dtls.send([0xaa, ...a, info.length, fingerprint.length], 1);
	}
}

export function Shutdown() {
	dtls.CloseConnection();
}
