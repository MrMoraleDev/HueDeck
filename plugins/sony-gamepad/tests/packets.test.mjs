// SPDX-License-Identifier: GPL-3.0-or-later
//
// Sony DualShock 4 / DualSense plugin (adapted from OpenRGB's SonyGamepadController): for every
// product id, the start-up I/O, frames and shutdown are byte for byte the reports worked out from
// the OpenRGB C++ in fixtures/expected.json; those reports differ from OpenRGB's only by the
// documented flag bits; no report ever enables rumble, haptics, triggers, audio or the microphone
// LED; Validate and the geometry follow OpenRGB's detector and RGBControllers.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));

// SonyGamepadControllerDetect.cpp, and the names its RGBControllers use.
const DS4_PIDS = [0x05C4, 0x09CC, 0x0BA0];
const DUALSENSE_PIDS = [0x0CE6, 0x0DF2];
const NAMES = new Map([
  [0x05C4, "Sony DualShock 4"],
  [0x09CC, "Sony DualShock 4"],
  [0x0BA0, "Sony DualShock 4"],
  [0x0CE6, "Sony DualSense"],
  [0x0DF2, "Sony DualSense Edge"],
]);
const DUALSENSE_NAMES = ["Lightbar", "Player 1", "Player 2", "Player 3", "Player 4", "Player 5"];
const DUALSENSE_POSITIONS = [[2, 0], [0, 1], [1, 1], [2, 1], [3, 1], [4, 1]];

const pid = (id) => `0x${id.toString(16).toUpperCase().padStart(4, "0")}`;
const hex = (h) => bytesToHex(hexToBytes(h));
const writtenHex = (device) => device.written.map((w) => bytesToHex(w.bytes));
const callsTo = (device, method) => device.calls.filter((c) => c.method === method).map((c) => c.args);
const usbInput = (id) => [id, ...new Array(63).fill(0)];

async function setup({ productId, color = [0, 0, 0], params = {}, inputReports = { 0: usbInput(0x01) } } = {}) {
  const device = createMockDevice({ productId, vendorId: 0x054C, color, inputReports });
  const restore = installGlobals(device, params);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

// The canvas of a DualSense: the light bar cell and the five player cells.
function dualSenseCanvas(frame) {
  return (x, y) => {
    if (y === 0 && x === 2) return frame.lightbar;
    if (y === 1) return frame.players[x];
    return [0x55, 0x55, 0x55];
  };
}

test("metadata: OpenRGB's DualShock 4 ids (DualSense ids kept by sony-dualsense), in the manifest too", async () => {
  const { plugin, restore } = await setup({ productId: 0x0CE6 });
  try {
    assert.equal(plugin.VendorId(), 0x054C);
    assert.deepEqual(plugin.ProductId(), [...DS4_PIDS]);
    assert.equal(manifest.devices.length, 1);
    assert.equal(Number(manifest.devices[0].vid), 0x054C);
    assert.deepEqual(manifest.devices[0].pids.map(Number), plugin.ProductId());
    assert.deepEqual(manifest.devices[0].tested, []);
    assert.equal(manifest.devices[0].transport, "hid");
    assert.equal(plugin.Type(), "Hid");
    assert.equal(plugin.Publisher(), "HueDeck");
    assert.equal(plugin.Version(), manifest.version);
    assert.equal(plugin.Documentation(), "hardware/sony-gamepad");
    assert.equal(plugin.DeviceType(), manifest.deviceType);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    const [w, h] = plugin.Size();
    for (const [x, y] of plugin.LedPositions()) {
      assert.ok(x >= 0 && x < w && y >= 0 && y < h);
    }
    const params = plugin.ControllableParameters();
    assert.deepEqual(params.map((p) => p.property), ["playerLedBrightness"]);
    assert.deepEqual(params[0].values, ["Low", "Medium", "High"]);
    assert.equal(params[0].default, "Medium");
  } finally {
    restore();
  }
});

test("Validate takes the game pad collection of a USB interface only", async () => {
  const { plugin, restore } = await setup({ productId: 0x0CE6 });
  try {
    // The DualSense's only HID collection (HueDeck's device listing): interface 3, 0x0001/0x0005.
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0001, usage: 0x0005, collection: 0 }), true);
    // OpenRGB's detector takes any interface number.
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0005, collection: 0 }), true);
    // Bluetooth (no USB interface): not supported.
    assert.equal(plugin.Validate({ interface: -1, usage_page: 0x0001, usage: 0x0005, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0xFF00, usage: 0x0005, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0001, usage: 0x0004, collection: 0 }), false);
  } finally {
    restore();
  }
});

for (const id of DS4_PIDS) {
  test(`DualShock 4 ${pid(id)}: start-up probe, frames and shutdown are OpenRGB's USB reports`, async () => {
    const { device, plugin, restore } = await setup({ productId: id });
    try {
      plugin.Initialize();
      assert.deepEqual(callsTo(device, "setName"), [[NAMES.get(id)]]);
      assert.deepEqual(callsTo(device, "setSize"), [[[1, 1]]]);
      assert.deepEqual(callsTo(device, "setControllableLeds"), [[["Lightbar"], [[0, 0]]]]);
      // SonyDS4Controller's constructor: GET_FEATURE 0x02 (64), then five input reports of 64.
      const probe = expected.ds4.initialize;
      assert.deepEqual(callsTo(device, "get_report").map(([ids, length]) => [ids[0], length]),
        [[probe.get_report.id, probe.get_report.length]]);
      const reads = callsTo(device, "read");
      assert.equal(reads.length, probe.reads);
      for (const [, length] of reads) {
        assert.equal(length, probe.read_length);
      }
      assert.equal(device.written.length, 0, "nothing is written at start");
      assert.equal(device.sent.length, 0, "no feature report is sent");

      for (const frame of expected.ds4.frames) {
        device.written.length = 0;
        device.setColor(frame.color);
        plugin.Render();
        assert.deepEqual(writtenHex(device), [hex(frame.huedeck)], `colour ${frame.color}`);
        assert.equal(device.written[0].length, 11);
      }
      device.written.length = 0;
      device.setColor([0x77, 0x88, 0x99]);
      plugin.Shutdown(false);
      assert.deepEqual(writtenHex(device), [hex(expected.ds4.shutdown.huedeck)]);
      device.written.length = 0;
      plugin.Shutdown(true);
      assert.deepEqual(writtenHex(device), [hex(expected.ds4.shutdown.huedeck)], "system suspend");
      assert.equal(device.sent.length, 0);
    } finally {
      restore();
    }
  });
}

test("a DualShock 4 that sends Bluetooth input reports (0x11) gets nothing", async () => {
  const { device, plugin, restore } = await setup({
    productId: 0x09CC,
    color: [0xFF, 0x00, 0x00],
    inputReports: { 0: usbInput(expected.ds4.initialize.bluetooth_report_id) },
  });
  try {
    plugin.Initialize();
    assert.equal(callsTo(device, "read").length, 1, "OpenRGB stops reading at the first 0x11 report");
    plugin.Render();
    plugin.Shutdown(false);
    assert.equal(device.written.length, 0);
    assert.ok(device.logs.some((l) => /Bluetooth/.test(l.message)));
  } finally {
    restore();
  }
});

test("a DualShock 4 whose input reports time out is driven as USB", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x05C4, color: [1, 2, 3], inputReports: {} });
  try {
    plugin.Initialize();
    assert.equal(callsTo(device, "read").length, 5);
    plugin.Render();
    assert.deepEqual(writtenHex(device), [hex(expected.ds4.frames[3].huedeck)]);
  } finally {
    restore();
  }
});

for (const id of DUALSENSE_PIDS) {
  test(`DualSense ${pid(id)}: no start-up I/O, frames and shutdown are OpenRGB's USB reports`, async () => {
    const first = expected.dualsense.frames[0];
    const { device, plugin, restore } = await setup({
      productId: id,
      color: dualSenseCanvas(first),
      params: { playerLedBrightness: first.brightness },
    });
    try {
      plugin.Initialize();
      assert.deepEqual(callsTo(device, "setName"), [[NAMES.get(id)]]);
      assert.deepEqual(callsTo(device, "setSize"), [[[5, 2]]]);
      assert.deepEqual(callsTo(device, "setControllableLeds"), [[DUALSENSE_NAMES, DUALSENSE_POSITIONS]]);
      // SonyDualSenseController's constructor does no I/O.
      for (const method of ["write", "send_report", "get_report", "read"]) {
        assert.equal(callsTo(device, method).length, 0, method);
      }

      for (const frame of expected.dualsense.frames) {
        device.written.length = 0;
        device.setColor(dualSenseCanvas(frame));
        globalThis.playerLedBrightness = frame.brightness;
        plugin.Render();
        assert.deepEqual(writtenHex(device), [hex(frame.huedeck)], JSON.stringify(frame.players));
        assert.equal(device.written[0].length, 48);
      }
      const off = expected.dualsense.shutdown;
      globalThis.playerLedBrightness = off.brightness;
      device.setColor([0x77, 0x88, 0x99]);
      for (const suspending of [false, true]) {
        device.written.length = 0;
        plugin.Shutdown(suspending);
        assert.deepEqual(writtenHex(device), [hex(off.huedeck)]);
      }
      assert.equal(device.sent.length, 0);
    } finally {
      restore();
    }
  });
}

test("DualSense player LED brightness: OpenRGB's default when the parameter is missing or unknown", async () => {
  for (const params of [{}, { playerLedBrightness: "Brightest" }, { playerLedBrightness: "toString" }]) {
    const { device, plugin, restore } = await setup({ productId: 0x0CE6, color: [0, 0, 0], params });
    try {
      plugin.Initialize();
      plugin.Render();
      assert.deepEqual(writtenHex(device), [hex(expected.dualsense.shutdown.huedeck)], JSON.stringify(params));
      assert.equal(device.written[0].bytes[43], 0x01, "0x02 - brightness 1");
    } finally {
      restore();
    }
  }
});

// The bits HueDeck clears in OpenRGB's reports (docs/hardware/sony-gamepad.md).
function maskDs4(bytes) {
  const out = bytes.slice();
  out[1] &= 0x06;
  return out;
}
function maskDualSense(bytes) {
  const out = bytes.slice();
  out[1] &= 0x00;
  out[2] &= 0x14;
  out[9] = 0x00;
  out[39] &= 0x03;
  return out;
}

test("the expected reports are OpenRGB's with only the documented bits cleared", () => {
  for (const frame of [...expected.ds4.frames, expected.ds4.shutdown]) {
    const openrgb = hexToBytes(frame.openrgb);
    assert.equal(openrgb.length, 11);
    assert.equal(openrgb[1], 0x07, "OpenRGB enables motors, light bar and blink");
    assert.deepEqual(hexToBytes(frame.huedeck), maskDs4(openrgb));
  }
  for (const frame of [...expected.dualsense.frames, expected.dualsense.shutdown]) {
    const openrgb = hexToBytes(frame.openrgb);
    assert.equal(openrgb.length, 48);
    assert.deepEqual([openrgb[1], openrgb[2], openrgb[9], openrgb[39]], [0x0F, 0x55, 0x01, 0xFF],
      "OpenRGB's flags and Direct mode value");
    assert.deepEqual(hexToBytes(frame.huedeck), maskDualSense(openrgb));
  }
});

test("no report enables rumble, haptics, triggers, audio or the microphone LED", async () => {
  let seed = 7;
  const random = () => {
    seed = (seed * 1103515245 + 12345) & 0x7FFFFFFF;
    return seed & 0xFF;
  };
  for (const id of [...DS4_PIDS, ...DUALSENSE_PIDS]) {
    for (const brightness of ["Low", "Medium", "High"]) {
      const { device, plugin, restore } = await setup({
        productId: id,
        color: () => [random(), random() & 0x01, random() & 0x80],
        params: { playerLedBrightness: brightness },
      });
      try {
        plugin.Initialize();
        for (let i = 0; i < 20; i++) plugin.Render();
        plugin.Shutdown(false);
        assert.equal(device.written.length, 21);
        for (const { bytes, length } of device.written) {
          if (DS4_PIDS.includes(id)) {
            assert.equal(length, 11);
            assert.equal(bytes[0], 0x05);
            assert.equal(bytes[1], 0x06, "valid_flag0: light bar and blink only, never the motors");
            assert.deepEqual(bytes.slice(2, 6), [0, 0, 0, 0], "valid_flag1, reserved, motors");
            assert.deepEqual(bytes.slice(9, 11), [0, 0], "no blinking");
          } else {
            assert.equal(length, 48);
            assert.equal(bytes[0], 0x02);
            assert.equal(bytes[1], 0x00, "valid_flag0: no vibration, haptics, triggers, volume or audio");
            assert.equal(bytes[2], 0x14, "valid_flag1: light bar and player LEDs only");
            assert.deepEqual(bytes.slice(3, 39), new Array(36).fill(0),
              "motors, volumes, audio, microphone LED, power save, triggers, audio_control2");
            assert.equal(bytes[39], 0x03, "valid_flag2: player LED brightness and light bar setup only");
            assert.deepEqual(bytes.slice(40, 43), [0, 0, 0], "reserved, lightbar_setup");
            assert.equal(bytes[43], 2 - { Low: 0, Medium: 1, High: 2 }[brightness]);
            assert.equal(bytes[44] & 0xE0, 0x20);
          }
        }
      } finally {
        restore();
      }
    }
  }
});

test("an unknown product id is left alone", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x1234, color: [9, 9, 9] });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.equal(device.calls.filter((c) => ["write", "send_report", "get_report", "read"].includes(c.method)).length, 0);
    assert.ok(device.logs.some((l) => /0x1234/.test(l.message)));
  } finally {
    restore();
  }
});
