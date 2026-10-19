// SPDX-License-Identifier: GPL-3.0-or-later
//
// MSI Mystic Light behind vendor id 0x0DB0 / product id 0x0076: the plugin against the HID traffic
// OpenRGB's own code produces (fixtures/openrgb-common.json), the serial-number board lookup, and
// that protocol.js / boards.js are the same files as in plugins/msi-mystic-light.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes } from "../../_testkit/mock-device.mjs";
import { runScenario, expectedEvents } from "../../msi-mystic-light/tests/openrgb-scenario.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const sibling = join(root, "..", "msi-mystic-light");
const pluginPath = join(root, "plugin.js");
const fixtures = JSON.parse(readFileSync(join(root, "fixtures", "openrgb-common.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(root, "manifest.json"), "utf8"));
const scenario = (id) => fixtures.scenarios.find((s) => s.id === id);

for (const s of fixtures.scenarios) {
  test(`OpenRGB scenario ${s.id}: same HID calls, byte for byte`, async () => {
    const { events } = await runScenario(pluginPath, s, { vendorId: 0x0DB0 });
    assert.deepEqual(events, expectedEvents(s), s.note);
  });
}

for (const s of fixtures.scenarios) {
  test(`scenario ${s.id}: Shutdown restores the start-up image (save byte 0) only after a write`, async () => {
    const { events, shutdown } = await runScenario(pluginPath, s, { vendorId: 0x0DB0 });
    if (!events.some((e) => e.op === "send_feature")) {
      assert.deepEqual(shutdown, []);
      return;
    }
    // As read, save byte 0, J_CORSAIR +9 (0x3E) read-back bias of 4 removed (185-byte images).
    const image = hexToBytes(s.image);
    image[image.length - 1] = 0;
    if (image.length === 185 && image[0x3E] >= 4) {
      image[0x3E] -= 4;
    }
    const data = image.map((b) => b.toString(16).padStart(2, "0")).join("").toUpperCase();
    assert.deepEqual(shutdown, [{ op: "send_feature", len: image.length, data }]);
  });
}

test("serial 7E12...: the MAG X670E TOMAHAWK WIFI row (JRGB1, JRGB2, JRAINBOW1/2)", async () => {
  const { device } = await runScenario(pluginPath, scenario("common-7e12"), { vendorId: 0x0DB0 });
  assert.deepEqual(device.calls.find((c) => c.method === "setName").args, ["MSI MAG X670E TOMAHAWK WIFI"]);
  const [names] = device.calls.find((c) => c.method === "setControllableLeds").args;
  assert.deepEqual(names, ["JRGB1, LED 1", "JRGB2, LED 1"]);
  assert.deepEqual(device.channels.map((c) => [c.name, c.ledLimit]), [["JRAINBOW1", 200], ["JRAINBOW2", 240]]);
});

test("serial with no table row: nothing is written", async () => {
  const { events, shutdown } = await runScenario(pluginPath, scenario("common-unknown"), { vendorId: 0x0DB0 });
  assert.ok(!events.some((e) => e.op === "send_feature"));
  assert.deepEqual(shutdown, []);
});

test("Validate: the 0x0001/0x00 and 0xFF00/0x01 collections", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x00, collection: 0 }), true);
  assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x01, collection: 0 }), true);
  assert.equal(plugin.Validate({ interface: 0, usage_page: 0xFF00, usage: 0x00, collection: 0 }), false);
});

test("VendorId 0x0DB0, ProductId [0x0076], same as the manifest", async () => {
  const plugin = await loadPlugin(pluginPath);
  assert.equal(plugin.VendorId(), 0x0DB0);
  assert.deepEqual(plugin.ProductId(), [0x0076]);
  assert.equal(Number(manifest.devices[0].vid), 0x0DB0);
  assert.deepEqual(manifest.devices[0].pids.map(Number), [0x0076]);
  assert.deepEqual(manifest.devices[0].tested, []);
});

test("protocol.js, boards.js and the shared half of plugin.js are the same as in plugins/msi-mystic-light", () => {
  for (const file of ["protocol.js", "boards.js"]) {
    assert.equal(readFileSync(join(root, file), "utf8"), readFileSync(join(sibling, file), "utf8"), file);
  }
  const shared = (path) => readFileSync(path, "utf8").split("(same code below this line).")[1];
  assert.ok(shared(pluginPath).length > 500);
  assert.equal(shared(pluginPath), shared(join(sibling, "plugin.js")));
});

// Two collections, X870 enumerated first: the plugin activates 0x0001/0x00 first and falls back to
// 0xFF00/0x01 only when the first answers GET_FEATURE 0x52 with an unsupported length.
async function withEndpoints(answers, body) {
  const image = hexToBytes(scenario("common-7e12").image);
  const device = createMockDevice({ productId: 0x0076, vendorId: 0x0DB0, featureReports: { 0x52: image } });
  device.getDeviceInfo = () => ({ serialNumber: "7E12A1B2C3D4" });
  const endpoints = [
    { interface: 0, usage: 0x01, usage_page: 0xFF00, collection: 1 },
    { interface: 0, usage: 0x00, usage_page: 0x0001, collection: 0 },
  ];
  let active = endpoints[0];
  const selected = [];
  device.getHidEndpoints = () => endpoints.map((e) => ({ ...e }));
  device.set_endpoint = (iface, usage, page, collection) => {
    active = endpoints.find((e) => e.usage === usage && e.usage_page === page);
    selected.push(`${page.toString(16)}:${usage}`);
  };
  const getReport = device.get_report;
  device.get_report = (id, length) => {
    const answer = answers[`${active.usage_page.toString(16)}:${active.usage}`];
    device.setFeatureReport(0x52, answer === "image" ? image : new Array(answer).fill(0x52));
    return getReport(id, length);
  };
  const restore = installGlobals(device);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    plugin.Render();
    await body(device, selected);
  } finally {
    restore();
  }
}

test("0DB0:0076 with both collections: the Common (0x0001/0x00) collection is tried first", async () => {
  await withEndpoints({ "1:0": "image", "ff00:1": 761 }, (device, selected) => {
    assert.deepEqual(selected, ["1:0"]);
    assert.ok(device.sent.length > 0);
  });
});

test("0DB0:0076: falls back to the X870 collection when Common answers an unsupported length", async () => {
  await withEndpoints({ "1:0": 761, "ff00:1": "image" }, (device, selected) => {
    assert.deepEqual(selected, ["1:0", "ff00:1"]);
    assert.ok(device.sent.length > 0);
  });
});

test("0DB0:0076: neither collection supported, nothing written", async () => {
  await withEndpoints({ "1:0": 761, "ff00:1": 761 }, (device, selected) => {
    assert.deepEqual(selected, ["1:0", "ff00:1"]);
    assert.deepEqual(device.sent, []);
    assert.deepEqual(device.written, []);
  });
});
