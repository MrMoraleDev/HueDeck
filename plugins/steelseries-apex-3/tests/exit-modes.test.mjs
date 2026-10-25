// SPDX-License-Identifier: GPL-3.0-or-later
//
// Exit modes (docs/host-api/save-shutdown.md), for every product id of the manifest: "keep" sends
// the last frame once more with the reports of a frame, "off" the same reports black, "restore"
// and suspend are unchanged (the Apex 3 TKL restores its rainbow wave); only colour reports in keep and off, never the save command or the rainbow wave.

import { test } from "node:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { checkExitModes } from "../../_testkit/exit-modes.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const manifest = JSON.parse(readFileSync(join(here, "..", "manifest.json"), "utf8"));
const pids = manifest.devices.flatMap((d) => d.pids).map(Number);

test("exit modes: keep resends the last frame, off is black, restore and suspend unchanged", async () => {
  for (const pid of pids) {
    await checkExitModes(pluginPath, {
      options: { productId: pid },
      restoreIsBlack: pid !== 0x1622,
      forbidden: (b) => b[0] !== 0x00 || ![0x0A, 0x0B, 0x21, 0x23].includes(b[1]),
    });
  }
});
