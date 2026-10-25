// SPDX-License-Identifier: GPL-3.0-or-later
//
// Exit modes (docs/host-api/save-shutdown.md) for every product id: "keep" sends the last frame
// once more in the output report of a frame, "off" the same report black, "restore" and suspend
// are unchanged (black).

import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { checkExitModes } from "../../_testkit/exit-modes.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const usbInput = (id) => [id, ...new Array(63).fill(0)];

test("exit modes: keep resends the last frame, off is black, restore and suspend unchanged", async () => {
  for (const productId of [0x05C4, 0x09CC, 0x0BA0, 0x0CE6, 0x0DF2]) {
    await checkExitModes(pluginPath, {
      options: { productId, vendorId: 0x054C, inputReports: { 0: usbInput(0x01) } },
      // Only the USB output reports of a frame (DualShock 4 0x05, DualSense 0x02).
      forbidden: (b) => b[0] !== 0x05 && b[0] !== 0x02,
    });
  }
});
