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

test("metadata: OpenRGB's DualShock 4 and DualSense ids (the sony-dualsense plugin was retired), in the manifest too", async () => {
  const { plugin, restore } = await setup({ productId: 0x0CE6 });
  try {
    assert.equal(plugin.VendorId(), 0x054C);
    assert.deepEqual(plugin.ProductId(), [...DS4_PIDS, ...DUALSENSE_PIDS]);
    assert.equal(manifest.devices.length, 1);
    assert.equal(Number(manifest.devices[0].vid), 0x054C);
    assert.deepEqual(manifest.devices[0].pids.map(Number), plugin.ProductId());
    assert.deepEqual(manifest.devices[0].tested, ["0x0CE6"]);
    assert.equal(manifest.devices[0].transport, "hid");
    assert.equal(plugin.Type(), "Hid");
    assert.equal(plugin.Publisher(), "Lightloom");
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
    // The DualSense's only HID collection (Lightloom's device listing): interface 3, 0x0001/0x0005.
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
        assert.deepEqual(writtenHex(device), [hex(frame.lightloom)], `colour ${frame.color}`);
        assert.equal(device.written[0].length, 11);
      }
      device.written.length = 0;
      device.setColor([0x77, 0x88, 0x99]);
      plugin.Shutdown(false);
      assert.deepEqual(writtenHex(device), [hex(expected.ds4.shutdown.lightloom)]);
      device.written.length = 0;
      plugin.Shutdown(true);
      assert.deepEqual(writtenHex(device), [hex(expected.ds4.shutdown.lightloom)], "system suspend");
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
    assert.deepEqual(writtenHex(device), [hex(expected.ds4.frames[3].lightloom)]);
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
        assert.deepEqual(writtenHex(device), [hex(frame.lightloom)], JSON.stringify(frame.players));
        assert.equal(device.written[0].length, 48);
      }
      const off = expected.dualsense.shutdown;
      globalThis.playerLedBrightness = off.brightness;
      device.setColor([0x77, 0x88, 0x99]);
      for (const suspending of [false, true]) {
        device.written.length = 0;
        plugin.Shutdown(suspending);
        assert.deepEqual(writtenHex(device), [hex(off.lightloom)]);
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
      assert.deepEqual(writtenHex(device), [hex(expected.dualsense.shutdown.lightloom)], JSON.stringify(params));
      assert.equal(device.written[0].bytes[43], 0x01, "0x02 - brightness 1");
    } finally {
      restore();
    }
  }
});

// The bits Lightloom clears in OpenRGB's reports (docs/hardware/sony-gamepad.md).
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
    assert.deepEqual(hexToBytes(frame.lightloom), maskDs4(openrgb));
  }
  for (const frame of [...expected.dualsense.frames, expected.dualsense.shutdown]) {
    const openrgb = hexToBytes(frame.openrgb);
    assert.equal(openrgb.length, 48);
    assert.deepEqual([openrgb[1], openrgb[2], openrgb[9], openrgb[39]], [0x0F, 0x55, 0x01, 0xFF],
      "OpenRGB's flags and Direct mode value");
    assert.deepEqual(hexToBytes(frame.lightloom), maskDualSense(openrgb));
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

// --- battery ------------------------------------------------------------------------------------

/** A USB input report 0x01 of 64 bytes with `status` at `offset` (report id = byte 0). */
function batteryInput(offset, status) {
  const report = usbInput(0x01);
  report[offset] = status;
  return report;
}

/** Make the mock's read() answer from a queue (empty: nothing waiting, size 0). */
function queueReads(device) {
  const queue = [];
  let size = 0;
  device.read = (ids, length, timeout) => {
    device.calls.push({ method: "read", args: [ids, length, timeout] });
    const next = queue.shift();
    size = next ? next.length : 0;
    return next ? next.slice() : [];
  };
  device.getLastReadSize = () => size;
  return queue;
}

const DUALSENSE_CASES = [
  { status: 0x08, level: 85, state: 1, note: "discharging, 8 -> 85 %" },
  { status: 0x0A, level: 100, state: 1, note: "discharging, 10 -> 100 % (clamped)" },
  { status: 0x13, level: 35, state: 2, note: "charging, 3 -> 35 %" },
  { status: 0x20, level: 100, state: 4, note: "full" },
  { status: 0x00, level: 5, state: 1, note: "discharging, empty" },
];

for (const id of DUALSENSE_PIDS) {
  test(`DualSense ${pid(id)} battery: byte 53 of input report 0x01, level and charging state`, async (t) => {
    t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
    for (const c of DUALSENSE_CASES) {
      const { device, plugin, restore } = await setup({ productId: id });
      try {
        const queue = queueReads(device);
        plugin.Initialize();
        assert.deepEqual(device.features, ["battery"]);
        queue.push(batteryInput(53, 0x05), batteryInput(53, c.status));
        plugin.Render();
        assert.deepEqual([device.battery.levels, device.battery.states], [[c.level], [c.state]], c.note);
        const reads = callsTo(device, "read");
        assert.equal(reads.length, 3, "drained until nothing waits; the newest report counts");
        assert.ok(reads.every(([, length, timeout]) => length === 64 && timeout === 0), "non-blocking reads");
        assert.equal(device.sent.length, 0, "nothing is sent for the battery");
        assert.equal(device.written.length, 1, "only the frame");
      } finally {
        restore();
      }
    }
  });
}

test("DualSense battery: error states are unknown and keep the last level", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup({ productId: 0x0CE6 });
  try {
    const queue = queueReads(device);
    plugin.Initialize();
    queue.push(batteryInput(53, 0x06));
    plugin.Render();
    t.mock.timers.tick(5000);
    queue.push(batteryInput(53, 0xB3));
    plugin.Render();
    assert.deepEqual([device.battery.levels, device.battery.states], [[65], [1, 0]]);
  } finally {
    restore();
  }
});

test("DualSense battery: polled every 5 s, changes reported only, nothing waiting changes nothing", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup({ productId: 0x0CE6 });
  try {
    const queue = queueReads(device);
    plugin.Initialize();
    queue.push(batteryInput(53, 0x07));
    plugin.Render();
    assert.equal(callsTo(device, "read").length, 2);
    queue.push(batteryInput(53, 0x17));
    t.mock.timers.tick(4999);
    plugin.Render();
    assert.equal(callsTo(device, "read").length, 2, "no read before 5 s");
    t.mock.timers.tick(1);
    plugin.Render();
    assert.deepEqual([device.battery.levels, device.battery.states], [[75], [1, 2]], "same level, new state");
    t.mock.timers.tick(5000);
    plugin.Render();
    assert.deepEqual([device.battery.levels, device.battery.states], [[75], [1, 2]], "nothing read: unchanged");
    queue.push(...new Array(100).fill(batteryInput(53, 0x17)));
    t.mock.timers.tick(5000);
    const before = callsTo(device, "read").length;
    plugin.Render();
    assert.equal(callsTo(device, "read").length - before, 64, "at most 64 reads per poll");
  } finally {
    restore();
  }
});

const DS4_CASES = [
  { status: 0x15, level: 55, state: 2, note: "cable, 5 -> charging 55 %" },
  { status: 0x1A, level: 100, state: 2, note: "cable, 10 -> 100 %, still charging" },
  { status: 0x1B, level: 100, state: 4, note: "cable, 11 -> full" },
  { status: 0x04, level: 45, state: 1, note: "no cable, 4 -> discharging 45 %" },
  { status: 0x0B, level: 100, state: 1, note: "no cable, 11 -> clamped to 100 %" },
];

for (const id of DS4_PIDS) {
  test(`DualShock 4 ${pid(id)} battery: byte 30 of input report 0x01, bit 4 = cable`, async (t) => {
    t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
    for (const c of DS4_CASES) {
      const { device, plugin, restore } = await setup({ productId: id });
      try {
        const queue = queueReads(device);
        plugin.Initialize();
        assert.deepEqual(device.features, ["battery"]);
        queue.push(batteryInput(30, c.status));
        plugin.Render();
        assert.deepEqual([device.battery.levels, device.battery.states], [[c.level], [c.state]], c.note);
      } finally {
        restore();
      }
    }
  });
}

test("DualShock 4 battery: a charging error (cable, 14/15) is unknown", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup({ productId: 0x09CC });
  try {
    const queue = queueReads(device);
    plugin.Initialize();
    queue.push(batteryInput(30, 0x1E));
    plugin.Render();
    assert.deepEqual([device.battery.levels, device.battery.states], [[], [0]]);
  } finally {
    restore();
  }
});

test("DualShock 4 USB wireless adapter battery: byte 31 bit 2 = no controller connected (state unknown)", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup({ productId: 0x0BA0 });
  try {
    const queue = queueReads(device);
    plugin.Initialize();
    queue.push(batteryInput(30, 0x07));
    plugin.Render();
    const gone = batteryInput(30, 0x00);
    gone[31] = 0x04;
    queue.push(gone);
    t.mock.timers.tick(5000);
    plugin.Render();
    assert.deepEqual([device.battery.levels, device.battery.states], [[75], [1, 0]], "level kept, state unknown");
  } finally {
    restore();
  }
});

test("battery: not on a Bluetooth DualShock 4", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  for (const [productId, inputReports] of [[0x09CC, { 0: usbInput(0x11) }]]) {
    const { device, plugin, restore } = await setup({ productId, inputReports });
    try {
      plugin.Initialize();
      const reads = callsTo(device, "read").length;
      plugin.Render();
      t.mock.timers.tick(10_000);
      plugin.Render();
      assert.deepEqual(device.features, []);
      assert.equal(callsTo(device, "read").length, reads, "no battery reads");
      assert.deepEqual(device.battery.states, []);
    } finally {
      restore();
    }
  }
});
