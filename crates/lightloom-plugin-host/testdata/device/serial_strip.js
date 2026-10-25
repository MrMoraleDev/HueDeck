// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic serial plugin for the device-session tests, shaped like the USB
// CDC LED controllers of the corpus: `Type() "serial"`, matched by VID:PID,
// `Validate` picks interface 0, `connect()` without a portName binds the
// device's own port. Initialize does a handshake (write, then read the reply),
// tries a port of another device (refused) and logs what the module reports;
// Render reconnects when the port went away and writes the colour of its LED.

import serial from "@SignalRGB/serial";

export function Name() { return "Serial strip probe"; }
export function Publisher() { return "Lightloom tests"; }
export function Type() { return "serial"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x5e01; }
export function Size() { return [1, 1]; }

/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }

const SETTINGS = { baudRate: 115200, dataBits: 8, stopBits: "One", parity: "None" };

export function Initialize() {
  const foreign = serial.connect({ ...SETTINGS, portName: "COM9" });
  const unsupported = serial.connect({ ...SETTINGS, stopBits: "OneAndHalf" });
  const connected = serial.connect(SETTINGS);
  const written = serial.write([0xaa, 0x01]);
  const reply = serial.read(4, 500);
  const silent = serial.read(1, 30);
  const info = serial.getDeviceInfo();
  device.log(
    "probe " +
      JSON.stringify({
        foreign,
        unsupported,
        connected,
        written,
        reply,
        silent,
        port: serial.getPortName(),
        baud: serial.getBaudRate(),
        ports: serial.availablePorts().map((p) => [p.portName, p.vendorId, p.productId, p.interface]),
        deviceId: info.deviceId,
        isConnected: serial.isConnected(),
      }),
  );
}

export function Render() {
  if (!serial.isConnected() && !serial.connect(SETTINGS)) {
    return;
  }
  const [r, g, b] = device.color(0, 0);
  serial.write(new Uint8Array([0x55, r, g, b]));
}

export function Shutdown() {
  if (serial.isConnected()) {
    serial.write([0x55, 0, 0, 0]);
  }
  serial.disconnect();
}
