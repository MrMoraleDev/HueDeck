// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Synthetic device plugin for the device-session tests: Initialize() calls the
// member named by the `probe` parameter, then writes one output report. In a
// device session every member HueDeck does not implement yet must throw
// HostApiMissing, so the write never happens.

import systeminfo from "@SignalRGB/systeminfo";

export function Name() { return "Gap probe"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x5678; }
export function Size() { return [1, 1]; }
export function ControllableParameters() {
  return [{ property: "probe", group: "", label: "Probe", type: "textfield", default: "none" }];
}

/* global device, bus, probe */
export function Validate(endpoint) { return endpoint.interface === 0; }

export function Initialize() {
  switch (probe) {
    case "bulk": device.bulk_transfer(0x01, [1, 2, 3], 3); break;
    case "control": device.control_transfer(0x40, 1, 0, 0, [], 0, 100); break;
    case "setProtocol": device.setProtocol("hybrid"); break;
    case "subdevice": device.createSubdevice("Strip"); break;
    case "fan": device.createFanControl("Fan 1"); break;
    case "board": device.getMotherboardName(); break;
    case "systeminfo": systeminfo.GetMotherboardInfo(); break;
    case "bus": bus.WriteByte(0x50, 0x00, 0x01); break;
    case "udp": device.addFeature("udp"); break;
    case "unknown": device.definitelyNotAHostMember(); break;
    case "fanflag":
      if (device.fanControlDisabled() !== true) {
        throw new Error("fan control must be reported as disabled");
      }
      break;
    default: break;
  }
  device.write([0x00, 0x01], 2);
}

export function Render() {}
export function Shutdown() {}
