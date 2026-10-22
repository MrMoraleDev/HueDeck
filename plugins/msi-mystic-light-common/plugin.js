// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: MSI Mystic Light motherboard RGB controllers behind vendor id 0x0DB0, product id
// 0x0076 (OpenRGB's "MSI Mystic Light Common" and "MSI Mystic Light X870" detectors): 185-byte boards
// whose own product id is the first four hex digits of the USB serial number, in OpenRGB's Direct mode.
// Adapted from OpenRGB Controllers/MSIMotherboardController/MSIMotherboardControllerDetect.cpp,
// MSIMotherboardCommon.h, MSIMotherboard185Controller/*, MSIMotherboard162Controller/* and
// MSIMotherboard112Controller/* (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Copyright (C) 2020 T-bond, Copyright (C) 2021 Adam Honse, Copyright (C) 2022 thombo
// (185-byte Direct mode based on the mystic-why project by Aleksandr Garashchenko).
//
// ./protocol.js and ./boards.js are identical copies of the ones in plugins/msi-mystic-light (the
// tests check it); only the vendor id, product id and collections differ. A plugin has a single
// vendor id, hence the second directory. Hardware notes: docs/hardware/msi-mystic-light.md.

import { DETECTORS } from "./boards.js";
import { createMysticLight } from "./protocol.js";

const VENDOR_ID = 0x0DB0;
// OpenRGB's REGISTER_HID_DETECTOR_PU rows for this vendor id: product 0x0076 on the collections
// 0x0001/0x00 ("Common") and 0xFF00/0x01 ("X870").
const CLAIMS = DETECTORS.filter((d) => d.vid === VENDOR_ID && !d.untested);
const PRODUCT_IDS = [...new Set(CLAIMS.map((d) => d.pid))];
const USAGES = [...new Set(CLAIMS.map((d) => `${d.usagePage}:${d.usage}`))].map((s) => s.split(":").map(Number));

export function Name() { return "MSI Mystic Light (0DB0)"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/msi-mystic-light"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCT_IDS.slice(); }
export function Type() { return "Hid"; }
export function DeviceType() { return "motherboard"; }
// Placeholder geometry; Initialize() sets the board's real zones.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// MSI's own lighting services (MSI Center / Mystic Light) hold and rewrite this controller; the
// list is the one of plugins/msi-mystic-light-7c91, which was validated on a board of this family.
// Lightloom reports them, never kills them.
export function ConflictingProcesses() {
  return ["LEDKeeper.exe", "LEDKeeper2.exe", "LightKeeperService.exe", "LightKeeperService2.exe"];
}

/* global device */

// REGISTER_HID_DETECTOR_PU(..., 0x0001, 0x00) and (..., 0xFF00, 0x01). The detection read decides
// whether the collection speaks one of the supported report sizes; otherwise nothing is written.
export function Validate(endpoint) {
  return USAGES.some(([page, usage]) => endpoint.usage_page === page && endpoint.usage === usage);
}

let light = null;

// OpenRGB runs its detector on each collection; Lightloom activates the first validated one. So try
// the 0x0001/0x00 ("Common") collection first and the 0xFF00/0x01 ("X870") one only if the first did
// not answer GET_FEATURE 0x52 with a supported length (nothing is written before that answer).
export function Initialize() {
  const endpoints = typeof device.getHidEndpoints === "function" ? device.getHidEndpoints() || [] : [];
  const candidates = USAGES
    .map(([page, usage]) => endpoints.find((e) => e.usage_page === page && e.usage === usage))
    .filter((e) => e !== undefined);
  if (candidates.length < 2 || typeof device.set_endpoint !== "function") {
    light = start(device, "MSI Mystic Light");
    return;
  }
  for (const endpoint of candidates) {
    device.set_endpoint(endpoint.interface, endpoint.usage, endpoint.usage_page, endpoint.collection);
    light = start(device, "MSI Mystic Light");
    if (light.variant !== 0) {
      return;
    }
  }
}

export function Render() {
  render(device, light);
}

export function Shutdown(SystemSuspending) {
  // Normal exit and suspend alike: write the configuration read at start-up back (save byte 0).
  if (light !== null) {
    light.restore();
  }
}

// ---------------------------------------------------------------------------------------------
// Shared with plugins/msi-mystic-light/plugin.js (same code below this line).
// ---------------------------------------------------------------------------------------------
function start(dev, label) {
  const ml = createMysticLight(dev, { label });
  const channelCount = (name, max) => {
    dev.addChannel(name, max);
    return ledCount(dev, name);
  };
  const info = typeof dev.getDeviceInfo === "function" ? dev.getDeviceInfo() : null;
  const serial = info && info.serialNumber != null ? String(info.serialNumber) : "";
  ml.initialize({ productId: Number(dev.productId()), serial, channelCount });

  dev.setName(ml.name);
  const rows = ml.zones.filter((z) => z.kind === "row" && z.row >= 0);
  const names = [];
  const positions = [];
  let width = 1;
  for (const zone of rows) {
    zone.names.forEach((ledName, x) => {
      names.push(ledName);
      positions.push([x, zone.row]);
    });
    width = Math.max(width, zone.names.length);
  }
  dev.setSize([width, Math.max(1, rows.length)]);
  dev.setControllableLeds(names, positions);
  const channels = ml.zones.filter((z) => z.kind === "channel");
  if (channels.length > 0) {
    dev.SetLedLimit(channels.reduce((sum, z) => sum + z.max, 0));
  }
  return ml;
}

function render(dev, ml) {
  if (ml === null || !ml.ready) {
    return;
  }
  const counts = {};
  for (const zone of ml.zones) {
    if (zone.kind === "channel") {
      counts[zone.name] = ledCount(dev, zone.name);
    }
  }
  ml.resize(counts);
  ml.frame((zone) => {
    if (zone.kind === "channel") {
      const flat = channelColours(dev, zone.name);
      return (i) => (3 * i + 2 < flat.length ? [flat[3 * i], flat[3 * i + 1], flat[3 * i + 2]] : [0, 0, 0]);
    }
    return (i) => rgbOrBlack(dev.color(i, zone.row));
  });
}

function ledCount(dev, name) {
  const channel = dev.channel(name);
  return channel ? Number(channel.LedCount()) || 0 : 0;
}

function channelColours(dev, name) {
  const channel = dev.channel(name);
  const colours = channel ? channel.getColors("Inline", "RGB") : [];
  return colours && typeof colours.length === "number" ? Array.from(colours, (v) => v & 0xFF) : [];
}

function rgbOrBlack(rgb) {
  return rgb && typeof rgb.length === "number" && rgb.length >= 3 ? [rgb[0] & 0xFF, rgb[1] & 0xFF, rgb[2] & 0xFF] : [0, 0, 0];
}
