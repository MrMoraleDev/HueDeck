// SPDX-License-Identifier: GPL-3.0-or-later
//
// Facade for "@SignalRGB/performance" (Rust-backed timing probes).
//
// Corpus (2026-10-17, 1006 plugin .js files): 0 imports, 0 member calls.
// Exe strings list the module name and the PerformanceModule methods
// StartTimer, EndTimer, LogReport, GetTimers (argument shapes unknown).
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["StartTimer", "EndTimer", "LogReport", "GetTimers"]);

const performance = createFacade("@SignalRGB/performance", MEMBERS, {
  hint: "no corpus plugin uses this module; open an issue with the call shape",
});

export default performance;
