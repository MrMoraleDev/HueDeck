// SPDX-License-Identifier: GPL-3.0-or-later
// Compile-only checks of sdk/huedeck.d.ts (tsc -p sdk): typical plugin code must type-check,
// and misuse must not (the @ts-expect-error lines).

import { Assert } from "@SignalRGB/Errors.js";
import permissions from "@SignalRGB/permissions";
import systeminfo from "@huedeck/systeminfo";
import { createColorArray } from "@huedeck/createColorArray";
import { hexToRgb } from "@huedeck/hexToRgb";
import base64mod from "@SignalRGB/base64";

export const exportsShape: HueDeck.PluginExports = {
  Name: "Typed",
  VendorId: () => 0x1209,
  ProductId: [0x0001],
  HidUsages: [[0x59, 0x01]],
  ControllableParameters: () => [{ property: "speed", type: "number", min: "1", max: 10, default: "5" }],
  Validate: (ep) => ep.interface === 0 && ep.usage_page === 0xff00,
  Scan: (free) => (free.SpdSlots().length > 0 && free.WriteQuick(0x39) === 0 ? [0x39] : []),
  Initialize() {
    device.setName("Typed");
    device.addFeature("battery");
    battery.setBatteryState(battery.charging);
  },
  Render() {
    const [r, g, b] = device.color(0, 0);
    const written: number = device.write([0x00, r, g, b], 65);
    const reply = device.read([0x00], 65, 10);
    const fields = device.getHidReportFields();
    if (fields !== null) fields.fields.forEach((f) => f.usages.length);
    const ch = device.channel("Channel 1");
    const colors = ch ? ch.getColors("Inline", "GRB") : createColorArray("#000000", 8, "Inline");
    Assert.isOk(written >= 0 && reply.length >= 0 && colors.length >= 0);
    if (bus.TransactionLock() === false) return;
    bus.WriteBlock(0x03, 3, [r, g, b]);
    const [status, bytes] = bus.ReadBlockBytes(0x10, 4);
    bus.TransactionUnlock();
    device.log(bytes, { hex: status === 0 });
  },
  Shutdown(suspending) {
    device.write([0x00, ...hexToRgb(suspending ? "#000000" : "#FFFFFF")], 4);
  },
};

const p: boolean = permissions.permissions().MSI.lighting;
const board = systeminfo.GetMotherboardInfo()?.model;
const text: string = base64mod.Encode([1, 2, 3]);
const sensor = engine.getSensorValue("CPU Load")?.value;
void [p, board, text, sensor];

// @ts-expect-error the fixed bus takes no address
bus.ReadByte(0x39, 0x00);
// @ts-expect-error setSubdeviceSize takes two numbers, not an array
device.setSubdeviceSize("a", [1, 2]);
// @ts-expect-error not a feature
device.addFeature("teleport");
