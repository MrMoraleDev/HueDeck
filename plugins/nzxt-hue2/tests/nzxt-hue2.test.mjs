// SPDX-License-Identifier: GPL-3.0-or-later
//
// NZXT Hue 2 plugin: for several products and colours, the start, a frame and the shutdown are byte
// for byte the reports worked out from OpenRGB's NZXTHue2Controller (fixtures/expected.json, not
// produced by plugin.js); plus geometry, Validate, the bounded reply wait, frame skipping, and the
// packets the user's Kraken X3 already accepted in Lightloom's recordings (fixtures/recorded-kraken-x3.json).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginDir = join(here, "..");
const pluginPath = join(pluginDir, "plugin.js");
const expected = JSON.parse(readFileSync(join(pluginDir, "fixtures", "expected.json"), "utf8"));
const recorded = JSON.parse(readFileSync(join(pluginDir, "fixtures", "recorded-kraken-x3.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(pluginDir, "manifest.json"), "utf8"));

const hex = (list) => list.map((h) => bytesToHex(hexToBytes(h)));
const written = (device) => device.written.map((w) => bytesToHex(w.bytes));

// The ids in NZXTHue2ControllerDetect.cpp, in registration order, with the RGB channel count of
// their Detect* function.
const PRODUCTS = [
  [0x2009, 2], [0x2010, 2], [0x200E, 2], [0x2011, 6], [0x2019, 6], [0x2020, 6], [0x201F, 6],
  [0x2022, 6], [0x201B, 6], [0x2001, 4], [0x2002, 2], [0x2005, 2], [0x200B, 2], [0x2007, 3],
  [0x2014, 3], [0x3012, 2], [0x2012, 3], [0x2021, 3], [0x2006, 2], [0x200D, 2], [0x200F, 2],
];
const KRAKEN_X3 = [0x2007, 0x2014];

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

// Mock options for one fixture case.
function caseOptions(c) {
  const inputReports = {};
  if (c.firmwareReply) inputReports[0x11] = hexToBytes(c.firmwareReply);
  if (c.deviceListReply) inputReports[0x21] = hexToBytes(c.deviceListReply);
  return {
    productId: Number(c.productId),
    inputReports,
    color: (x, y) => c.cells[`${x},${y}`] ?? c.canvas,
    channelColors: c.channels,
  };
}

for (const c of expected.cases) {
  test(`${c.name}: start, frame and shutdown are OpenRGB's reports`, async () => {
    const { device, plugin, restore } = await setup(caseOptions(c));
    try {
      plugin.Initialize();
      assert.deepEqual(written(device), hex(c.expected.init), "Initialize");
      device.written.length = 0;
      plugin.Render();
      assert.deepEqual(written(device), hex(c.expected.frame), "Render");
      device.written.length = 0;
      plugin.Shutdown(false);
      assert.deepEqual(written(device), hex(c.expected.shutdown), "Shutdown");
      assert.ok(device.written.every((w) => w.length === 64));
    } finally {
      restore();
    }
  });

  test(`${c.name}: channels, default LED counts and the firmware version`, async () => {
    const { device, plugin, restore } = await setup(caseOptions(c));
    try {
      plugin.Initialize();
      const pid = Number(c.productId);
      const names = KRAKEN_X3.includes(pid)
        ? ["Channel 1"]
        : Array.from({ length: PRODUCTS.find(([p]) => p === pid)[1] }, (_, i) => `Channel ${i + 1}`);
      const defaults = c.expected.defaultLedCounts;
      assert.deepEqual(device.channels, names.map((name) => ({
        name,
        ledLimit: 40,
        defaultLedCount: defaults === null ? undefined : defaults[name],
      })));
      const limit = device.calls.find((call) => call.method === "SetLedLimit");
      assert.deepEqual(limit.args, [names.length * 40]);
      if (c.expected.firmware !== null) {
        assert.ok(device.logs.some((l) => l.message.includes(`firmware ${c.expected.firmware}.`)), "firmware logged");
      }
    } finally {
      restore();
    }
  });
}

test("metadata: OpenRGB's product ids, 2007 included (the nzxt-kraken-x3 plugin was retired), in the manifest too", async () => {
  const { plugin, restore } = await setup();
  try {
    assert.equal(plugin.VendorId(), 0x1E71);
    assert.deepEqual(plugin.ProductId(), PRODUCTS.map(([pid]) => pid));
    assert.ok(plugin.ProductId().includes(0x2007));
    assert.equal(manifest.devices.length, 1);
    assert.equal(Number(manifest.devices[0].vid), 0x1E71);
    assert.deepEqual(manifest.devices[0].pids.map(Number), plugin.ProductId());
    assert.deepEqual(manifest.devices[0].tested, ["0x2007"]);
    assert.deepEqual(manifest.conflictingProcesses, ["NZXT CAM.exe"]);
    assert.equal(plugin.Publisher(), "Lightloom");
    assert.equal(plugin.Version(), manifest.version);
    assert.equal(plugin.Documentation(), "hardware/nzxt-hue2");
    assert.equal(plugin.Type(), "Hid");
    assert.equal(plugin.DeviceType(), manifest.deviceType);
    assert.deepEqual(plugin.ConflictingProcesses(), []);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
  } finally {
    restore();
  }
});

test("Validate accepts only vendor-defined collections (Lightloom binds the first accepted one)", async () => {
  const { plugin, restore } = await setup();
  try {
    // The collection of the user's Kraken X3 (Lightloom's device listing).
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x0001, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFFC0, usage: 0x0001, collection: 1 }), true);
    // A standard collection (here a keyboard) is never the one the 22 xx reports go to.
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x0001, usage: 0x0006, collection: 2 }), false);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x000C, usage: 0x0001, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("geometry: the Kraken X3 ids get the ring (a circle) and the logo, the others only channels", async () => {
  for (const [pid, channels] of PRODUCTS) {
    const { device, plugin, restore } = await setup({ productId: pid });
    try {
      plugin.Initialize();
      const call = (method) => device.calls.filter((c) => c.method === method).map((c) => c.args);
      assert.equal(call("setName").length, 1, `0x${pid.toString(16)} named`);
      if (KRAKEN_X3.includes(pid)) {
        assert.deepEqual(call("setSize"), [[[5, 5]]]);
        assert.deepEqual(call("setControllableLeds"), [[
          ["Ring LED 1", "Ring LED 2", "Ring LED 3", "Ring LED 4", "Ring LED 5", "Ring LED 6", "Ring LED 7", "Ring LED 8", "Logo LED"],
          [[3, 0], [4, 1], [4, 3], [3, 4], [1, 4], [0, 3], [0, 1], [1, 0], [2, 2]],
        ]]);
        assert.deepEqual(device.channels.map((c) => c.name), ["Channel 1"]);
      } else {
        assert.deepEqual(call("setSize"), [[[1, 1]]]);
        assert.deepEqual(call("setControllableLeds"), [[[], []]]);
        assert.deepEqual(device.channels.map((c) => c.name), Array.from({ length: channels }, (_, i) => `Channel ${i + 1}`));
      }
    } finally {
      restore();
    }
  }
  const names = { 0x2007: "NZXT Kraken X3 Series", 0x2022: "NZXT RGB & Fan Controller 2024", 0x201B: "NZXT B650E Motherboard",
    0x3012: "NZXT Kraken 2024 ELITE Series RGB", 0x200F: "NZXT Smart Device V2", 0x2002: "NZXT Hue 2 Ambient" };
  for (const [pid, name] of Object.entries(names)) {
    const { device, plugin, restore } = await setup({ productId: Number(pid) });
    try {
      plugin.Initialize();
      assert.deepEqual(device.calls.find((c) => c.method === "setName").args, [name]);
    } finally {
      restore();
    }
  }
});

test("every zone takes channel bit 1 << zone index, 6 channels up to 0x20", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x2020 });
  try {
    plugin.Initialize();
    device.written.length = 0;
    plugin.Render();
    // No components: one empty 22 10 report and one apply report per zone, as OpenRGB sends for a
    // zone of 0 LEDs.
    assert.deepEqual(device.written.map((w) => [w.bytes[0], w.bytes[1], w.bytes[2]]), [
      [0x22, 0x10, 0x01], [0x22, 0xA0, 0x01], [0x22, 0x10, 0x02], [0x22, 0xA0, 0x02],
      [0x22, 0x10, 0x04], [0x22, 0xA0, 0x04], [0x22, 0x10, 0x08], [0x22, 0xA0, 0x08],
      [0x22, 0x10, 0x10], [0x22, 0xA0, 0x10], [0x22, 0x10, 0x20], [0x22, 0xA0, 0x20],
    ]);
  } finally {
    restore();
  }
});

test("only lighting reports are ever written: no fan (0x62), no hardware effect (0x28, 0x2A)", async () => {
  for (const c of expected.cases) {
    const { device, plugin, restore } = await setup(caseOptions(c));
    try {
      plugin.Initialize();
      for (let i = 0; i < 3; i++) plugin.Render();
      plugin.Shutdown(true);
      for (const { bytes, length } of device.written) {
        assert.equal(length, 64);
        const head = (bytes[0] << 8) | bytes[1];
        assert.ok([0x1001, 0x2003, 0x2210, 0x2211, 0x22A0].includes(head), `${c.name}: ${bytesToHex(bytes.slice(0, 4))}`);
      }
      assert.equal(device.calls.filter((call) => call.method === "send_report").length, 0, "no feature reports");
    } finally {
      restore();
    }
  }
});

test("a reply is waited for at most 10 unrelated reports or 1000 ms of silence", async () => {
  // Every read returns a 64-byte fan status report (67 02), never the reply.
  const status = [0x67, 0x02, ...new Array(62).fill(0)];
  {
    const { device, plugin, restore } = await setup({ productId: 0x2006, inputReports: { 0x11: status, 0x21: status } });
    try {
      plugin.Initialize();
      assert.equal(device.calls.filter((c) => c.method === "read").length, 20, "10 reads per request");
      assert.deepEqual(device.channels.map((c) => c.defaultLedCount), [undefined, undefined]);
      assert.deepEqual(device.calls.filter((c) => c.method === "read").map((c) => c.args[2]), new Array(20).fill(0), "non-blocking reads");
    } finally {
      restore();
    }
  }
  {
    // A quiet device: the firmware request is polled every 10 ms for 1000 ms; the device-list
    // request is still sent but, as the device never sent a report, read once and not waited for.
    const { device, plugin, restore } = await setup({ productId: 0x2006 });
    try {
      plugin.Initialize();
      assert.equal(device.pauses.length, 100);
      assert.ok(device.pauses.every((ms) => ms === 10));
      assert.equal(device.calls.filter((c) => c.method === "read").length, 101 + 1);
      assert.deepEqual(written(device).map((h) => h.slice(0, 5)), ["10 01", "20 03"]);
    } finally {
      restore();
    }
  }
  {
    // A reply shorter than 64 bytes is not OpenRGB's reply (ret_val != 64).
    const short = [0x21, 0x03, ...new Array(30).fill(0x0B)];
    const { device, plugin, restore } = await setup({ productId: 0x2006, inputReports: { 0x21: short } });
    try {
      plugin.Initialize();
      assert.deepEqual(device.channels.map((c) => c.defaultLedCount), [undefined, undefined]);
    } finally {
      restore();
    }
  }
});

test("an unchanged frame is skipped for up to 1 s, a changed one goes out at once", async () => {
  const realNow = Date.now;
  let now = 1_000_000;
  Date.now = () => now;
  const { device, plugin, restore } = await setup({
    productId: 0x2006,
    channelColors: { "Channel 1": [[1, 2, 3]], "Channel 2": [] },
  });
  try {
    plugin.Initialize();
    device.written.length = 0;
    plugin.Render();
    assert.equal(device.written.length, 4);
    now += 999;
    plugin.Render();
    assert.equal(device.written.length, 4, "unchanged, within 1 s");
    now += 1;
    plugin.Render();
    assert.equal(device.written.length, 8, "keepalive after 1 s");
    device.setChannelColors("Channel 1", [[1, 2, 4]]);
    plugin.Render();
    assert.equal(device.written.length, 12, "changed colour");
    assert.deepEqual(device.written[8].bytes.slice(0, 7), [0x22, 0x10, 0x01, 0x00, 2, 1, 4]);
    // The shutdown always goes out, and the next frame after it is sent again.
    plugin.Shutdown(false);
    assert.equal(device.written.length, 16);
    plugin.Render();
    assert.equal(device.written.length, 20);
  } finally {
    Date.now = realNow;
    restore();
  }
});

test("a frame whose write failed is sent again on the next Render", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x2002, channelColors: { "Channel 1": [[9, 8, 7]] } });
  try {
    plugin.Initialize();
    const realWrite = device.write;
    let fail = true;
    device.write = (data, length) => {
      const n = realWrite(data, length);
      return fail ? -1 : n;
    };
    device.written.length = 0;
    plugin.Render();
    assert.equal(device.written.length, 4);
    fail = false;
    plugin.Render();
    assert.equal(device.written.length, 8);
    plugin.Render();
    assert.equal(device.written.length, 8, "then skipped as unchanged");
  } finally {
    restore();
  }
});

test("an id outside the table sends nothing", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x1234 });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.equal(device.written.length, 0);
    assert.equal(device.channels.length, 0);
  } finally {
    restore();
  }
});

test("Kraken X3: header and ring reports equal what the device received in Lightloom's recordings", async () => {
  for (const [key, rgb] of [["green", [0, 255, 0]], ["orange", [255, 128, 0]]]) {
    const rec = recorded[key];
    const { device, plugin, restore } = await setup({
      productId: 0x2007,
      color: rgb,
      channelColors: { "Channel 1": new Array(20).fill(rgb) },
    });
    try {
      plugin.Initialize();
      device.written.length = 0;
      plugin.Render();
      const ours = written(device);
      const theirs = hex(rec.frame);
      // Header (bit 0x01) and ring (bit 0x02): colour report and apply report, byte for byte.
      for (const i of [0, 1, 2, 3]) {
        assert.ok(theirs.includes(ours[i]), `${key}: ${ours[i].slice(0, 8)} was recorded`);
      }
      // What differs, by design of OpenRGB's protocol: the recording also sent an empty second
      // group (22 11) for zones of 20 LEDs or fewer, and the logo as a 2A 04 report, where OpenRGB
      // sends the logo as zone 2 (bit 0x04) with 22 10 and an apply report.
      const extra = theirs.filter((h) => !ours.includes(h)).map((h) => h.slice(0, 8));
      assert.deepEqual(extra, ["22 11 01", "22 11 02", "2A 04 04"]);
      assert.deepEqual(ours.slice(4).map((h) => h.slice(0, 8)), ["22 10 04", "22 A0 04"]);
      assert.equal(ours[4].slice(12, 20), bytesToHex([rgb[1], rgb[0], rgb[2]]), "logo colour G R B");
    } finally {
      restore();
    }
  }
});
