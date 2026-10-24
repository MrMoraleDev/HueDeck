// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: MSI Mystic Light motherboard RGB controllers under vendor id 0x1462 (185-, 162-
// and 112-byte configuration reports), OpenRGB's Direct mode: onboard LEDs, JRGB and pipe zones,
// and the JRAINBOW / JCORSAIR ARGB headers as channels on per-LED boards.
// Adapted from OpenRGB Controllers/MSIMotherboardController/MSIMotherboardControllerDetect.cpp,
// MSIMotherboardCommon.h, MSIMotherboard185Controller/*, MSIMotherboard162Controller/* and
// MSIMotherboard112Controller/* (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Copyright (C) 2020 T-bond, Copyright (C) 2021 Adam Honse, Copyright (C) 2022 thombo
// (185-byte Direct mode based on the mystic-why project by Aleksandr Garashchenko).
//
// The protocol lives in ./protocol.js and the board tables in ./boards.js (generated from
// OpenRGB's tables); plugins/msi-mystic-light-common carries identical copies for vendor id 0x0DB0.
// Product 0x7C91 (MAG B550 TOMAHAWK) is left to plugins/msi-mystic-light-7c91, which was validated
// on that board. Hardware notes: docs/hardware/msi-mystic-light.md.

import { DETECTORS } from "./boards.js";
import { createMysticLight } from "./protocol.js";

const VENDOR_ID = 0x1462;
// Boards that have their own plugin.
const EXCLUDED_PIDS = new Set([0x7C91]);
// OpenRGB's REGISTER_HID_DETECTOR_PU rows for this vendor id that it compiles (the
// ENABLE_UNTESTED_MYSTIC_LIGHT rows are left out, as in OpenRGB), minus the excluded boards.
const CLAIMS = DETECTORS.filter((d) => d.vid === VENDOR_ID && !d.untested && !EXCLUDED_PIDS.has(d.pid));
const PRODUCT_IDS = [...new Set(CLAIMS.map((d) => d.pid))];
const USAGES = [...new Set(CLAIMS.map((d) => `${d.usagePage}:${d.usage}`))].map((s) => s.split(":").map(Number));

export function Name() { return "MSI Mystic Light"; }
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

// REGISTER_HID_DETECTOR_PU(..., 0x0001, 0x00): the collection with usage page 0x0001, usage 0x00.
export function Validate(endpoint) {
  return USAGES.some(([page, usage]) => endpoint.usage_page === page && endpoint.usage === usage);
}

let light = null;

export function Initialize() {
  light = start(device, "MSI Mystic Light");
}

export function Render() {
  render(device, light);
}

// Lightloom extensions (docs/host-api/save-shutdown.md): "Save to devices" stores OpenRGB's Static or
// Breathing mode with one colour per zone in the board's flash (185- and 162-byte boards); the exit
// modes "keep" and "off" show the last colours or black in Static mode, never stored.
export function SaveCapabilities() { return { static: true, effects: ["breathing"] }; }
export function ShutdownModes() { return ["restore", "keep", "off"]; }

export function SaveToDevice(request) {
  if (light === null) {
    return { unsupported: "the board was not recognised" };
  }
  const effect = request && request.effect ? String(request.effect) : null;
  const colourOf = canvasColours(device);
  return request && request.action === "save" ? light.save(colourOf, effect) : light.describeSave(colourOf, effect);
}

export function Shutdown(SystemSuspending, mode) {
  if (light === null) {
    return;
  }
  if (!SystemSuspending && (mode === "keep" || mode === "off")) {
    light.showStatic(mode === "keep" ? canvasColours(device) : () => () => [0, 0, 0]);
    return;
  }
  // Normal exit and suspend alike: write the configuration read at start-up back (save byte 0).
  light.restore();
}

// ---------------------------------------------------------------------------------------------
// Shared with plugins/msi-mystic-light-common/plugin.js (same code below this line).
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
  ml.frame(canvasColours(dev));
}

// The frame's colours per zone, as frame() wants them: zone -> (index) -> [r, g, b].
function canvasColours(dev) {
  return (zone) => {
    if (zone.kind === "channel") {
      const flat = channelColours(dev, zone.name);
      return (i) => (3 * i + 2 < flat.length ? [flat[3 * i], flat[3 * i + 1], flat[3 * i + 2]] : [0, 0, 0]);
    }
    return (i) => rgbOrBlack(dev.color(i, zone.row));
  };
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
