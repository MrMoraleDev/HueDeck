// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Synthetic channel/subdevice plugin for the device-session tests, shaped like
// the ARGB-header controllers of the corpus: Initialize() declares two
// channels and a subdevice and logs what the host reports about them;
// Render() writes one output report per channel and one for the subdevice.

import permissions from "@SignalRGB/permissions";
import systeminfo from "@SignalRGB/systeminfo";

export function Name() { return "Channel probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x7777; }
export function Size() { return [1, 1]; }
export function SubdeviceController() { return true; }

/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }

export function Initialize() {
  device.SetLedLimit(200);
  // No components assigned by the test: the host adds Default Strip - 60.
  device.addChannel("ARGB 1", 120);
  // Assigned by the test.
  device.addChannel("ARGB 2", 10, 4);
  device.createSubdevice("Logo");
  device.setSubdeviceName("Logo", "Probe - Logo");
  device.setSubdeviceLeds("Logo", ["Led 1", "Led 2"], [[0, 0], [1, 0]]);
  device.setSubdeviceSize("Logo", 2, 1);
  const ch1 = device.channel("ARGB 1");
  const ch2 = device.channel("ARGB 2");
  const ids = ch2.getComponentNames();
  const data = ch2.getComponentData(ids[0]);
  device.log("probe " + JSON.stringify({
    names: device.getChannelNames(),
    missing: device.channel("nope") === undefined,
    count1: ch1.LedCount(),
    count2: ch2.ledCount,
    limit2: ch2.LedLimit(),
    total: device.getLedCount(),
    ids,
    product: data.ProductName,
    extra: data.Extra,
    component: ch2.getComponent(ids[0]).componentID,
    onlyDefaults1: ch1.onlyHasDefaultComponents(),
    onlyDefaults2: ch2.onlyHasDefaultComponents(),
    subdevices: device.getCurrentSubdevices(),
    pulse: device.getChannelPulseColor("ARGB 1"),
    pulsing: ch1.shouldPulseColors(),
    lighting: permissions.permissions().MSI.lighting,
    board: systeminfo.GetMotherboardInfo().model,
    bios: systeminfo.GetBiosInfo().releaseDate,
    ram: systeminfo.GetRamInfo().map((m) => m.partNumber),
  }));
}

export function Render() {
  const c1 = device.channel("ARGB 1");
  device.write([0x01].concat(c1.getColors("Inline").slice(0, 6)), 7);
  const sep = device.channel("ARGB 2").getColors("Separate", "GRB");
  device.write([0x02, sep[0][0], sep[1][0], sep[2][0]], 4);
  device.write([0x03].concat(device.subdeviceColor("Logo", 1, 0)), 4);
}

export function Shutdown() {}
