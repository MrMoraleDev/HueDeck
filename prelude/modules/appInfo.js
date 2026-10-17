// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Facade for "@SignalRGB/appInfo" (Rust-backed; the real module reports the
// HueDeck version plus `compat: "signalrgb-2.5"`, see architecture-synthesis).
//
// Corpus (2026-10-17, 1006 plugin .js files): 0 imports, 0 member calls.
// Exe strings list only the module name; no member names are evidenced, so
// this facade declares none and throws on ANY property access (plugins would
// read fields, not call methods, on an info object).
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze([]);

const appInfo = createFacade("@SignalRGB/appInfo", MEMBERS, {
  accessThrows: true,
  hint: "no corpus plugin uses this module; open an issue with the field name",
});

export default appInfo;
