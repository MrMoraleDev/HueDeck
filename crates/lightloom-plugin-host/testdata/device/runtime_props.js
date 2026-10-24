// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic device plugin for the device-session tests: parameters and
// messages that change at run time. Initialize adds a `dpi` property (numeric
// strings, `live: false`) and a message; choosing mode "b" removes `dpi`,
// relabels `mode` through getProperty + addProperty, adds a keycode property,
// drops the exported message and raises a notification; mode "a" withdraws
// the notification. Every Render writes the `dpi` global (in hundreds).

export function Name() { return "Runtime properties probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x8888; }
export function Size() { return [1, 1]; }
export function ControllableParameters() {
  return [{ property: "mode", group: "lighting", label: "Mode", type: "combobox", values: ["a", "b"], default: "a" }];
}
export function DeviceMessages() {
  return [{ property: "fw", message: "Firmware 1.0", tooltip: "Update in the vendor app" }];
}

/* global device, mode, dpi */
export function Validate(endpoint) { return endpoint.interface === 0; }

let alert;

export function Initialize() {
  device.addProperty({
    property: "dpi", group: "dpi", label: "DPI", type: "number",
    min: "100", max: "1600", step: "100", default: "800", live: false, order: "1",
  });
  device.addMessage("hello", "Hello from the plugin", "");
  device.addProperty(5);
  device.addProperty({ label: "nameless" });
  device.log(`names ${device.getPropertyNames().join(",")} dpi ${typeof dpi} ${dpi}`);
}

export function Render() {
  device.write([0x00, typeof dpi === "number" ? dpi / 100 : 0xff], 2);
}

export function onmodeChanged() {
  if (mode === "b") {
    device.removeProperty("dpi");
    const p = device.getProperty("mode");
    p.label = "Mode B";
    device.addProperty(p);
    device.addProperty({ property: "key", label: "Key", type: "keycode", default: "F1" });
    device.removeMessage("fw");
    alert = device.notify("Heads up", "Mode b is on", 2, "Documentation");
  } else if (alert !== undefined) {
    device.denotify(alert);
    alert = undefined;
  }
}

export function ondpiChanged() {
  device.write([0x01, dpi / 100], 2);
}

export function Shutdown() {}
