// SPDX-License-Identifier: GPL-3.0-or-later
//
// Razer family plugin: the packets of Initialize / Render / Shutdown against the ones worked out
// from OpenRGB's C++ code (fixtures/openrgb-packets.json), the DeathAdder V2 Pro against HueDeck's
// own recording (fixtures/deathadder-v2-pro-recorded.json), Validate against the detector table,
// geometry, keyboard layouts, paired devices, the ARGB controller and the Kraken V4 family.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";
import { DETECTORS, DEVICES } from "../devices.js";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const fixtures = JSON.parse(readFileSync(join(here, "..", "fixtures", "openrgb-packets.json"), "utf8"));
const recorded = JSON.parse(readFileSync(join(here, "..", "fixtures", "deathadder-v2-pro-recorded.json"), "utf8"));
const hex = (h) => bytesToHex(hexToBytes(h));

/** The test canvas (the fixture generator uses the same rule). */
const canvas = (x, y) => [(x * 10 + 1) & 0xFF, (y * 20 + 2) & 0xFF, (x * 3 + y * 7 + 5) & 0xFF];

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

/** Everything the plugin sent (feature reports and output reports) since the last call. */
function drain(device) {
  const out = [...device.sent, ...device.written].map((s) => bytesToHex(s.bytes));
  device.sent.length = 0;
  device.written.length = 0;
  return out;
}

/** OpenRGB's addressable zone-size report: command class 0x0F, id 0x08 (bytes 7 and 8). */
function isSizeReport(spacedHex) {
  const b = spacedHex.split(" ");
  return b.length === 91 && b[7].toUpperCase() === "0F" && b[8] === "08";
}

function phases(plugin, device) {
  plugin.Initialize();
  const initialize = drain(device);
  plugin.Render();
  const frame = drain(device);
  plugin.Shutdown(false);
  const shutdown = drain(device);
  return { initialize, frame, shutdown };
}

for (const c of fixtures.razer) {
  test(`${c.name} (${c.pid}): ${c.note}`, async () => {
    const { device, plugin, restore } = await setup({ productId: Number(c.pid), color: canvas });
    try {
      const got = phases(plugin, device);
      assert.deepEqual(got.initialize, c.initialize.map(hex), "initialize");
      assert.deepEqual(got.frame, c.frame.map(hex), "frame");
      assert.deepEqual(got.shutdown, c.shutdown.map(hex), "shutdown");
      assert.ok(device.sent.every((s) => s.length === 91));
    } finally {
      restore();
    }
  });
}

test("a receiver's paired device gets its own subdevice, transaction id and relayed-command polling", async () => {
  const c = fixtures.paired;
  const replies = (sent) => {
    const reply = sent.slice();
    reply[1] = 0x02;
    if (sent[7] === 0x00 && sent[8] === 0xBF) {
      // two slots: the dock itself (0x00A4) and a Basilisk V3 Pro (Wireless) 0x00AB, then an empty one
      const slots = [3, 0x01, 0x00, 0xA4, 0x01, 0x00, 0xAB, 0x00, 0xFF, 0xFF];
      for (let i = 0; i < slots.length; i++) reply[9 + i] = slots[i];
    }
    return reply;
  };
  const { device, plugin, restore } = await setup({
    productId: Number(c.pid), color: canvas, featureReply: replies,
    subdeviceColors: {},
  });
  try {
    // subdeviceColor() answers black unless told otherwise: give the paired mouse the canvas too
    const colourOf = device.subdeviceColor.bind(device);
    device.subdeviceColor = (id, x, y) => { colourOf(id, x, y); return canvas(x, y); };
    const got = phases(plugin, device);
    assert.deepEqual(got.initialize, c.initialize.map(hex), "initialize");
    assert.deepEqual(got.frame, c.frame.map(hex), "frame");
    assert.deepEqual(got.shutdown, c.shutdown.map(hex), "shutdown");
    assert.deepEqual(device.subdevices.map((s) => s.id), ["paired-00AB"]);
    const sub = device.subdevices[0];
    assert.equal(sub.name, "Razer Basilisk V3 Pro (Wireless) (Razer Mouse Dock Pro)");
    assert.deepEqual(sub.size, [11, 3]);
    assert.deepEqual(sub.leds.names.slice(0, 3), ["Scroll Wheel", "Logo", "LED Strip LED 1"]);
    // every command to the paired mouse is followed by a read of the receiver's answer
    const reads = device.calls.filter((call) => call.method === "get_report").length;
    assert.ok(reads > 0);
  } finally {
    restore();
  }
});

test("Chroma Addressable RGB Controller: channels, 321-byte frames on interface 1, no zone-size report unless resized", async () => {
  const c = fixtures.argb;
  const { device, plugin, restore } = await setup({
    productId: 0x0F1F,
    channelColors: { "Channel 1": [[1, 2, 3], [4, 5, 6], [7, 8, 9]], "Channel 2": [[10, 11, 12], [13, 14, 15]] },
  });
  try {
    const endpoints = [
      { interface: 0, usage: 0x0002, usage_page: 0x0001, collection: 0 },
      { interface: 1, usage: 0x0000, usage_page: 0x000C, collection: 0 },
    ];
    device.getHidEndpoints = () => endpoints;
    // Record the active interface with every feature report.
    let active = null;
    const onInterface = [];
    device.set_endpoint = (iface) => { active = iface; return true; };
    const sendReport = device.send_report.bind(device);
    device.send_report = (bytes, length) => { onInterface.push([length, active]); return sendReport(bytes, length); };
    const got = phases(plugin, device);
    assert.deepEqual(got.initialize, c.initialize.map(hex), "initialize");
    assert.deepEqual(got.frame, c.frame.map(hex), "frame");
    assert.deepEqual(got.shutdown, c.shutdown.map(hex), "shutdown");
    assert.ok(!got.frame.some(isSizeReport), "no zone-size report on the first frame");
    assert.deepEqual(device.channels.map((ch) => [ch.name, ch.ledLimit]),
      [1, 2, 3, 4, 5, 6].map((n) => [`Channel ${n}`, 80]));
    assert.ok(onInterface.length > 0);
    for (const [length, iface] of onInterface) {
      if (length === 321) assert.equal(iface, 1, "every 321-byte ARGB frame goes to interface 1");
      else if (length === 91) assert.equal(iface, 0, "every 91-byte report goes to interface 0");
      else assert.fail(`unexpected feature report length ${length}`);
    }
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x000C, usage: 0 }), true);
  } finally {
    restore();
  }
});

test("Chroma Addressable RGB Controller: the zone-size report goes out only after a resize, never all zero", async () => {
  const c = fixtures.argb;
  const { device, plugin, restore } = await setup({ productId: 0x0F1F, channelColors: {} });
  try {
    device.getHidEndpoints = () => [
      { interface: 0, usage: 0x0002, usage_page: 0x0001, collection: 0 },
      { interface: 1, usage: 0x0000, usage_page: 0x000C, collection: 0 },
    ];
    const isSize = isSizeReport;
    plugin.Initialize();
    plugin.Render();
    assert.ok(!drain(device).some(isSize), "unset channels (all 0): nothing sent");
    device.setChannelColors("Channel 1", [[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
    device.setChannelColors("Channel 2", [[10, 11, 12], [13, 14, 15]]);
    plugin.Render();
    assert.deepEqual(drain(device).filter(isSize), [hex(c.resize.report)], "resize to 3, 2, 0, 0, 0, 0");
    plugin.Render();
    assert.ok(!drain(device).some(isSize), "same sizes: not sent again");
    device.setChannelColors("Channel 1", []);
    device.setChannelColors("Channel 2", []);
    plugin.Render();
    assert.ok(!drain(device).some(isSize), "every size back to 0: never sent");
  } finally {
    restore();
  }
});

test("Chroma Addressable RGB Controller: with interface 1 missing nothing is sent", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0F1F });
  try {
    device.getHidEndpoints = () => [{ interface: 0, usage: 0x0002, usage_page: 0x0001, collection: 0 }];
    plugin.Initialize();
    assert.deepEqual(drain(device), []);
  } finally {
    restore();
  }
});

for (const c of fixtures.krakenV4) {
  test(`Kraken V4 family ${c.name} (${c.pid}): 64-byte output reports`, async () => {
    const { device, plugin, restore } = await setup({ productId: Number(c.pid), color: canvas });
    try {
      const got = phases(plugin, device);
      assert.deepEqual(got.initialize, c.initialize.map(hex), "initialize");
      assert.deepEqual(got.frame, c.frame.map(hex), "frame");
      assert.deepEqual(got.shutdown, c.shutdown.map(hex), "shutdown");
      assert.ok(device.written.length === 0 && device.sent.length === 0);
    } finally {
      restore();
    }
  });
}

test("DeathAdder V2 Pro (1532:007C) keeps HueDeck's recorded packets", async () => {
  const mouse = (sent) => {
    if (sent[2] !== 0x1F) return undefined;
    const reply = sent.slice();
    reply[1] = 0x02;
    return reply;
  };
  for (const pid of [0x007C, 0x007D]) {
    const { device, plugin, restore } = await setup({ productId: pid, color: [0x0C, 0x20, 0xC0], featureReply: mouse });
    try {
      plugin.Initialize();
      assert.deepEqual(drain(device), [hex(recorded.serialRequest), hex(recorded.setEffect)]);
      plugin.Render();
      plugin.Render();
      assert.deepEqual(drain(device), [hex(recorded.frame), hex(recorded.frame)], "every frame is sent");
      plugin.Shutdown(false);
      assert.deepEqual(drain(device), [hex(recorded.frameBlack), hex(recorded.modeNormal)]);
    } finally {
      restore();
    }
  }
});

test("Validate accepts exactly the collections of OpenRGB's detector lines", async () => {
  const pids = [...new Set(DETECTORS.map((d) => d[1]))];
  for (const pid of pids) {
    const { plugin, restore } = await setup({ productId: pid });
    try {
      const rows = DETECTORS.filter((d) => d[1] === pid);
      for (const [name, , , iface, page, usage] of rows) {
        const endpoint = { interface: iface === -1 ? 2 : iface, usage_page: page === -1 ? 0xFF00 : page, usage: usage === -1 ? 0x0001 : usage, collection: 0 };
        assert.equal(plugin.Validate(endpoint), true, `${name} ${JSON.stringify(endpoint)}`);
        if (page !== -1) {
          assert.equal(plugin.Validate({ ...endpoint, usage_page: 0xFFFF }), false, `${name} other usage page`);
        }
        if (iface !== -1 && pid !== 0x0F1F) {
          assert.equal(plugin.Validate({ ...endpoint, interface: 9 }), false, `${name} other interface`);
        }
      }
      assert.equal(plugin.Validate({ interface: 0, usage_page: 0x59, usage: 0x01, collection: 0 }), false, "LampArray");
    } finally {
      restore();
    }
  }
  const { plugin, restore } = await setup({ productId: 0x1234 });
  try {
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x01, usage: 0x02, collection: 0 }), false);
  } finally {
    restore();
  }
});

test("Blackwidow V4 validates on both of its detector collections (firmware before and after 1.5)", async () => {
  const { plugin, restore } = await setup({ productId: 0x0287 });
  try {
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x01, usage: 0x00 }), true);
    assert.equal(plugin.Validate({ interface: 3, usage_page: 0x0C, usage: 0x01 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x01, usage: 0x06 }), false);
  } finally {
    restore();
  }
});

test("ProductId lists every product of the plugin's detectors", async () => {
  const { plugin, restore } = await setup();
  try {
    const ids = plugin.ProductId();
    assert.equal(ids.length, 187);
    // The DeathAdder V2 Pro (wired and wireless) and the Mouse Dock Chroma belong to this plugin
    // (the single-device razer-deathadder-v2-pro plugin was retired), tested on hardware.
    for (const pid of [0x007C, 0x007D, 0x007E]) assert.ok(ids.includes(pid), `0x${pid.toString(16)} claimed`);
    assert.equal(new Set(ids).size, ids.length);
    for (const id of ids) assert.ok(DEVICES[id], `0x${id.toString(16)} has a table entry`);
    const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
    assert.deepEqual(manifest.devices[0].pids.map(Number).sort((a, b) => a - b), ids.slice().sort((a, b) => a - b));
    assert.deepEqual(manifest.devices[0].tested.map(Number), [0x007C, 0x007D, 0x007E]);
    assert.ok(manifest.conflictingProcesses.includes("RazerAppEngine.exe"), "Synapse 4 is reported as a conflict");
  } finally {
    restore();
  }
});

test("geometry: zones stacked, keyboard keys named, unnamed cells left out", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0226, color: canvas });
  try {
    plugin.Initialize();
    const size = device.calls.find((c) => c.method === "setSize").args[0];
    assert.deepEqual(size, [22, 9]);
    const leds = device.calls.filter((c) => c.method === "setControllableLeds").at(-1).args;
    const [names, positions] = leds;
    assert.equal(names.length, positions.length);
    assert.ok(names.includes("Key: Escape"));
    assert.ok(!names.includes(""));
    assert.equal(names.filter((n) => n.startsWith("Underglow LED ")).length, 66);
    assert.deepEqual(positions[names.indexOf("Underglow LED 1")], [0, 6]);
    assert.equal(new Set(names).size, names.length, "names are unique");
    for (const [x, y] of positions) assert.ok(x < 22 && y < 9);
    assert.deepEqual(device.calls.find((c) => c.method === "setName").args, ["Razer Huntsman Elite"]);
  } finally {
    restore();
  }
});

test("keyboard layout: a German keyboard (layout 3) gets the QWERTZ key names", async () => {
  const replies = (sent) => {
    if (sent[8] !== 0x86) return undefined;
    const reply = sent.slice();
    reply[1] = 0x02;
    reply[9] = 3;
    return reply;
  };
  const { device, plugin, restore } = await setup({ productId: 0x0241, color: canvas, featureReply: replies });
  try {
    plugin.Initialize();
    const calls = device.calls.filter((c) => c.method === "setControllableLeds");
    assert.equal(calls.length, 2, "ANSI first, then the layout the keyboard reported");
    const [ansi] = calls[0].args;
    const [qwertz, positions] = calls[1].args;
    assert.ok(!ansi.includes("Key: #"));
    assert.ok(qwertz.includes("Key: #"));
    assert.deepEqual(positions[qwertz.indexOf("Key: #")], [13, 3]);
  } finally {
    restore();
  }
});

test("unchanged frames are not sent again within a second", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0084, color: [5, 6, 7] });
  try {
    plugin.Initialize();
    drain(device);
    plugin.Render();
    assert.equal(drain(device).length, 2);
    plugin.Render();
    assert.equal(drain(device).length, 0);
    device.setColor([5, 6, 8]);
    plugin.Render();
    assert.equal(drain(device).length, 2);
  } finally {
    restore();
  }
});

test("Blade 14 (2021): the keepalive device-mode request goes out with the frames", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0270, color: [1, 1, 1] });
  try {
    plugin.Initialize();
    drain(device);
    plugin.Render();
    const sent = drain(device);
    const last = hexToBytes(sent.at(-1));
    assert.deepEqual([last[6], last[7], last[8]], [0x02, 0x00, 0x84]);
    assert.equal(sent.length, 6 + 1 + 1, "6 rows, the custom effect, the keepalive");
  } finally {
    restore();
  }
});

test("an unknown product id is logged and left alone", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x1234 });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.equal(device.sent.length + device.written.length, 0);
    assert.ok(device.logs.some((l) => /not one this plugin knows/.test(l.message)));
  } finally {
    restore();
  }
});

test("Tiamat 7.1 V2 (interface-only detector): the collection with the 91-byte feature report is used", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0F03, color: [1, 2, 3] });
  try {
    const endpoints = [
      { interface: 0, usage: 0x0001, usage_page: 0x000C, collection: 1 },
      { interface: 0, usage: 0x0002, usage_page: 0x0001, collection: 2 },
    ];
    let active = null;
    device.getHidEndpoints = () => endpoints;
    device.set_endpoint = (iface, usage, page, collection) => { active = collection; return true; };
    device.getHidReportFields = () => (active === 2
      ? { usagePage: 1, usage: 2, reports: [{ kind: "feature", reportId: 0, bytes: 91 }], fields: [] }
      : { usagePage: 0x0C, usage: 1, reports: [{ kind: "input", reportId: 2, bytes: 3 }], fields: [] });
    assert.equal(plugin.Validate(endpoints[0]), true);
    assert.equal(plugin.Validate(endpoints[1]), true);
    plugin.Initialize();
    assert.equal(active, 2);
    assert.ok(device.sent.length > 0);
  } finally {
    restore();
  }
});

test("Blackwidow V4 (two detector lines): the matching collection with the 91-byte feature report is used", async () => {
  for (const pid of [0x0287, 0x028D]) {
    const { device, plugin, restore } = await setup({ productId: pid, color: [1, 2, 3] });
    try {
      const endpoints = [
        { interface: 3, usage: 0x0000, usage_page: 0x0001, collection: 1 },
        { interface: 3, usage: 0x0001, usage_page: 0x000C, collection: 2 },
        { interface: 3, usage: 0x0002, usage_page: 0x0001, collection: 3 },
      ];
      let active = null;
      const probed = [];
      device.getHidEndpoints = () => endpoints;
      device.set_endpoint = (iface, usage, page, collection) => { active = collection; return true; };
      device.getHidReportFields = () => {
        probed.push(active);
        return active === 2
          ? { usagePage: 0x0C, usage: 1, reports: [{ kind: "feature", reportId: 0, bytes: 91 }], fields: [] }
          : { usagePage: 1, usage: 0, reports: [{ kind: "input", reportId: 1, bytes: 8 }], fields: [] };
      };
      assert.equal(plugin.Validate(endpoints[0]), true);
      assert.equal(plugin.Validate(endpoints[1]), true);
      assert.equal(plugin.Validate(endpoints[2]), false);
      plugin.Initialize();
      assert.equal(active, 2);
      assert.ok(!probed.includes(3), "only collections of a detector line are considered");
      assert.ok(device.sent.length > 0);
    } finally {
      restore();
    }
  }
});

test("collection probe: when no collection declares the report, the first one is left active", async () => {
  const { device, plugin, restore } = await setup({ productId: 0x0F03, color: [1, 2, 3] });
  try {
    let active = null;
    device.getHidEndpoints = () => [
      { interface: 0, usage: 0x0001, usage_page: 0x000C, collection: 1 },
      { interface: 0, usage: 0x0002, usage_page: 0x0001, collection: 2 },
    ];
    device.set_endpoint = (iface, usage, page, collection) => { active = collection; return true; };
    device.getHidReportFields = () => ({ usagePage: 1, usage: 2, reports: [], fields: [] });
    plugin.Initialize();
    assert.equal(active, 1);
  } finally {
    restore();
  }
});

// --- battery ------------------------------------------------------------------------------------

/** A device answering every request: status success, the request echoed, the battery answers in argument 1. */
function batteryDevice({ level = 0xBF, charging = 0, status = 0x02, silent = () => false } = {}) {
  return (sent) => {
    if (silent()) return undefined;
    const reply = sent.slice();
    reply[1] = 0x02;
    if (sent[7] === 0x07) {
      reply[1] = status;
      reply[10] = sent[8] === 0x80 ? level : charging;
    }
    return reply;
  };
}

/** The battery requests (command class 0x07) sent since the last call, as spaced hex. */
function batteryRequests(device) {
  const out = device.sent.filter((s) => s.bytes[7] === 0x07).map((s) => bytesToHex(s.bytes));
  device.sent.length = 0;
  device.written.length = 0;
  return out;
}

/** A 91-byte battery request: report id 0, transaction id, size 2, class 0x07, id, crc. */
function batteryRequest(transactionId, commandId) {
  const bytes = new Array(91).fill(0);
  bytes[2] = transactionId;
  bytes[6] = 0x02;
  bytes[7] = 0x07;
  bytes[8] = commandId;
  bytes[89] = 0x02 ^ 0x07 ^ commandId;
  return bytesToHex(bytes);
}

test("battery: DeathAdder V2 Pro (Wireless) asks 0x07 0x80 then 0x07 0x84 on separate frames, 45 s apart", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup({ productId: 0x007D, color: [1, 2, 3], featureReply: batteryDevice({ level: 0xBF, charging: 1 }) });
  try {
    plugin.Initialize();
    assert.deepEqual(device.features, ["battery"]);
    assert.deepEqual(batteryRequests(device), [], "nothing asked in Initialize");
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [], "not on the first frame");
    t.mock.timers.tick(2000);
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [batteryRequest(0x3F, 0x80)], "level first");
    assert.deepEqual(device.battery.levels, [], "reported once both answers are in");
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [batteryRequest(0x3F, 0x84)], "charging status on the next frame");
    assert.deepEqual(device.battery.levels, [75], "0xBF / 255 = 75 %");
    assert.deepEqual(device.battery.states, [2], "charging");
    plugin.Render();
    t.mock.timers.tick(44_999);
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [], "nothing before 45 s");
    t.mock.timers.tick(1);
    plugin.Render();
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [batteryRequest(0x3F, 0x80), batteryRequest(0x3F, 0x84)]);
    assert.deepEqual(device.battery.levels, [75], "unchanged values are not reported again");
  } finally {
    restore();
  }
});

test("battery: percent scaling and states (full when charging at 100 %, draining when not charging)", async (t) => {
  const cases = [
    { level: 0xFF, charging: 1, percent: 100, state: 4 },
    { level: 0xFF, charging: 0, percent: 100, state: 1 },
    { level: 0x80, charging: 0, percent: 50, state: 1 },
    { level: 0x00, charging: 1, percent: 0, state: 2 },
    { level: 0x01, charging: 0, percent: 0, state: 1 },
  ];
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  for (const c of cases) {
    const { device, plugin, restore } = await setup({ productId: 0x00AB, color: [1, 2, 3], featureReply: batteryDevice(c) });
    try {
      plugin.Initialize();
      t.mock.timers.tick(2000);
      plugin.Render();
      plugin.Render();
      assert.deepEqual(batteryRequests(device), [batteryRequest(0x1F, 0x80), batteryRequest(0x1F, 0x84)]);
      assert.deepEqual([device.battery.levels, device.battery.states], [[c.percent], [c.state]], JSON.stringify(c));
    } finally {
      restore();
    }
  }
});

test("battery: Basilisk V3 X HyperSpeed has no charging-status query (level only, draining)", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup({ productId: 0x00B9, color: [1, 2, 3], featureReply: batteryDevice({ level: 0x40 }) });
  try {
    plugin.Initialize();
    t.mock.timers.tick(2000);
    plugin.Render();
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [batteryRequest(0x1F, 0x80)]);
    assert.deepEqual([device.battery.levels, device.battery.states], [[25], [1]]);
  } finally {
    restore();
  }
});

test("battery: products without a battery get no feature and no battery query", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  for (const productId of [0x0084, 0x007E, 0x0F1F, 0x056C]) {
    const { device, plugin, restore } = await setup({ productId, color: [1, 2, 3], featureReply: batteryDevice() });
    try {
      plugin.Initialize();
      for (let i = 0; i < 4; i++) {
        t.mock.timers.tick(30_000);
        plugin.Render();
      }
      assert.deepEqual(device.features, [], `0x${productId.toString(16)}`);
      assert.deepEqual(batteryRequests(device), []);
      assert.deepEqual(device.battery.levels, []);
    } finally {
      restore();
    }
  }
});

test("battery: a receiver's paired mouse is asked with the mouse's transaction id", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const answer = batteryDevice({ level: 0xFF, charging: 0 });
  const replies = (sent) => {
    const reply = answer(sent);
    if (sent[7] === 0x00 && sent[8] === 0xBF) {
      const slots = [3, 0x01, 0x00, 0xA4, 0x01, 0x00, 0xAB, 0x00, 0xFF, 0xFF];
      for (let i = 0; i < slots.length; i++) reply[9 + i] = slots[i];
    }
    return reply;
  };
  const { device, plugin, restore } = await setup({ productId: 0x00A4, color: [1, 2, 3], featureReply: replies });
  try {
    plugin.Initialize();
    assert.deepEqual(device.features, ["battery"], "the dock has none, its Basilisk V3 Pro has one");
    t.mock.timers.tick(2000);
    plugin.Render();
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [batteryRequest(0x1F, 0x80), batteryRequest(0x1F, 0x84)]);
    assert.deepEqual([device.battery.levels, device.battery.states], [[100], [1]]);
  } finally {
    restore();
  }
});

test("battery: a silent device keeps its level, turns unknown once, is logged once, and comes back", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  let asleep = false;
  const { device, plugin, restore } = await setup({
    productId: 0x00AB, color: [1, 2, 3], featureReply: batteryDevice({ level: 0x80, silent: () => asleep }),
  });
  try {
    plugin.Initialize();
    t.mock.timers.tick(2000);
    plugin.Render();
    plugin.Render();
    assert.deepEqual([device.battery.levels, device.battery.states], [[50], [1]]);
    batteryRequests(device);
    asleep = true;
    device.setFeatureReport(0x00, []);
    const pausesBefore = device.pauses.length;
    for (let i = 0; i < 3; i++) {
      t.mock.timers.tick(45_000);
      plugin.Render();
      plugin.Render();
    }
    assert.deepEqual(batteryRequests(device), [batteryRequest(0x1F, 0x80), batteryRequest(0x1F, 0x80), batteryRequest(0x1F, 0x80)],
      "one level query per poll, no charging query without a level");
    assert.deepEqual([device.battery.levels, device.battery.states], [[50, 50], [1, 0]], "level kept, state unknown");
    assert.equal(device.logs.filter((l) => /battery/.test(l.message)).length, 1, "logged once");
    const batteryWaits = device.pauses.slice(pausesBefore).filter((ms) => ms === 2).length;
    assert.ok(batteryWaits <= 3 * 10, `at most 10 short reads per query (${batteryWaits})`);
    asleep = false;
    t.mock.timers.tick(45_000);
    plugin.Render();
    plugin.Render();
    assert.deepEqual(device.battery.states, [1, 0, 1], "back once it answers");
  } finally {
    restore();
  }
});

test("battery: an answer 'not supported' stops the polling", async (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  const { device, plugin, restore } = await setup({ productId: 0x0090, color: [1, 2, 3], featureReply: batteryDevice({ status: 0x05 }) });
  try {
    plugin.Initialize();
    t.mock.timers.tick(2000);
    plugin.Render();
    assert.deepEqual(batteryRequests(device), [batteryRequest(0x1F, 0x80)]);
    for (let i = 0; i < 3; i++) {
      t.mock.timers.tick(45_000);
      plugin.Render();
    }
    assert.deepEqual(batteryRequests(device), []);
    assert.deepEqual(device.battery.states, []);
  } finally {
    restore();
  }
});
