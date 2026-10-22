// SPDX-License-Identifier: GPL-3.0-or-later
//
// Facade for "@SignalRGB/systeminfo" (Rust-backed: SMBIOS/WMI on Windows,
// sysfs DMI on Linux).
//
// Corpus (2026-10-17, 1006 plugin .js files): 21 files import it
// (`import systeminfo from "@SignalRGB/systeminfo"`). Member calls:
//   GetMotherboardInfo() -> {manufacturer, model}        16  (.model read 15x)
//   GetRamInfo()         -> [{partNumber, manufacturer}]  5  (.length, [i].manufacturer)
//   GetBiosInfo()        -> {releaseDate}                 4  (new Date(releaseDate))
// Exe moc: SystemInfoModule methods GetMotherboardInfo/GetBiosInfo/GetRamInfo.
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["GetMotherboardInfo", "GetBiosInfo", "GetRamInfo"]);

const systeminfo = createFacade("@SignalRGB/systeminfo", MEMBERS, {
  hint: "system info arrives with huedeck-inputs sensors (M5)",
});

export default systeminfo;
