// SPDX-License-Identifier: GPL-3.0-or-later
//
// Exit modes (docs/host-api/save-shutdown.md): "keep" sends the last frame once more with the
// update reports of a frame, "off" the same reports black, both without touching AutonomousMode;
// "restore" and suspend give the device its autonomous mode back.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { checkExitModes } from "../../_testkit/exit-modes.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const layout = JSON.parse(readFileSync(join(here, "..", "fixtures", "layout.json"), "utf8"));
const le = (value, bytes) => Array.from({ length: bytes }, (_, i) => Math.floor(value / 256 ** i) % 256);
const u16 = (bytes, at) => bytes[at] | (bytes[at + 1] << 8);

// The device of lamparray.test.mjs: ten lamps, lamp 9 not programmable.
const lamps = Array.from({ length: 10 }, (_, id) => ({ id, x: id * 10000, y: (id % 2) * 20000, programmable: id !== 9 }));
const attributes = [1, ...le(10, 2), ...le(100000, 4), ...le(40000, 4), ...le(5000, 4), ...le(1, 4), ...le(4000, 4)];
const featureReply = (sent) => {
  if (sent[0] !== 2) return undefined;
  const lamp = lamps[u16(sent, 1)];
  return [3, ...le(lamp.id, 2), ...le(lamp.x, 4), ...le(lamp.y, 4), ...le(0, 4), ...le(1000, 4), ...le(1, 4),
    256, 256, 256, 1, lamp.programmable ? 1 : 0, 0];
};
// fixtures/layout.json: feature report 6 is the control report (AutonomousMode).
const controlId = 6;

test("exit modes: keep resends the last frame, off is black, AutonomousMode only on restore and suspend", async () => {
  const { keep, off, restore } = await checkExitModes(pluginPath, {
    options: { featureReports: { 1: attributes }, featureReply, reportFields: layout },
    restoreIsBlack: false,
    forbidden: (b) => b[0] === controlId,
  });
  assert.ok(keep.length > 0 && off.length > 0);
  assert.ok(restore.length === 1 && Number.parseInt(restore[0].split(" ")[0], 16) === controlId, "restore: the control report");
});
