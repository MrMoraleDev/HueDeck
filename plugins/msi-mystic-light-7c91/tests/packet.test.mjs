// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-packet tests for the MSI Mystic Light 185 plugin. Run from the repo root with
//   node --test "plugins/**/*.test.mjs"
// (Node >= 21 no longer walks a bare directory argument, so `node --test plugins` fails.)
// The plugin is loaded as a real ES module with a mock `device` global; the expected bytes come
// from fixtures/expected-packet.json, which was produced independently of plugin.js.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const PLUGIN = join(here, "..", "plugin.js");
const fixture = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected-packet.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
const manifestSchema = JSON.parse(readFileSync(join(here, "..", "..", "..", "schemas", "plugin-manifest-v2.schema.json"), "utf8"));
const RESTORE = hexToBytes(fixture.restoreImage);
const REPORT_ID = fixture.reportId;
const LENGTH = fixture.reportLength;

function caseByName(name) {
  const c = fixture.cases.find((entry) => entry.name === name);
  assert.ok(c, `fixture case '${name}' missing`);
  return c;
}

function hexOf(hex) {
  return hexToBytes(hex);
}

// Byte 0x3E (J_CORSAIR record +9): the controller reports the written value + 4 (bench, 2026-10-18).
const CORSAIR_FLAGS = 0x3E;

/** What a GET of 0x52 returns after `sent` was SET: +4 at 0x3E, colour 2 of slot 6 not retained. */
function controllerReadback(sent) {
  const back = Array.from(sent);
  back[CORSAIR_FLAGS] = (back[CORSAIR_FLAGS] + 4) & 0xFF;
  for (let i = 0; i < 3; i++) {
    back[0x7C + 5 + i] = 0;
  }
  return back;
}

/** Boot a fresh plugin instance against a mock device whose 0x52 feature report is `image`. */
async function boot(image, params = {}, color = [0, 0, 0]) {
  const device = createMockDevice({ featureReports: { [REPORT_ID]: image }, color, productId: 0x7C91 });
  const restoreGlobals = installGlobals(device, params);
  const plugin = await loadPlugin(PLUGIN);
  return { device, plugin, restoreGlobals };
}

function assertPacket(actual, expectedHex, label) {
  const expected = hexOf(expectedHex);
  assert.equal(actual.length, LENGTH, `${label}: packet length`);
  for (let i = 0; i < LENGTH; i++) {
    if (actual.bytes[i] !== expected[i]) {
      assert.fail(`${label}: byte 0x${i.toString(16).toUpperCase().padStart(2, "0")} is ` +
                  `0x${actual.bytes[i].toString(16).padStart(2, "0")} expected 0x${expected[i].toString(16).padStart(2, "0")}\n` +
                  `actual:   ${bytesToHex(actual.bytes)}\nexpected: ${bytesToHex(expected)}`);
    }
  }
}

test("manifest.json follows schemas/plugin-manifest-v2 (required keys, no unknown keys, usb claim shape)", () => {
  for (const key of manifestSchema.required) {
    assert.ok(key in manifest, `manifest is missing required key '${key}'`);
  }
  for (const key of Object.keys(manifest)) {
    assert.ok(key in manifestSchema.properties, `manifest key '${key}' is not allowed by the schema (additionalProperties: false)`);
  }
  assert.equal(manifest.manifestVersion, 2);
  assert.match(manifest.id, new RegExp(manifestSchema.properties.id.pattern));
  assert.equal(manifest.id, "huedeck.msi-mystic-light-7c91");
  assert.equal(manifest.license, "GPL-3.0-or-later");
  assert.equal(manifest.compat, "signalrgb-2.5");
  assert.equal(manifest.entry, "plugin.js");
  assert.ok(manifestSchema.properties.permissions.items.$ref.endsWith("permission"));
  for (const p of manifest.permissions) {
    assert.ok(manifestSchema.$defs.permission.enum.includes(p), `unknown permission '${p}'`);
  }
  assert.equal(manifest.devices.length, 1);
  const claim = manifest.devices[0];
  for (const key of manifestSchema.$defs.usbClaim.required) {
    assert.ok(key in claim, `usb claim is missing '${key}'`);
  }
  for (const key of Object.keys(claim)) {
    assert.ok(key in manifestSchema.$defs.usbClaim.properties, `usb claim key '${key}' not allowed`);
  }
});

test("metadata exports match the manifest", async () => {
  const { plugin, restoreGlobals } = await boot(RESTORE);
  try {
    assert.equal(plugin.VendorId(), 0x1462);
    assert.equal(plugin.ProductId(), 0x7C91);
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].pids.map(Number), [plugin.ProductId()]);
    assert.equal(plugin.Type(), "Hid");
    assert.equal(plugin.Type().toLowerCase(), manifest.devices[0].transport);
    assert.equal(plugin.DeviceType(), "motherboard");
    assert.equal(plugin.DeviceType(), manifest.deviceType);
    assert.equal(plugin.Publisher(), "HueDeck");
    assert.equal(plugin.Publisher(), manifest.publisher);
    assert.equal(plugin.Name(), manifest.name);
    assert.equal(plugin.Version(), manifest.version);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    assert.equal(plugin.LedNames().length, fixture.onboardZoneOffsets.length);
    const [w, h] = plugin.Size();
    for (const [x, y] of plugin.LedPositions()) {
      assert.ok(x >= 0 && x < w && y >= 0 && y < h, "LedPositions inside Size");
    }
    const props = plugin.ControllableParameters().map((p) => p.property);
    assert.deepEqual(props, ["LightingMode", "forcedColor", "shutdownColor"]);
  } finally {
    restoreGlobals();
  }
});

test("Validate accepts the single HID interface (0, or -1 for a path without &MI_xx) only", async () => {
  const { plugin, restoreGlobals } = await boot(RESTORE);
  try {
    assert.equal(plugin.Validate({ interface: 0, usage: 0, usage_page: 1, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: -1, usage: 0, usage_page: 1, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage: 0, usage_page: 1, collection: 0 }), false);
    assert.equal(plugin.Validate({ interface: 2, usage: 0, usage_page: 0xFF00, collection: 0 }), false);
  } finally {
    restoreGlobals();
  }
});

test("Render sends exactly one 185-byte 0x52 packet: restore image + static red onboard zones, save flag cleared", async () => {
  const c = caseByName("solid red via device.color");
  const { device, plugin, restoreGlobals } = await boot(RESTORE, { LightingMode: c.lightingMode }, hexOf(c.color));
  try {
    plugin.Initialize();
    assert.equal(device.sent.length, 0, "Initialize must not write");
    assert.deepEqual(device.calls.filter((x) => x.method === "get_report").map((x) => x.args), [[[REPORT_ID], LENGTH]]);

    plugin.Render();
    assert.equal(device.sent.length, 1, "one feature report per Render");
    assert.equal(device.sent[0].reportId, REPORT_ID);
    assertPacket(device.sent[0], c.expectedPacket, c.name);

    // Only the six onboard records (9 bytes each) and the save flag differ from the restore image.
    const touched = new Set();
    for (const off of fixture.onboardZoneOffsets) {
      for (let i = 0; i < 9; i++) touched.add(off + i);
    }
    touched.add(0xB8);
    touched.add(CORSAIR_FLAGS);
    assert.equal(device.sent[0].bytes[CORSAIR_FLAGS], RESTORE[CORSAIR_FLAGS] - 4, "0x3E is written as read - 4");
    for (let i = 0; i < LENGTH; i++) {
      if (!touched.has(i)) {
        assert.equal(device.sent[0].bytes[i], RESTORE[i], `byte 0x${i.toString(16)} must be preserved from the restore image`);
      }
    }
  } finally {
    restoreGlobals();
  }
});

test("Render skips an unchanged packet within the keepalive window and sends a change at once", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, {}, [255, 0, 0]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Render();
    plugin.Render();
    assert.equal(device.sent.length, 1, "an identical packet is not resent within 1 s");
    device.setColor([0, 0, 255]);
    plugin.Render();
    assert.equal(device.sent.length, 2, "a colour change is sent on the next Render");
    assert.equal(device.sent[1].bytes[0x4A + 3], 0xFF, "blue byte of onboard LED 1");
    // A re-Initialize (resume, re-plug) always writes on the next Render.
    plugin.Initialize();
    plugin.Render();
    assert.equal(device.sent.length, 3);
  } finally {
    restoreGlobals();
  }
});

test("Forced lighting mode ignores the canvas and paints forcedColor", async () => {
  const c = caseByName("forced colour #00FF00");
  const { device, plugin, restoreGlobals } = await boot(RESTORE, { LightingMode: c.lightingMode, forcedColor: c.forcedColor }, hexOf(c.color));
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(device.sent.length, 1);
    assertPacket(device.sent[0], c.expectedPacket, c.name);
    assert.equal(device.calls.filter((x) => x.method === "color").length, 0, "device.color not sampled in Forced mode");
  } finally {
    restoreGlobals();
  }
});

test("Shutdown(false) writes the restore image back with the save flag cleared", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, {}, [255, 0, 0]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.equal(device.sent.length, 2);
    assertPacket(device.sent[1], fixture.expectedRestorePacket, "restore");
  } finally {
    restoreGlobals();
  }
});

test("Shutdown(true) paints shutdownColor (default black) on the onboard zones", async () => {
  const c = caseByName("suspend paints shutdownColor (default black)");
  const { device, plugin, restoreGlobals } = await boot(RESTORE, { shutdownColor: c.shutdownColor }, [255, 255, 255]);
  try {
    plugin.Initialize();
    plugin.Shutdown(true);
    assert.equal(device.sent.length, 1);
    assertPacket(device.sent[0], c.expectedPacket, c.name);
  } finally {
    restoreGlobals();
  }
});

test("re-Initialize after suspend keeps the original restore image when the board still holds our packet", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, { shutdownColor: "#112233" }, [255, 0, 0]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);                     // suspend: paints #112233
    assert.equal(device.sent.length, 2);
    // The controller now reports the packet we just wrote (with its read-back quirks); the host
    // re-runs Initialize on resume.
    device.setFeatureReport(REPORT_ID, controllerReadback(device.sent[1].bytes));
    plugin.Initialize();
    plugin.Shutdown(false);
    assert.equal(device.sent.length, 3);
    assertPacket(device.sent[2], fixture.expectedRestorePacket, "restore after resume");
  } finally {
    restoreGlobals();
  }
});

test("re-Initialize adopts a fresh image when someone else changed the controller in between", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, {}, [255, 0, 0]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(true);
    const foreign = RESTORE.slice();
    foreign[0x01] = 0x02;                      // JRGB1 effect changed by another application
    foreign[0xB8] = 0x01;
    device.setFeatureReport(REPORT_ID, foreign);
    plugin.Initialize();
    plugin.Shutdown(false);
    const expected = foreign.slice();
    expected[0xB8] = 0x00;
    expected[CORSAIR_FLAGS] = foreign[CORSAIR_FLAGS] - 4;
    assertPacket(device.sent[2], bytesToHex(expected), "restore of foreign image");
  } finally {
    restoreGlobals();
  }
});

test("a get_report whose array is shorter than getLastReadSize claims is rejected", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, {}, [255, 0, 0]);
  try {
    const original = device.get_report;
    device.get_report = (ids, len) => { original(ids, len); return RESTORE.slice(0, 100); };
    plugin.Initialize();
    plugin.Render();
    assert.equal(device.sent.length, 0, "no writes from an inconsistent read");
  } finally {
    restoreGlobals();
  }
});

test("a get_report that is not 185 bytes disables every write and is logged", async () => {
  const short = RESTORE.slice(0, 162);
  const { device, plugin, restoreGlobals } = await boot(short, {}, [255, 0, 0]);
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    plugin.Shutdown(true);
    assert.equal(device.sent.length, 0, "no writes without a valid restore image");
    assert.equal(device.written.length, 0);
    assert.ok(device.logs.some((l) => /162 bytes/.test(l.message) && l.options.toFile === true), "length mismatch logged to file");
  } finally {
    restoreGlobals();
  }
});

test("a 185-byte image with the wrong report id is rejected too", async () => {
  const wrong = RESTORE.slice();
  wrong[0] = 0x53;
  const { device, plugin, restoreGlobals } = await boot(wrong, {}, [255, 0, 0]);
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(device.sent.length, 0);
  } finally {
    restoreGlobals();
  }
});

test("JRAINBOW/JRGB headers are untouched and no channels are created in M1", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, {}, [1, 2, 3]);
  try {
    plugin.Initialize();
    plugin.Render();
    assert.equal(device.channels.length, 0, "addChannel is an M2 item");
    const p = device.sent[0].bytes;
    const untouchedRanges = [[0x01, 0x0B], [0x0B, 0x15], [0x15, 0x1F], [0x1F, 0x2A], [0x2A, 0x35], [0x35, 0x40], [0x40, 0x4A], [0x86, 0xB8]];
    for (const [from, to] of untouchedRanges) {
      for (let i = from; i < to; i++) {
        const want = i === CORSAIR_FLAGS ? RESTORE[i] - 4 : RESTORE[i];
        assert.equal(p[i], want, `header/unused byte 0x${i.toString(16)} preserved`);
      }
    }
  } finally {
    restoreGlobals();
  }
});

test("a GET of our own packet is recognised despite the controller's read-back quirks", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, {}, [10, 20, 30]);
  try {
    plugin.Initialize();
    plugin.Render();
    // A re-Initialize that reads back exactly what the board reports after our SET keeps the
    // original restore image; a byte-exact comparison would have adopted our own packet.
    device.setFeatureReport(REPORT_ID, controllerReadback(device.sent[0].bytes));
    plugin.Initialize();
    plugin.Shutdown(false);
    assertPacket(device.sent[device.sent.length - 1], fixture.expectedRestorePacket, "restore after re-Initialize");
  } finally {
    restoreGlobals();
  }
});

test("repeated sessions keep 0x3E stable on a controller that adds 4 on read-back", async () => {
  let image = RESTORE.slice();
  for (let session = 0; session < 3; session++) {
    const { device, plugin, restoreGlobals } = await boot(image, {}, [255, 0, 0]);
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(false);
      image = controllerReadback(device.sent[device.sent.length - 1].bytes);
    } finally {
      restoreGlobals();
    }
  }
  assert.equal(image[CORSAIR_FLAGS], RESTORE[CORSAIR_FLAGS], "no drift across sessions");
});

test("Shutdown(false) writes nothing when the session never wrote", async () => {
  const { device, plugin, restoreGlobals } = await boot(RESTORE, {}, [255, 0, 0]);
  try {
    plugin.Initialize();
    plugin.Shutdown(false);                    // e.g. Ctrl-C before the first Render
    assert.equal(device.sent.length, 0);
  } finally {
    restoreGlobals();
  }
});
