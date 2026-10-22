// SPDX-License-Identifier: GPL-3.0-or-later
//
// Facade for "@SignalRGB/tcp" (Rust-backed). Zero corpus uses; the docs give
// createSocket() -> socket {connect, send, bind, close, on("connected"|"disconnected"|"message"|"error")}
// and the exe lists the module name. Kept so an import never fails at
// module-evaluation time; calls throw HostApiMissing.
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["createSocket"]);

const tcp = createFacade("@SignalRGB/tcp", MEMBERS, {
  hint: "network sockets arrive with the lightloom-plugin-host net module (M6)",
});

export default tcp;
