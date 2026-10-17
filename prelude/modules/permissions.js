// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Facade for "@SignalRGB/permissions" (Rust-backed partner-permission store).
//
// Corpus (2026-10-17, 1006 plugin .js files): 29 files import it
// (`import permissions from "@SignalRGB/permissions"`). Member calls:
//   permissions.permissions() -> { [partner]: { [key]: bool } }   29
//   permissions.setCallback(fn(data))                             29
// Plugin-side PermissionsManager targets "Lian-Li" and "MSI" and defaults
// every missing key to true; the real Lightloom module answers from local
// config. Until it is registered, calls throw HostApiMissing.
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["permissions", "setCallback"]);

const permissions = createFacade("@SignalRGB/permissions", MEMBERS, {
  hint: "partner permissions are served from lightloomd config (M2)",
});

export default permissions;
