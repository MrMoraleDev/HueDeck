// SPDX-License-Identifier: GPL-3.0-or-later
//
// Facade for "@SignalRGB/serial" (Rust-backed via the `serialport` crate).
//
// Corpus (2026-10-17, 1006 plugin .js files): 7 files import it
// (`import Serial from` 5, `import serial from` 2). Member calls:
//   write(bytes)            25   disconnect()        15   isConnected()   18
//   connect({baudRate, dataBits, stopBits, parity})  7   read(maxBytes)   4
//   getDeviceInfo() -> {deviceId, vendorId, productId, portName}  4
//   availablePorts() -> [{portName, vendorId, productId}]          1
// Docs-only (0 uses): readAll(), getPortName(), getBaudRate().
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze([
  "connect",
  "disconnect",
  "isConnected",
  "write",
  "read",
  "readAll",
  "getDeviceInfo",
  "getPortName",
  "getBaudRate",
  "availablePorts",
]);

const serial = createFacade("@SignalRGB/serial", MEMBERS, {
  hint: "serial transport arrives with huedeck-hal serial backend (M2)",
});

export default serial;
