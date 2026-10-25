// SPDX-License-Identifier: GPL-3.0-or-later
//
// Exit modes (docs/host-api/save-shutdown.md), for every product id of the manifest: "keep" sends
// the last frame once more with the reports of a frame, "off" the same reports black, "restore"
// and suspend are unchanged; the save command (0x11) is never sent.

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
      restoreIsBlack: true,
      forbidden: (b) => b[1] === 0x11,
    });
  }
});
