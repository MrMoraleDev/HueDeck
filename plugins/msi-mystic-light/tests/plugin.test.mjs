// SPDX-License-Identifier: GPL-3.0-or-later
//
// MSI Mystic Light (vendor id 0x1462): the plugin against the HID traffic OpenRGB's own code
// produces for the same boards, images, zone sizes and colours (fixtures/openrgb-direct.json), plus
// the Lightloom safety rules (restore image, save byte, refusing to write without a read).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes } from "../../_testkit/mock-device.mjs";
import { runScenario, expectedEvents, testColour } from "./openrgb-scenario.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const pluginPath = join(root, "plugin.js");
const fixtures = JSON.parse(readFileSync(join(root, "fixtures", "openrgb-direct.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(root, "manifest.json"), "utf8"));
const scenario = (id) => fixtures.scenarios.find((s) => s.id === id);

// ---------------------------------------------------------------------------------------------
// Byte for byte against OpenRGB
// ---------------------------------------------------------------------------------------------
// Documented deviations from OpenRGB (docs/hardware/msi-mystic-light.md):
//  * 185-7d88-no-stream: OpenRGB sends the per-LED switch and never a stream; Lightloom stops after the
//    reads and writes nothing.
//  * keepDeviceConfig scenarios: OpenRGB's Direct mode as GetDeviceConfig() left it; checked by the
//    162 test further down, not byte for byte.
const NO_WRITE_DEVIATION = new Set(["185-7d88-no-stream"]);
const expectedFor = (s) => {
  const all = expectedEvents(s);
  return NO_WRITE_DEVIATION.has(s.id) ? all.filter((e) => e.op !== "send_feature") : all;
};
const BYTE_FOR_BYTE = fixtures.scenarios.filter((s) => !s.keepDeviceConfig);

for (const s of BYTE_FOR_BYTE) {
  test(`OpenRGB scenario ${s.id}: same HID calls, byte for byte`, async () => {
    const { events } = await runScenario(pluginPath, s, { vendorId: 0x1462 });
    assert.deepEqual(events, expectedFor(s), s.note);
  });
}

// The restore image: as read, save byte 0, and on 185-byte boards the J_CORSAIR +9 (0x3E) read-back
// bias of 4 removed (values below 4 as read).
function expectedRestore(imageHex) {
  const image = hexToBytes(imageHex);
  image[image.length - 1] = 0;
  if (image.length === 185 && image[0x3E] >= 4) {
    image[0x3E] -= 4;
  }
  return { op: "send_feature", len: image.length, data: image.map((b) => b.toString(16).padStart(2, "0")).join("").toUpperCase() };
}

for (const s of BYTE_FOR_BYTE) {
  test(`scenario ${s.id}: Shutdown writes the start-up image back with the save byte 0, and only after a write`, async () => {
    const { events, shutdown } = await runScenario(pluginPath, s, { vendorId: 0x1462 });
    const wrote = events.some((e) => e.op === "send_feature");
    if (!wrote) {
      assert.deepEqual(shutdown, []);
      return;
    }
    assert.deepEqual(shutdown, [expectedRestore(s.image)]);
  });
}

test("185-byte restore: the J_CORSAIR +9 byte is written back as read - 4 (7C91 read-back bias), below 4 as read", async () => {
  const s = scenario("185-7c79-sync");
  for (const [read, written] of [[0x04, 0x00], [0x24, 0x20], [0x03, 0x03]]) {
    const image = hexToBytes(s.image);
    image[0x3E] = read;
    const hex = image.map((b) => b.toString(16).padStart(2, "0")).join("").toUpperCase();
    const { shutdown } = await runScenario(pluginPath, { ...s, image: hex }, { vendorId: 0x1462 });
    assert.equal(shutdown.length, 1);
    assert.equal(hexToBytes(shutdown[0].data)[0x3E], written, `read ${read}`);
  }
});

test("162-byte board whose first zone reads Static: brightness 100 % and custom colour kept; OpenRGB's GetDeviceConfig() path differs only there", async () => {
  const theirs = scenario("162-7b85-static-first-openrgb-config");
  const { events } = await runScenario(pluginPath, theirs, { vendorId: 0x1462 });
  const ours = events.filter((e) => e.op === "send_feature");
  const openrgb = expectedEvents(theirs).filter((e) => e.op === "send_feature");
  assert.equal(ours.length, openrgb.length);
  let differences = 0;
  ours.forEach((e, k) => {
    const a = hexToBytes(e.data);
    const b = hexToBytes(openrgb[k].data);
    for (let i = 1; i < a.length - 1; i++) {
      if (a[i] === b[i]) {
        continue;
      }
      differences += 1;
      const field = (i - 1) % 10;
      if (field === 4) {
        assert.equal(a[i] & ~0x7C, b[i] & ~0x7C, `byte ${i}: only brightness bits differ`);
        assert.equal(a[i] & 0x7C, 10 << 2, `byte ${i}: Lightloom brightness 10`);
      } else {
        assert.equal(field, 8, `byte ${i} differs outside +4/+8`);
        assert.equal(a[i] ^ b[i], 0x80, `byte ${i}: only the custom-colour bit differs`);
        assert.ok(a[i] & 0x80, `byte ${i}: Lightloom keeps the custom colour`);
      }
    }
  });
  assert.ok(differences > 0, "the OpenRGB path really differs");
});

test("MPG Z790-S WIFI (7D88, JRGB1 only): no per-LED switch, nothing written at all", async () => {
  const { events, shutdown } = await runScenario(pluginPath, scenario("185-7d88-no-stream"), { vendorId: 0x1462 });
  assert.ok(!events.some((e) => e.op === "send_feature"));
  assert.deepEqual(shutdown, []);
});

test("firmware query: a stale or foreign input report is not taken as the version (7C56 then refused)", async () => {
  const s = scenario("185-7c56-fw6");
  const image = hexToBytes(s.image);
  await withDevice({ productId: 0x7C56, featureReports: { 0x52: image } }, (device, plugin) => {
    const stale = new Array(64).fill(0);
    stale[0] = 0x01;
    stale[1] = 0x99;          // not an answer to 01 B0
    stale[2] = 0x10;
    device.read = (id, length) => { device.calls.push({ method: "read", args: [id, length] }); return stale.slice(); };
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.ok(device.calls.some((c) => c.method === "clearReadBuffer"), "input queue emptied before the query");
    assert.deepEqual(device.sent, []);
  });
});

test("ConflictingProcesses: MSI's lighting services, as in msi-mystic-light-7c91", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.deepEqual(plugin.ConflictingProcesses(), ["LEDKeeper.exe", "LEDKeeper2.exe", "LightKeeperService.exe", "LightKeeperService2.exe"]);
});

test("no 0x52 report the plugin sends ever has the save byte set", async () => {
  for (const s of fixtures.scenarios) {
    const { events, shutdown } = await runScenario(pluginPath, s, { vendorId: 0x1462 });
    for (const e of [...events, ...shutdown]) {
      if (e.op === "send_feature" && e.data.startsWith("52")) {
        assert.ok([185, 162, 112].includes(e.len), `${s.id}: 0x52 length ${e.len}`);
        assert.equal(e.data.slice(-2), "00", `${s.id}: save byte`);
      }
    }
  }
});

// ---------------------------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------------------------
test("MPG Z490 GAMING EDGE WIFI (7C79): JRGB1, JRGB2 and 6 onboard LEDs on the grid, JRAINBOW1/2 as channels", async () => {
  const { device } = await runScenario(pluginPath, scenario("185-7c79-sync"), { vendorId: 0x1462 });
  const call = (m) => device.calls.find((c) => c.method === m);
  assert.deepEqual(call("setName").args, ["MSI MPG Z490 GAMING EDGE WIFI"]);
  assert.deepEqual(call("setSize").args, [[6, 3]]);
  const [names, positions] = call("setControllableLeds").args;
  assert.deepEqual(names, ["JRGB1, LED 1", "JRGB2, LED 1", "ONBOARD, LED 1", "ONBOARD, LED 2", "ONBOARD, LED 3",
    "ONBOARD, LED 4", "ONBOARD, LED 5", "ONBOARD, LED 6"]);
  assert.deepEqual(positions, [[0, 0], [0, 1], [0, 2], [1, 2], [2, 2], [3, 2], [4, 2], [5, 2]]);
  assert.deepEqual(device.channels.map((c) => [c.name, c.ledLimit]), [["JRAINBOW1", 200], ["JRAINBOW2", 240]]);
  assert.deepEqual(call("SetLedLimit").args, [440]);
});

test("MPG X570 GAMING PLUS (7C37, zone-based): the ARGB headers are one-colour zones on the grid, no channels", async () => {
  const { device } = await runScenario(pluginPath, scenario("185-7c37-zone"), { vendorId: 0x1462 });
  const [names] = device.calls.find((c) => c.method === "setControllableLeds").args;
  assert.deepEqual(names.slice(0, 4), ["JRGB1, LED 1", "JRGB2, LED 1", "JRAINBOW1", "JRAINBOW2"]);
  assert.equal(names.length, 10);
  assert.deepEqual(device.channels, []);
});

test("162-byte B450 GAMING PRO CARBON (7B85): five zones, 7 onboard LEDs, OpenRGB's LED names", async () => {
  const { device } = await runScenario(pluginPath, scenario("162-7b85"), { vendorId: 0x1462 });
  const [names, positions] = device.calls.find((c) => c.method === "setControllableLeds").args;
  assert.deepEqual(names.slice(0, 5), ["JRGB1 LED", "JRGB2 LED", "JRAINBOW1 LED", "JCORSAIR LED", "Onboard LEDs LED 1"]);
  assert.equal(names.length, 11);
  assert.deepEqual(positions[10], [6, 4]);
  assert.deepEqual(device.calls.find((c) => c.method === "setSize").args, [[7, 5]]);
});

test("112-byte board: JRGB1, JRAINBOW1, JCORSAIR and 7 onboard LEDs", async () => {
  const { device } = await runScenario(pluginPath, scenario("112-7b16"), { vendorId: 0x1462 });
  const [names] = device.calls.find((c) => c.method === "setControllableLeds").args;
  assert.deepEqual(names, ["JRGB1 LED", "JRAINBOW1 LED", "JCORSAIR LED", ...Array.from({ length: 7 }, (_, i) => `Onboard LEDs LED ${i + 1}`)]);
});

// ---------------------------------------------------------------------------------------------
// Safety
// ---------------------------------------------------------------------------------------------
async function withDevice(options, body) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    await body(device, plugin);
  } finally {
    restore();
  }
}

test("nothing is written when GET_FEATURE 0x52 fails", async () => {
  await withDevice({ productId: 0x7C79 }, (device, plugin) => {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(device.sent, []);
    assert.deepEqual(device.written, []);
  });
});

test("nothing is written when the settings read after the firmware queries comes back short", async () => {
  const image = hexToBytes(scenario("185-7c79-sync").image);
  await withDevice({ productId: 0x7C79, featureReports: { 0x52: image } }, (device, plugin) => {
    let reads = 0;
    const getReport = device.get_report;
    device.get_report = (id, length) => {
      reads += 1;
      return reads === 1 ? getReport(id, length) : (device.setFeatureReport(0x52, image.slice(0, 64)), getReport(id, length));
    };
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.equal(reads, 2);
    assert.deepEqual(device.sent, []);
  });
});

test("an unsupported report length (e.g. the 761-byte boards) is left alone", async () => {
  await withDevice({ productId: 0x7E70, featureReports: { 0x52: new Array(290).fill(0x52) } }, (device, plugin) => {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.deepEqual(device.sent, []);
    assert.deepEqual(device.written, [], "not even the firmware query");
  });
});

test("an unchanged frame is not resent within a second", async () => {
  const s = scenario("185-7c79-sync");
  await withDevice({ productId: 0x7C79, featureReports: { 0x52: hexToBytes(s.image) }, color: [1, 2, 3] }, (device, plugin) => {
    plugin.Initialize();
    plugin.Render();
    const sent = device.sent.length;
    plugin.Render();
    plugin.Render();
    assert.equal(device.sent.length, sent);
  });
});

test("a JRAINBOW channel that grows past 40 LEDs switches to one stream per header (OpenRGB resize)", async () => {
  const { events } = await runScenario(pluginPath, scenario("185-7c79-resize"), { vendorId: 0x1462 });
  const streams = events.filter((e) => e.op === "send_feature" && e.data.startsWith("53"));
  assert.deepEqual(streams.map((e) => e.data.slice(0, 10)), ["5325060000", "5325040000", "5325040100", "5325060000"]);
});

// ---------------------------------------------------------------------------------------------
// Detection and metadata
// ---------------------------------------------------------------------------------------------
test("Validate: only the usage page 0x0001 / usage 0x00 collection", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x00, collection: 0 }), true);
  assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x01, collection: 0 }), false);
  assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x06, collection: 0 }), false);
});

test("ProductId: OpenRGB's 109 compiled 0x1462 detectors minus 0x7C91, same as the manifest", async () => {
  const plugin = await loadPlugin(pluginPath);
  const pids = plugin.ProductId();
  assert.equal(pids.length, 108);
  assert.ok(!pids.includes(0x7C91));
  assert.equal(plugin.VendorId(), 0x1462);
  for (const pid of [0x1720, 0x7B12, 0x7C79, 0x7D86, 0x7E86, 0xB926]) {
    assert.ok(pids.includes(pid), pid.toString(16));
  }
  for (const untested of [0x3EA4, 0x4459, 0x7B10, 0x7B94, 0x7B96, 0x7C42, 0x7C70, 0x7C85, 0x7C88, 0x7C89, 0x7C96, 0x7C99, 0x905D]) {
    assert.ok(!pids.includes(untested), `untested ${untested.toString(16)}`);
  }
  const claim = manifest.devices[0];
  assert.equal(Number(claim.vid), 0x1462);
  assert.deepEqual(claim.pids.map(Number).sort((a, b) => a - b), pids.slice().sort((a, b) => a - b));
  assert.deepEqual(claim.tested, []);
});

test("the test colour helper matches the fixtures' formula", () => {
  assert.deepEqual(testColour(1, 13, 2), [209, 2, 178]);
});
