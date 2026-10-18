// SPDX-License-Identifier: GPL-3.0-or-later
//
// Every first-party plugin's manifest.json against schemas/plugin-manifest-v2.schema.json: the
// required keys are there, no key is outside the schema, USB claims have the claim shape, the
// tested product ids are among the claimed ones, and the entry file exists.

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const plugins = join(dirname(fileURLToPath(import.meta.url)), "..");
const schema = JSON.parse(readFileSync(join(plugins, "..", "schemas", "plugin-manifest-v2.schema.json"), "utf8"));
const dirs = readdirSync(plugins, { withFileTypes: true })
  .filter((d) => d.isDirectory() && existsSync(join(plugins, d.name, "manifest.json")))
  .map((d) => d.name);

test("there are first-party plugins to check", () => {
  assert.ok(dirs.length >= 5, `found ${dirs.join(", ")}`);
});

for (const dir of dirs) {
  test(`${dir}/manifest.json follows the manifest schema`, () => {
    const manifest = JSON.parse(readFileSync(join(plugins, dir, "manifest.json"), "utf8"));
    for (const key of schema.required) {
      assert.ok(key in manifest, `missing required key '${key}'`);
    }
    for (const key of Object.keys(manifest)) {
      assert.ok(key in schema.properties, `key '${key}' is not in the schema`);
    }
    assert.equal(manifest.manifestVersion, 2);
    assert.match(manifest.id, new RegExp(schema.properties.id.pattern));
    assert.equal(manifest.id, `huedeck.${dir}`);
    assert.equal(manifest.license, "GPL-3.0-or-later");
    assert.ok(schema.properties.deviceType.enum.includes(manifest.deviceType), `deviceType '${manifest.deviceType}'`);
    for (const p of manifest.permissions) {
      assert.ok(schema.$defs.permission.enum.includes(p), `unknown permission '${p}'`);
    }
    assert.ok(existsSync(join(plugins, dir, manifest.entry)), `entry '${manifest.entry}' exists`);
    const usbId = new RegExp(schema.$defs.usbId.anyOf.find((form) => form.type === "string").pattern);
    for (const claim of manifest.devices) {
      for (const key of schema.$defs.usbClaim.required) {
        assert.ok(key in claim, `usb claim is missing '${key}'`);
      }
      for (const key of Object.keys(claim)) {
        assert.ok(key in schema.$defs.usbClaim.properties, `usb claim key '${key}' is not in the schema`);
      }
      for (const id of [claim.vid, ...claim.pids, ...(claim.tested ?? [])]) {
        assert.match(id, usbId);
      }
      const pids = claim.pids.map((p) => Number(p));
      for (const pid of claim.tested ?? []) {
        assert.ok(pids.includes(Number(pid)), `tested ${pid} is not one of the claimed pids`);
      }
    }
  });
}
