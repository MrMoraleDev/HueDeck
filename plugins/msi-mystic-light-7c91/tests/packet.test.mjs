// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tests for the MSI Mystic Light 185 plugin in per-LED mode. Run from the repo root with
//   node --test "plugins/**/*.test.mjs"
// The plugin is loaded as a real ES module with a mock `device` global. Configuration, reset and
// EZ LED packets are compared with what was recorded on the wire (fixtures/per-led.json); the
// expected colour streams are built here from the documented slot order, not by the plugin.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const PLUGIN = join(here, "..", "plugin.js");
const fixture = JSON.parse(readFileSync(join(here, "..", "fixtures", "per-led.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
const manifestSchema = JSON.parse(readFileSync(join(here, "..", "..", "..", "schemas", "plugin-manifest-v2.schema.json"), "utf8"));

const RESTORE = hexToBytes(fixture.restoreImage);
const CONFIG_ID = 0x52;
const STREAM_ID = 0x53;
const HEADER_1 = "12v RGB Header 1";
const HEADER_2 = "12v RGB Header 2";
const STRIP_1 = "JRainbow 1";
const STRIP_2 = "JRainbow 2";

/** `count` LEDs whose colours name the strip and the LED: [led, tag, 255 - led]. */
function strip(count, tag) {
  return Array.from({ length: count }, (_, led) => [led & 0xFF, tag, (255 - led) & 0xFF]);
}

/** Boot a fresh plugin instance against a mock board. */
async function boot({ image = RESTORE, params = {}, color = [0, 0, 0], strips = [60, 60], headers = {}, ezReply } = {}) {
  const device = createMockDevice({
    featureReports: { [CONFIG_ID]: image },
    inputReports: ezReply ? { 1: ezReply } : {},
    color,
    productId: 0x7C91,
    channelColors: { [STRIP_1]: strip(strips[0], 0x11), [STRIP_2]: strip(strips[1], 0x22) },
    subdeviceColors: headers,
  });
  const restoreGlobals = installGlobals(device, params);
  const plugin = await loadPlugin(PLUGIN);
  return { device, plugin, restoreGlobals };
}

/** The 0x53 report for these slot colours, built from the documented layout. */
function expectedStream(slots) {
  const bytes = new Array(725).fill(0);
  bytes.splice(0, 5, 0x53, 0x25, 0x06, 0x00, 0x00);
  slots.forEach((rgb, slot) => { bytes.splice(5 + 3 * slot, 3, ...rgb); });
  return bytes;
}

/** A config packet from the recorded one with other LED counts at 0x29 and 0x34. */
function configWithCounts(first, second) {
  const bytes = hexToBytes(fixture.config_60_60);
  bytes[0x29] = first;
  bytes[0x34] = second;
  return bytes;
}

function assertBytes(actual, expected, label) {
  assert.equal(actual.length, expected.length, `${label}: length`);
  const at = actual.findIndex((b, i) => b !== expected[i]);
  if (at >= 0) {
    assert.fail(`${label}: byte 0x${at.toString(16)} is 0x${actual[at].toString(16)}, expected 0x${expected[at].toString(16)}\n` +
                `actual:   ${bytesToHex(actual)}\nexpected: ${bytesToHex(expected)}`);
  }
}

const sentWith = (device, id) => device.sent.filter((s) => s.reportId === id);

test("manifest.json follows schemas/plugin-manifest-v2 (required keys, no unknown keys, usb claim shape)", () => {
  for (const key of manifestSchema.required) {
    assert.ok(key in manifest, `manifest is missing required key '${key}'`);
  }
  for (const key of Object.keys(manifest)) {
    assert.ok(key in manifestSchema.properties, `manifest key '${key}' is not allowed by the schema (additionalProperties: false)`);
  }
  assert.equal(manifest.manifestVersion, 2);
  assert.match(manifest.id, new RegExp(manifestSchema.properties.id.pattern));
  assert.equal(manifest.id, "lightloom.msi-mystic-light-7c91");
  assert.equal(manifest.license, "GPL-3.0-or-later");
  assert.equal(manifest.compat, "signalrgb-2.5");
  assert.equal(manifest.entry, "plugin.js");
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
  const { plugin, restoreGlobals } = await boot();
  try {
    assert.equal(plugin.VendorId(), Number(manifest.devices[0].vid));
    assert.deepEqual(manifest.devices[0].pids.map(Number), [plugin.ProductId()]);
    assert.equal(plugin.Type().toLowerCase(), manifest.devices[0].transport);
    assert.equal(plugin.DeviceType(), manifest.deviceType);
    assert.equal(plugin.Publisher(), manifest.publisher);
    assert.equal(plugin.Name(), manifest.name);
    assert.equal(plugin.Version(), manifest.version);
    assert.deepEqual(plugin.ConflictingProcesses(), manifest.conflictingProcesses);
    assert.equal(plugin.LedNames().length, 6);
    assert.equal(plugin.LedNames().length, plugin.LedPositions().length);
    const [w, h] = plugin.Size();
    for (const [x, y] of plugin.LedPositions()) {
      assert.ok(x >= 0 && x < w && y >= 0 && y < h, "LedPositions inside Size");
    }
    assert.deepEqual(plugin.ControllableParameters().map((p) => p.property), ["LightingMode", "forcedColor", "shutdownColor"]);
  } finally {
    restoreGlobals();
  }
});

test("Validate accepts the single HID interface (0, or -1 for a path without &MI_xx) only", async () => {
  const { plugin, restoreGlobals } = await boot();
  try {
    assert.equal(plugin.Validate({ interface: 0, usage: 0, usage_page: 1, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: -1, usage: 0, usage_page: 1, collection: 0 }), true);
    assert.equal(plugin.Validate({ interface: 1, usage: 0, usage_page: 1, collection: 0 }), false);
  } finally {
    restoreGlobals();
  }
});

test("Initialize reads 0x52 first, then sends the recorded reset, per-LED configuration and EZ LED query", async () => {
  const { device, plugin, restoreGlobals } = await boot({ ezReply: hexToBytes(fixture.ezLedReplyOn) });
  try {
    plugin.Initialize();
    const io = device.calls.filter((c) => ["get_report", "send_report", "write", "read", "clearReadBuffer"].includes(c.method));
    assert.deepEqual(io.map((c) => c.method), ["get_report", "send_report", "send_report", "clearReadBuffer", "write", "read"]);
    assertBytes(device.sent[0].bytes, hexToBytes(fixture.reset), "reset");
    assertBytes(device.sent[1].bytes, hexToBytes(fixture.config_60_60), "configuration");
    assertBytes(device.written[0].bytes, hexToBytes(fixture.ezLedQuery), "EZ LED query");
    assert.ok(!device.logs.some((l) => l.message.includes("EZ LED")), "EZ LED on: nothing to report");
    assert.deepEqual(device.subdevices.map((s) => [s.id, s.size]), [[HEADER_1, [1, 1]], [HEADER_2, [1, 1]]]);
    assert.deepEqual(device.channels.map((c) => [c.name, c.ledLimit]), [[STRIP_1, 120], [STRIP_2, 120]]);
    assert.ok(device.calls.some((c) => c.method === "SetLedLimit" && c.args[0] === 232));
  } finally {
    restoreGlobals();
  }
});

test("the configuration carries each strip's LED count: at least 1, at most 120, both within 232", async () => {
  for (const [strips, counts] of [[[0, 0], [1, 1]], [[60, 60], [60, 60]], [[120, 120], [120, 112]], [[5, 200], [5, 120]]]) {
    const { device, plugin, restoreGlobals } = await boot({ strips });
    try {
      plugin.Initialize();
      const expected = counts[0] === 1 && counts[1] === 1 ? hexToBytes(fixture.config_1_1) : configWithCounts(...counts);
      assertBytes(sentWith(device, CONFIG_ID)[0].bytes, expected, `configuration for ${strips}`);
    } finally {
      restoreGlobals();
    }
  }
});

test("a frame streams the onboard LEDs, JRGB1, JRGB2, then the JRAINBOW1 and JRAINBOW2 LEDs", async () => {
  const { device, plugin, restoreGlobals } = await boot({
    strips: [3, 2],
    color: (x) => [x + 1, 0x10, 0x20],
    headers: { [HEADER_1]: [0xA1, 0xA2, 0xA3], [HEADER_2]: [0xB1, 0xB2, 0xB3] },
  });
  try {
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Render();
    const slots = [
      ...[0, 1, 2, 3, 4, 5].map((x) => [x + 1, 0x10, 0x20]),
      [0xA1, 0xA2, 0xA3], [0xB1, 0xB2, 0xB3],
      ...strip(3, 0x11), ...strip(2, 0x22),
    ];
    assert.equal(device.sent.length, 1);
    assertBytes(device.sent[0].bytes, expectedStream(slots), "stream");
    assert.equal(device.sent[0].length, 725);
  } finally {
    restoreGlobals();
  }
});

test("an unchanged frame is skipped within the keepalive window; a change is sent at once", async () => {
  const { device, plugin, restoreGlobals } = await boot({ color: [1, 2, 3] });
  try {
    plugin.Initialize();
    plugin.Render();
    const before = sentWith(device, STREAM_ID).length;
    plugin.Render();
    assert.equal(sentWith(device, STREAM_ID).length, before, "unchanged frame skipped");
    device.setColor([4, 5, 6]);
    plugin.Render();
    assert.equal(sentWith(device, STREAM_ID).length, before + 1, "changed frame sent");
  } finally {
    restoreGlobals();
  }
});

test("Forced lighting mode paints forcedColor on every LED, strips included", async () => {
  const { device, plugin, restoreGlobals } = await boot({
    strips: [4, 1], params: { LightingMode: "Forced", forcedColor: "#00FF00" }, color: [9, 9, 9],
  });
  try {
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Render();
    assertBytes(device.sent[0].bytes, expectedStream(new Array(6 + 2 + 4 + 1).fill([0, 255, 0])), "forced stream");
  } finally {
    restoreGlobals();
  }
});

test("a changed LED count reconfigures before the next frame", async () => {
  const { device, plugin, restoreGlobals } = await boot({ strips: [60, 60] });
  try {
    plugin.Initialize();
    plugin.Render();
    device.sent.length = 0;
    device.setChannelColors(STRIP_2, strip(30, 0x22));
    plugin.Render();
    assert.deepEqual(device.sent.map((s) => s.reportId), [STREAM_ID, CONFIG_ID, STREAM_ID]);
    assertBytes(device.sent[0].bytes, hexToBytes(fixture.reset), "reset");
    assertBytes(device.sent[1].bytes, configWithCounts(60, 30), "new configuration");
  } finally {
    restoreGlobals();
  }
});

test("Shutdown(false) blanks the stream and writes the restore image back with the save flag cleared", async () => {
  const { device, plugin, restoreGlobals } = await boot({ color: [255, 0, 0] });
  try {
    plugin.Initialize();
    plugin.Render();
    device.sent.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(device.sent.map((s) => s.reportId), [STREAM_ID, CONFIG_ID]);
    assertBytes(device.sent[0].bytes, expectedStream([]), "black stream");
    assertBytes(device.sent[1].bytes, hexToBytes(fixture.expectedRestorePacket), "restore packet");
  } finally {
    restoreGlobals();
  }
});

test("Shutdown(true) paints shutdownColor (default black) on every LED", async () => {
  const { device, plugin, restoreGlobals } = await boot({ strips: [2, 2], color: [7, 7, 7], params: { shutdownColor: "#102030" } });
  try {
    plugin.Initialize();
    plugin.Render();
    device.sent.length = 0;
    plugin.Shutdown(true);
    assert.equal(device.sent.length, 1);
    assertBytes(device.sent[0].bytes, expectedStream(new Array(6 + 2 + 2 + 2).fill([0x10, 0x20, 0x30])), "suspend stream");
  } finally {
    restoreGlobals();
  }
});

test("re-Initialize after suspend keeps the original restore image when the board reads back our configuration", async () => {
  const { device, plugin, restoreGlobals } = await boot();
  try {
    plugin.Initialize();
    plugin.Shutdown(true);
    // What the real board answered after this configuration: +4 at 0x3E, 0x87 for 0xBF at 0x52.
    device.setFeatureReport(CONFIG_ID, hexToBytes(fixture.boardReadBack));
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Shutdown(false);
    assertBytes(sentWith(device, CONFIG_ID)[0].bytes, hexToBytes(fixture.expectedRestorePacket), "restore after resume");
  } finally {
    restoreGlobals();
  }
});

test("re-Initialize adopts a fresh image when someone else changed the controller in between", async () => {
  const { device, plugin, restoreGlobals } = await boot();
  try {
    plugin.Initialize();
    const other = hexToBytes(fixture.boardReadBack);
    other[0x4A] = 0x01;                     // another application set the master zone to static
    device.setFeatureReport(CONFIG_ID, other);
    plugin.Initialize();
    device.sent.length = 0;
    plugin.Shutdown(false);
    const expected = other.slice();
    expected[0x3E] -= 4;
    assertBytes(sentWith(device, CONFIG_ID)[0].bytes, expected, "restores the newer image");
  } finally {
    restoreGlobals();
  }
});

test("a GET that is not a full 185-byte 0x52 report disables every write and is logged", async () => {
  const short = RESTORE.slice(0, 64);
  const wrongId = RESTORE.slice();
  wrongId[0] = 0x53;
  for (const image of [short, wrongId]) {
    const { device, plugin, restoreGlobals } = await boot({ image });
    try {
      plugin.Initialize();
      plugin.Render();
      plugin.Shutdown(true);
      plugin.Shutdown(false);
      assert.equal(device.sent.length, 0);
      assert.equal(device.written.length, 0);
      assert.ok(device.logs.some((l) => l.message.includes("all writes disabled")));
    } finally {
      restoreGlobals();
    }
  }
});

test("EZ LED Control off in the BIOS is reported", async () => {
  const reply = hexToBytes(fixture.ezLedReplyOn);
  reply[6] = 0x00;
  const { device, plugin, restoreGlobals } = await boot({ ezReply: reply });
  try {
    plugin.Initialize();
    assert.ok(device.logs.some((l) => l.message.includes("EZ LED Control is off")));
  } finally {
    restoreGlobals();
  }
});

test("no 0x52 packet ever sets the save flag", async () => {
  const image = RESTORE.slice();
  const { device, plugin, restoreGlobals } = await boot({ image, color: (x) => [x, x, x] });
  try {
    plugin.Initialize();
    for (let i = 0; i < 3; i++) plugin.Render();
    device.setChannelColors(STRIP_1, strip(10, 0x11));
    plugin.Render();
    plugin.Shutdown(true);
    plugin.Shutdown(false);
    const configs = sentWith(device, CONFIG_ID);
    assert.ok(configs.length >= 3);
    assert.ok(configs.every((s) => s.bytes[0xB8] === 0x00));
  } finally {
    restoreGlobals();
  }
});

// --- Lightloom extensions: "Save to devices" and the exit modes -----------------------------------

/**
 * The static (or breathing) configuration, built here from the documented record layout
 * (docs/hardware/msi-mystic-light-185.md, "Save to devices"): the restore packet with this board's
 * zone records set to [effect, R, G, B, options, R, G, B, flags, 0], the strips' LED counts at +10
 * of the JRAINBOW records, and the save byte.
 */
function staticPacket(zones, { effect = 0x01, options = 0x28, save = 0, counts = [60, 60] } = {}) {
  const bytes = hexToBytes(fixture.expectedRestorePacket);
  const record = (offset, rgb, flags) => bytes.splice(offset, 10, effect, ...rgb, options, ...rgb, flags, 0x00);
  const [onboard, jrgb1, jrgb2, jr1, jr2] = zones;
  record(0x01, jrgb1, 0x80);
  record(0xAE, jrgb2, 0x80);
  record(0x1F, jr1, 0x80);
  record(0x2A, jr2, 0x80);
  bytes[0x29] = counts[0];
  bytes[0x34] = counts[1];
  record(0x4A, onboard, 0x81);
  for (let slot = 1; slot <= 9; slot++) record(0x4A + 10 * slot, onboard, 0x80);
  bytes[0xB8] = save;
  return bytes;
}

const avg = (leds) => [0, 1, 2].map((k) => Math.round(leds.reduce((s, c) => s + c[k], 0) / leds.length));
const HEADERS = { [HEADER_1]: [1, 2, 3], [HEADER_2]: [4, 5, 6] };
const ZONES = [[0x40, 0x50, 0x60], [1, 2, 3], [4, 5, 6], avg(strip(60, 0x11)), avg(strip(60, 0x22))];

test("SaveToDevice: OpenRGB's Update(true) sequence, then per-LED mode again", async () => {
  const { device, plugin, restoreGlobals } = await boot({ color: [0x40, 0x50, 0x60], headers: HEADERS });
  try {
    plugin.Initialize();
    plugin.Render();
    device.sent.length = 0;
    const result = plugin.SaveToDevice({ action: "save", effect: null, colors: new Array(6).fill([0x40, 0x50, 0x60]) });
    assert.deepEqual(result.zones.map((z) => z.name), ["Onboard LEDs", "JRGB1", "JRGB2", "JRAINBOW1", "JRAINBOW2"]);
    assert.equal(result.zones[0].color, "#405060");
    assert.deepEqual(device.sent.map((s) => s.reportId), [CONFIG_ID, CONFIG_ID, STREAM_ID, CONFIG_ID]);
    const before = hexToBytes(fixture.expectedRestorePacket);
    before[0xB8] = 0x01;
    assertBytes(device.sent[0].bytes, before, "the board's own state, save byte set");
    assertBytes(device.sent[1].bytes, staticPacket(ZONES, { save: 1 }), "the static configuration, save byte set");
    assertBytes(device.sent[2].bytes, hexToBytes(fixture.reset), "reset");
    assertBytes(device.sent[3].bytes, configWithCounts(60, 60), "per-LED mode again");
    // The next frame goes out, and the restore on exit puts back what the board now holds.
    device.sent.length = 0;
    plugin.Render();
    assert.deepEqual(device.sent.map((s) => s.reportId), [STREAM_ID]);
  } finally {
    restoreGlobals();
  }
});

test("SaveToDevice: breathing, previews and refusals", async () => {
  const { device, plugin, restoreGlobals } = await boot({ color: [0x40, 0x50, 0x60], headers: HEADERS });
  try {
    plugin.Initialize();
    device.sent.length = 0;
    const preview = plugin.SaveToDevice({ action: "preview", effect: null, colors: new Array(6).fill([0x40, 0x50, 0x60]) });
    assert.equal(preview.zones.length, 5);
    assert.equal(device.sent.length, 0, "a preview sends nothing");
    assert.ok(plugin.SaveToDevice({ action: "save", effect: "spectrum", colors: [] }).unsupported);
    assert.equal(device.sent.length, 0);
    plugin.SaveToDevice({ action: "save", effect: "breathing", colors: new Array(6).fill([0x40, 0x50, 0x60]) });
    assertBytes(device.sent[1].bytes, staticPacket(ZONES, { effect: 0x02, options: 0x29, save: 1 }), "breathing");
    assert.deepEqual(plugin.SaveCapabilities(), { static: true, effects: ["breathing"] });
  } finally {
    restoreGlobals();
  }
  const { device: d2, plugin: p2, restoreGlobals: r2 } = await boot({ image: RESTORE.slice(0, 64) });
  try {
    p2.Initialize();
    assert.ok(p2.SaveToDevice({ action: "save", effect: null, colors: [] }).unsupported);
    assert.equal(d2.sent.length, 0, "no restore image: nothing at all");
  } finally {
    r2();
  }
});

test("exit modes keep and off: a static configuration with the last colours or black, save byte 0", async () => {
  for (const mode of ["keep", "off"]) {
    const { device, plugin, restoreGlobals } = await boot({ color: [0x40, 0x50, 0x60], headers: HEADERS });
    try {
      plugin.Initialize();
      plugin.Render();
      device.sent.length = 0;
      plugin.Shutdown(false, mode);
      assert.deepEqual(device.sent.map((s) => s.reportId), [STREAM_ID, CONFIG_ID]);
      assertBytes(device.sent[0].bytes, expectedStream([]), "black stream");
      const zones = mode === "keep" ? ZONES : new Array(5).fill([0, 0, 0]);
      assertBytes(device.sent[1].bytes, staticPacket(zones), `${mode} configuration`);
    } finally {
      restoreGlobals();
    }
  }
});

test("the save byte is set only by SaveToDevice({action: 'save'})", async () => {
  const { device, plugin, restoreGlobals } = await boot({ color: [9, 9, 9] });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.SaveToDevice({ action: "preview", effect: null, colors: [] });
    plugin.SaveToDevice({ action: "preview", effect: "breathing", colors: [] });
    for (const mode of [undefined, "restore", "keep", "off"]) {
      plugin.Shutdown(false, mode);
    }
    plugin.Shutdown(true);
    const configs = sentWith(device, CONFIG_ID);
    assert.ok(configs.length >= 4);
    assert.ok(configs.every((s) => s.bytes[0xB8] === 0x00));
  } finally {
    restoreGlobals();
  }
});
