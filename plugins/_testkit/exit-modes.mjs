// SPDX-License-Identifier: GPL-3.0-or-later
//
// Shared checks of the Lightloom exit modes (docs/host-api/save-shutdown.md) for plugins whose
// "keep" and "off" resend the last frame (or a black one) with the reports every frame uses:
//   * ShutdownModes() is ["restore", "keep", "off"];
//   * "off" sends the same bytes whatever the canvas showed, and they are what "keep" sends after a
//     black frame (the same packets, only the colours differ);
//   * "keep" after a coloured frame differs from "off" (the colours are in it);
//   * "restore" is what Shutdown(false) without a mode (SignalRGB) sends, black when the plugin
//     says so;
//   * suspend (Shutdown(true, mode)) is what it was before the modes existed;
//   * an optional `forbidden(bytes)` (a save / flash command) never matches what keep or off send.

import assert from "node:assert/strict";
import { createMockDevice, installGlobals, loadPlugin, bytesToHex } from "./mock-device.mjs";

const OUT = new Set(["write", "send_report"]);

/**
 * Initialize, one Render with `colour`, then Shutdown(systemSuspending, mode): the bytes written
 * (output and feature reports) by the Shutdown call only.
 */
export async function shutdownIo(pluginPath, { options = {}, params = {}, colour, mode, systemSuspending = false, before }) {
  const device = createMockDevice({ ...options, color: colour });
  const restore = installGlobals(device, params);
  try {
    const plugin = await loadPlugin(pluginPath);
    plugin.Initialize();
    plugin.Render();
    if (before) {
      before(plugin, device);
    }
    device.calls.length = 0;
    plugin.Shutdown(systemSuspending, mode);
    return {
      modes: plugin.ShutdownModes ? plugin.ShutdownModes() : null,
      out: device.calls.filter((c) => OUT.has(c.method)).map(({ args: [data, length] }) =>
        bytesToHex(Array.from({ length }, (_, i) => (i < data.length ? data[i] & 0xFF : 0)))),
    };
  } finally {
    restore();
  }
}

/**
 * The checks above for one device setup.
 * @param {string} pluginPath
 * @param {object} c  { options, params, restoreIsBlack = true, forbidden, colour }
 */
export async function checkExitModes(pluginPath, c) {
  const colour = c.colour ?? [0x12, 0x34, 0x56];
  const run = (col, mode, systemSuspending = false) =>
    shutdownIo(pluginPath, { options: c.options, params: c.params, colour: col, mode, systemSuspending, before: c.before });
  const black = [0, 0, 0];
  const keep = await run(colour, "keep");
  assert.deepEqual(keep.modes, ["restore", "keep", "off"], "ShutdownModes()");
  const keepBlack = await run(black, "keep");
  const off = await run(colour, "off");
  const offBlack = await run(black, "off");
  const restore = await run(colour, "restore");
  const plain = await run(colour, undefined);
  assert.ok(off.out.length > 0, "off sends something");
  assert.deepEqual(off.out, offBlack.out, "off ignores the canvas");
  assert.deepEqual(keepBlack.out, off.out, "keep after a black frame = off: the same packets");
  assert.notDeepEqual(keep.out, off.out, "keep carries the last colours");
  assert.deepEqual(restore.out, plain.out, "restore = SignalRGB's Shutdown(false)");
  if (c.restoreIsBlack ?? true) {
    assert.deepEqual(restore.out, off.out, "restore is the black exit");
  }
  for (const mode of ["keep", "off"]) {
    const suspend = await run(colour, mode, true);
    const before = await run(colour, undefined, true);
    assert.deepEqual(suspend.out, before.out, `suspend with ${mode} is unchanged`);
  }
  if (c.forbidden) {
    for (const report of [...keep.out, ...off.out]) {
      assert.ok(!c.forbidden(report.split(" ").map((h) => parseInt(h, 16))), `no save command: ${report}`);
    }
  }
  return { keep: keep.out, off: off.out, restore: restore.out };
}
