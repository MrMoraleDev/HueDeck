// SPDX-License-Identifier: GPL-3.0-or-later
//
// Test helper for the MSI Mystic Light plugins: replays one scenario of the OpenRGB-derived fixtures
// (fixtures/openrgb-*.json, see their "$comment") against plugin.js on the mock device, and returns
// the HID calls the plugin made in the fixtures' event format.

import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

// MSI_ZONE value of each zone name, as the fixture generator coloured them.
export const ZONE_VALUE = {
  JRGB1: 1, JRGB2: 2, PIPE1: 3, PIPE2: 4, JRAINBOW1: 8, JRAINBOW2: 9, JRAINBOW3: 10, JCORSAIR: 11,
  ONBOARD: 13, "Onboard LEDs": 13,
};

// The fixtures' test colour of LED `index` of zone value `zone` in frame `frame`.
export function testColour(frame, zone, index) {
  return [(zone * 16 + frame) & 0xFF, index & 0xFF, (index * 5 + zone * 9 + frame * 50 + 1) & 0xFF];
}

// "ONBOARD, LED 3" / "JRAINBOW1" / "Onboard LEDs LED 3" / "JRGB1 LED" -> zone name.
export function zoneOfLedName(name) {
  const comma = name.indexOf(", LED ");
  if (comma >= 0) {
    return name.slice(0, comma);
  }
  return name.replace(/ LED( \d+)?$/, "");
}

const hex = (bytes) => bytesToHex(bytes).replace(/ /g, "");

/**
 * Runs `scenario` through the plugin at `pluginPath` with `vendorId`. Returns { device, plugin,
 * events, restore, rowZones } where `events` are the I/O calls up to the last frame.
 * `beforeShutdown(plugin, device)` runs after the last frame (its I/O: `hookEvents`, its return
 * value: `extra`); `shutdownMode` is Shutdown()'s second argument (Lightloom exit modes).
 */
export async function runScenario(pluginPath, scenario, { vendorId, beforeShutdown = null, shutdownMode = undefined }) {
  const image = hexToBytes(scenario.image);
  const sizes = { ...scenario.sizes };
  let frame = 0;
  const channelColours = (count) => (name) => {
    const z = ZONE_VALUE[name];
    return Array.from({ length: count }, (_, i) => testColour(frame, z, i));
  };
  const initialChannels = {};
  for (const [name, count] of Object.entries(sizes)) {
    initialChannels[name] = channelColours(count)(name);
  }

  let rowZones = [];
  const device = createMockDevice({
    featureReports: { 0x52: image },
    productId: Number(scenario.pid),
    vendorId,
    channelColors: initialChannels,
    color: (x, y) => testColour(frame, ZONE_VALUE[rowZones[y]], x),
  });
  device.getDeviceInfo = () => ({ product: "MYSTIC LIGHT ", manufacturer: "MSI", serialNumber: scenario.serial });
  // Input reports: the APROM then the LDROM version reply (01 B0 <aprom> CC.., 01 B6 <ldrom> CC..).
  const replies = [scenario.aprom, scenario.ldrom].map((version, k) => {
    const reply = new Array(64).fill(0xCC);
    reply[0] = 0x01;
    reply[1] = k === 0 ? 0xB0 : 0xB6;
    reply[2] = version;
    return reply;
  });
  let lastRead = 0;
  device.read = (id, length, timeout) => {
    device.calls.push({ method: "read", args: [id, length, timeout] });
    const reply = replies.shift();
    lastRead = reply ? reply.length : 0;
    return reply ? reply.slice() : [];
  };
  const getReport = device.get_report;
  const mockReadSize = device.getLastReadSize;
  device.get_report = (id, length) => {
    const out = getReport(id, length);
    lastRead = mockReadSize();
    return out;
  };
  device.getLastReadSize = () => lastRead;

  const restoreGlobals = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    const leds = device.calls.find((c) => c.method === "setControllableLeds");
    if (leds) {
      const [names, positions] = leds.args;
      rowZones = [];
      names.forEach((name, i) => { rowZones[positions[i][1]] = zoneOfLedName(name); });
    }
    const paintChannels = () => {
      for (const [name, count] of Object.entries(sizes)) {
        device.setChannelColors(name, channelColours(count)(name));
      }
    };
    for (; frame < scenario.frames; frame++) {
      paintChannels();
      plugin.Render();
    }
    if (scenario.resize && Object.keys(scenario.resize).length > 0) {
      Object.assign(sizes, scenario.resize);
      for (let k = 0; k < scenario.framesAfter; k++, frame++) {
        paintChannels();
        plugin.Render();
      }
    }
    const events = eventsOf(device.calls);
    const hookStart = device.calls.length;
    const extra = beforeShutdown ? beforeShutdown(plugin, device) : undefined;
    const afterHook = device.calls.length;
    const before = device.calls.length;
    plugin.Shutdown(false, shutdownMode);
    const shutdown = eventsOf(device.calls.slice(before));
    return { device, plugin, events, shutdown, rowZones, extra, hookEvents: eventsOf(device.calls.slice(hookStart, afterHook)) };
  } finally {
    restoreGlobals();
  }
}

/** The mock's calls as fixture events (get_feature, write, read, send_feature, pause). */
export function eventsOf(calls) {
  const out = [];
  for (const { method, args } of calls) {
    switch (method) {
      case "get_report":
        out.push({ op: "get_feature", id: Number(args[0][0]), len: args[1] });
        break;
      case "write":
        out.push({ op: "write", len: args[1], data: hex(normalise(args[0], args[1])) });
        break;
      case "read":
        out.push({ op: "read", len: args[1] });
        break;
      case "send_report":
        out.push({ op: "send_feature", len: args[1], data: hex(normalise(args[0], args[1])) });
        break;
      case "pause":
        out.push({ op: "pause", ms: args[0] });
        break;
      default:
        break;
    }
  }
  return out;
}

/** The fixture's events without marks, answers and returned lengths. */
export function expectedEvents(scenario) {
  const out = [];
  for (const e of scenario.events) {
    switch (e.op) {
      case "get_feature": out.push({ op: "get_feature", id: e.id, len: e.len }); break;
      case "write": out.push({ op: "write", len: e.len, data: e.data }); break;
      case "read": out.push({ op: "read", len: e.len }); break;
      case "send_feature": out.push({ op: "send_feature", len: e.len, data: e.data }); break;
      case "pause": out.push({ op: "pause", ms: e.ms }); break;
      default: break;
    }
  }
  return out;
}

function normalise(data, length) {
  const out = new Array(length).fill(0);
  for (let i = 0; i < Math.min(length, data.length); i++) {
    out[i] = data[i] & 0xFF;
  }
  return out;
}
