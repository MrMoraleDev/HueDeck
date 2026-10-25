// SPDX-License-Identifier: GPL-3.0-or-later
//
// Exit modes (docs/host-api/save-shutdown.md): "keep" sends the last frame once more with the
// colour reports of a frame (0x32 pump, 0x40 fans), "off" the same reports black; "restore" and
// suspend stay the spectrum cycle.

import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { checkExitModes } from "../../_testkit/exit-modes.mjs";
import { DETECTORS } from "../devices.js";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");

test("exit modes: keep resends the last frame, off is black, restore and suspend unchanged", async () => {
  for (const productId of new Set(DETECTORS.map((d) => d[1]))) {
    await checkExitModes(pluginPath, {
      options: { productId },
      restoreIsBlack: false,
      forbidden: (b) => b[0] !== 0x00 || ![0x32, 0x40].includes(b[1]),
    });
  }
});
