// SPDX-License-Identifier: GPL-3.0-or-later
//
// Every first-party plugin passes the SDK lint (sdk/lint/lint.mjs) with no error: manifest.json
// against schemas/plugin-manifest-v2.schema.json, tested ids among the claimed ones, the entry
// file exists, SMBus addresses inside 0x03..0x77 and equal to SmbusAddresses(), deniedRegisters
// equal to SmbusDeniedRegisters(), spdManufacturers to SpdManufacturers(), batchWrites to
// SmbusBatchWrites(), pci cards to BrandGPUList(), USB claims to VendorId()/ProductId(),
// hidUsage claims to HidUsages(), and the exports themselves (LedNames/LedPositions,
// ControllableParameters, no DOM or Node globals). On top of that, the first-party policy:
// id `lightloom.<dir>`, GPL-3.0-or-later, an explicit manifestVersion and deviceType, and ids
// written as hex strings.

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { formatResult, lintPlugin, MANIFEST_SCHEMA as schema } from "../../sdk/lint/lint.mjs";

const plugins = join(dirname(fileURLToPath(import.meta.url)), "..");
const dirs = readdirSync(plugins, { withFileTypes: true })
  .filter((d) => d.isDirectory() && existsSync(join(plugins, d.name, "manifest.json")))
  .map((d) => d.name);

test("there are first-party plugins to check", () => {
  assert.ok(dirs.length >= 5, `found ${dirs.join(", ")}`);
});

for (const dir of dirs) {
  test(`${dir} passes the SDK lint`, async () => {
    const result = await lintPlugin(join(plugins, dir));
    const errors = result.findings.filter((f) => f.level === "error");
    assert.deepEqual(errors, [], formatResult(result));
  });

  test(`${dir}/manifest.json follows the first-party policy`, () => {
    const manifest = JSON.parse(readFileSync(join(plugins, dir, "manifest.json"), "utf8"));
    assert.equal(manifest.manifestVersion, 2);
    assert.equal(manifest.id, `lightloom.${dir}`);
    assert.equal(manifest.license, "GPL-3.0-or-later");
    assert.ok(schema.properties.deviceType.enum.includes(manifest.deviceType), `deviceType '${manifest.deviceType}'`);
    const usbId = new RegExp(schema.$defs.usbId.anyOf.find((form) => form.type === "string").pattern);
    for (const claim of manifest.devices) {
      const ids = [
        ...("vid" in claim ? [claim.vid, ...claim.pids, ...(claim.tested ?? [])] : []),
        ...("hidUsage" in claim ? [claim.hidUsage.usagePage, claim.hidUsage.usage] : []),
        ...("smbus" in claim ? [...(claim.smbus.pci ?? []), ...(claim.smbus.tested ?? []).filter((t) => !("bus" in t))].flatMap((t) => Object.values(t)) : []),
      ];
      for (const id of ids) {
        assert.equal(typeof id, "string", `id ${id} is written as a hex string`);
        assert.match(id, usbId);
      }
    }
  });
}
