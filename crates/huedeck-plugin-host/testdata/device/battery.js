// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic device plugin for the device-session tests: the `battery`
// feature. Initialize reports a level and a state and logs what the global
// reads back; every Render writes the sampled colour of its one LED and the
// low-battery warning level it sees.

export function Name() { return "Battery probe"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x2222; }
export function Size() { return [1, 1]; }

/* global device, battery */
export function Validate(endpoint) { return endpoint.interface === 0; }

export function Initialize() {
  device.addFeature("battery");
  const before = { level: battery.batteryLevel, state: battery.batteryState };
  battery.setBatteryLevel(42.4);
  battery.setBatteryState(battery.draining);
  battery.setBatteryLevel("not a number");
  device.log(
    `probe ${JSON.stringify({
      before,
      level: battery.batteryLevel,
      state: battery.batteryState,
      text: battery.stateString,
      warn: battery.lowBatteryWarningLevel,
      constants: [battery.unknown, battery.draining, battery.charging, battery.fullCharging, battery.full, battery.wirelessCharging],
    })}`,
  );
}

let frames = 0;
export function Render() {
  frames += 1;
  if (frames === 3) {
    battery.setBatteryState(2);
    battery.setBatteryLevel(120);
  }
  const [r, g, b] = device.color(0, 0);
  device.write([0x00, r, g, b, battery.lowBatteryWarningLevel], 5);
}

export function Shutdown() {}
