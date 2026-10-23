// SPDX-License-Identifier: GPL-3.0-or-later
//
// `@SignalRGB/systeminfo` as a Lightloom device session answers it on a machine whose SMBIOS
// table could not be read (host.js `systeminfoModule`): no motherboard or BIOS facts, no RAM.

const systeminfo = Object.freeze({
  GetMotherboardInfo: () => undefined,
  GetBiosInfo: () => undefined,
  GetRamInfo: () => [],
});

export default systeminfo;
