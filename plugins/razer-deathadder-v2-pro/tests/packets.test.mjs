// SPDX-License-Identifier: GPL-3.0-or-later
//
// Razer DeathAdder V2 Pro plugin: the requests, a frame and the shutdown are what the mouse
// received in HueDeck's own recording (fixtures/recorded.json); the transaction id is found by
// asking; replies of another command or transaction id are not taken for confirmations.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const recorded = JSON.parse(readFileSync(join(here, "..", "fixtures", "recorded.json"), "utf8"));
const hex = (h) => bytesToHex(hexToBytes(h));

/** A mouse that confirms every request whose transaction id it answers to. */
function mouse(answersTo = 0x1F) {
  return (sent) => {
    if (sent[2] !== answersTo) return undefined;
    const reply = sent.slice();
    reply[1] = 0x02;
    return reply;
  };
}

async function setup(options) {
  const device = createMockDevice(options);
  const restore = installGlobals(device);
  const plugin = await loadPlugin(pluginPath);
  return { device, plugin, restore };
}

test("requests, a frame and the shutdown are byte for byte what the mouse received", async () => {
  const { device, plugin, restore } = await setup({ color: [0x0C, 0x20, 0xC0], featureReply: mouse() });
  try {
    plugin.Initialize();
    const sent = device.sent.map((s) => bytesToHex(s.bytes));
    assert.deepEqual(sent, [hex(recorded.serialRequest), hex(recorded.setEffect)]);
    device.sent.length = 0;
    plugin.Render();
    assert.deepEqual(device.sent.map((s) => bytesToHex(s.bytes)), [hex(recorded.frame)]);
    device.sent.length = 0;
    plugin.Shutdown();
    assert.deepEqual(device.sent.map((s) => bytesToHex(s.bytes)), [hex(recorded.frameBlack), hex(recorded.modeNormal)]);
    assert.ok(device.sent.every((s) => s.length === 91));
  } finally {
    restore();
  }
});

test("frames carry the transaction id the mouse answered to", async () => {
  const { device, plugin, restore } = await setup({ color: [1, 2, 3], featureReply: mouse(0x3F) });
  try {
    plugin.Initialize();
    const ids = device.sent.filter((s) => s.bytes[7] === 0x00 && s.bytes[8] === 0x82).map((s) => s.bytes[2]);
    assert.deepEqual(ids, [0x1F, 0x3F], "0x1F asked first, 0x3F answered");
    device.sent.length = 0;
    plugin.Render();
    assert.equal(device.sent[0].bytes[2], 0x3F);
    assert.deepEqual(device.sent[0].bytes.slice(9, 17), [0, 0, 0, 0, 0, 1, 2, 3]);
  } finally {
    restore();
  }
});

test("a silent mouse still gets frames, with a note in the log", async () => {
  const { device, plugin, restore } = await setup({ color: [9, 9, 9] });
  try {
    plugin.Initialize();
    assert.ok(device.logs.some((l) => /transaction id/.test(l.message)));
    device.sent.length = 0;
    plugin.Render();
    assert.equal(device.sent.length, 1);
    assert.equal(device.sent[0].bytes[2], 0x1F);
  } finally {
    restore();
  }
});

test("only the mouse collection of interface 0 validates", async () => {
  const { plugin, restore } = await setup();
  try {
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0002 }), true);
    assert.equal(plugin.Validate({ interface: 0, usage_page: 0x0001, usage: 0x0006 }), false);
    assert.equal(plugin.Validate({ interface: 1, usage_page: 0x0001, usage: 0x0002 }), false);
  } finally {
    restore();
  }
});
