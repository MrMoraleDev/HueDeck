// SPDX-License-Identifier: GPL-3.0-or-later
//
// Facade for "@SignalRGB/udp" (Rust-backed in huedeck-plugin-host; this file
// only stands in when the native module is not registered, so every call
// fails loudly with HostApiMissing instead of silently doing nothing).
//
// Corpus (2026-10-17, 1006 plugin .js files):
//   import udp from "@SignalRGB/udp"      3 addon files (WLED, Govee, Yeelight)
//   udp.createSocket()                    6 calls -> socket {on, bind, connect, send, write, close, disconnect, address, remoteAddress, state}
//   udp.send(ip, port, bytes[, endian])  21 calls  (feature global after device.addFeature("udp"); max 8192 bytes)
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["createSocket", "send"]);

const udp = createFacade("@SignalRGB/udp", MEMBERS, {
  hint: "network sockets arrive with the huedeck-plugin-host net module (M6)",
});

export default udp;
