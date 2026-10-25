// SPDX-License-Identifier: GPL-3.0-or-later
//
// Exit modes (docs/host-api/save-shutdown.md). Kraken V3: "keep" sends the last Direct colours
// once more, "off" black ones; Kraken 7.1: "keep" the custom colour and effect byte "on" of a
// frame, "off" OpenRGB's SetModeOff() (effect byte 0). "restore" and suspend stay the hardware
// effect, and nothing but those reports is sent.

import { test } from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { checkExitModes, shutdownIo } from "../../_testkit/exit-modes.mjs";
import { DETECTORS, KRAKEN_ADDRESSES, KRAKEN_CUSTOM_RGB } from "../devices.js";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const hex = (b) => b.map((v) => v.toString(16).toUpperCase().padStart(2, "0")).join(" ");

function krakenReport(address, args) {
  const b = new Array(37).fill(0);
  b[0] = 0x04;
  b[1] = 0x40;
  b[2] = args.length;
  b[3] = address >> 8;
  b[4] = address & 0xFF;
  args.forEach((v, i) => { b[5 + i] = v; });
  return hex(b);
}

test("Kraken V3 exit modes: keep resends the Direct colours, off black ones, restore and suspend unchanged", async () => {
  for (const [, pid] of DETECTORS.filter((d) => d[2] === "kraken-v3")) {
    await checkExitModes(pluginPath, {
      options: { productId: pid },
      restoreIsBlack: false,
      forbidden: (b) => b[0] !== 0x40 || b[1] !== 0x03,
    });
  }
});

test("Kraken 7.1 exit modes: keep = custom colour + effect on, off = effect byte 0", async () => {
  const colour = [0x12, 0x34, 0x56];
  for (const [, pid] of DETECTORS.filter((d) => d[2] === "kraken")) {
    const { ledMode, custom } = KRAKEN_ADDRESSES[pid];
    const run = (mode, systemSuspending = false) =>
      shutdownIo(pluginPath, { options: { productId: pid }, colour, mode, systemSuspending });
    const keep = await run("keep");
    assert.deepEqual(keep.modes, ["restore", "keep", "off"]);
    assert.deepEqual(keep.out, [
      ...(KRAKEN_CUSTOM_RGB.includes(pid) ? [krakenReport(custom, colour)] : []),
      krakenReport(ledMode, [0x01]),
    ], `0x${pid.toString(16)} keep`);
    assert.deepEqual((await run("off")).out, [krakenReport(ledMode, [0x00])]);
    const spectrum = [krakenReport(ledMode, [0x05])];
    assert.deepEqual((await run("restore")).out, spectrum);
    assert.deepEqual((await run(undefined)).out, spectrum);
    assert.deepEqual((await run("keep", true)).out, spectrum);
    assert.deepEqual((await run("off", true)).out, spectrum);
  }
});
