// SPDX-License-Identifier: GPL-3.0-or-later
//
// Every first-party plugin's manifest.json against schemas/plugin-manifest-v2.schema.json: the
// required keys are there, no key is outside the schema, USB claims have the claim shape, the
// tested product ids are among the claimed ones, and the entry file exists. SMBus claims list valid
// 7-bit addresses, which must equal the plugin's SmbusAddresses() export, and their deniedRegisters
// equal its SmbusDeniedRegisters(), their spdManufacturers its SpdManufacturers(), and their
// batchWrites its SmbusBatchWrites().

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
// Registers the @SignalRGB/* module hook network plugins import.
import "./mock-net.mjs";

const plugins = join(dirname(fileURLToPath(import.meta.url)), "..");
const schema = JSON.parse(readFileSync(join(plugins, "..", "schemas", "plugin-manifest-v2.schema.json"), "utf8"));
const dirs = readdirSync(plugins, { withFileTypes: true })
  .filter((d) => d.isDirectory() && existsSync(join(plugins, d.name, "manifest.json")))
  .map((d) => d.name);

test("there are first-party plugins to check", () => {
  assert.ok(dirs.length >= 5, `found ${dirs.join(", ")}`);
});

for (const dir of dirs) {
  test(`${dir}/manifest.json follows the manifest schema`, async () => {
    const manifest = JSON.parse(readFileSync(join(plugins, dir, "manifest.json"), "utf8"));
    for (const key of schema.required) {
      assert.ok(key in manifest, `missing required key '${key}'`);
    }
    for (const key of Object.keys(manifest)) {
      assert.ok(key in schema.properties, `key '${key}' is not in the schema`);
    }
    assert.equal(manifest.manifestVersion, 2);
    assert.match(manifest.id, new RegExp(schema.properties.id.pattern));
    assert.equal(manifest.id, `lightloom.${dir}`);
    assert.equal(manifest.license, "GPL-3.0-or-later");
    assert.ok(schema.properties.deviceType.enum.includes(manifest.deviceType), `deviceType '${manifest.deviceType}'`);
    for (const p of manifest.permissions) {
      assert.ok(schema.$defs.permission.enum.includes(p), `unknown permission '${p}'`);
    }
    assert.ok(existsSync(join(plugins, dir, manifest.entry)), `entry '${manifest.entry}' exists`);
    const usbId = new RegExp(schema.$defs.usbId.anyOf.find((form) => form.type === "string").pattern);
    for (const claim of manifest.devices) {
      if ("smbus" in claim) {
        const smbus = claim.smbus;
        for (const key of Object.keys(smbus)) {
          assert.ok(key in schema.$defs.smbusClaim.properties.smbus.properties, `smbus claim key '${key}' is not in the schema`);
        }
        assert.ok(smbus.addresses.length > 0);
        for (const a of smbus.addresses) {
          assert.ok(Number.isInteger(a) && a >= 0x03 && a <= 0x77, `address ${a} outside 0x03..0x77`);
        }
        const kinds = schema.$defs.smbusClaim.properties.smbus.properties.buses.items.enum;
        for (const b of smbus.buses ?? []) {
          assert.ok(kinds.includes(b), `bus kind '${b}'`);
        }
        const tuple = (t) => {
          for (const key of schema.$defs.pciTuple.required) {
            assert.match(t[key], usbId, `pci ${key}`);
          }
          assert.deepEqual(Object.keys(t).sort(), [...schema.$defs.pciTuple.required].sort());
          return schema.$defs.pciTuple.required.map((k) => Number(t[k])).join(":");
        };
        const cards = (smbus.pci ?? []).map(tuple);
        for (const t of smbus.tested ?? []) {
          assert.ok(cards.includes(tuple(t)), "tested card is not one of the claimed cards");
        }
        const plugin = await import(pathToFileURL(join(plugins, dir, manifest.entry)).href);
        assert.deepEqual(plugin.SmbusAddresses(), smbus.addresses, "SmbusAddresses() equals the manifest addresses");
        assert.deepEqual(
          plugin.SmbusDeniedRegisters?.() ?? [],
          smbus.deniedRegisters ?? [],
          "SmbusDeniedRegisters() equals the manifest deniedRegisters",
        );
        assert.deepEqual(
          plugin.SpdManufacturers?.() ?? [],
          smbus.spdManufacturers ?? [],
          "SpdManufacturers() equals the manifest spdManufacturers",
        );
        assert.equal(
          plugin.SmbusBatchWrites?.() === true,
          smbus.batchWrites === true,
          "SmbusBatchWrites() equals the manifest batchWrites",
        );
        if (manifest.deviceType === "ram" && !(smbus.pci ?? []).length) {
          assert.ok((smbus.spdManufacturers ?? []).length > 0, "a RAM plugin declares spdManufacturers (SPD check)");
        }
        assert.equal(String(plugin.Type()).toUpperCase(), "SMBUS");
        continue;
      }
      if ("network" in claim) {
        const net = claim.network;
        const keys = schema.$defs.networkClaim.properties.network.properties;
        for (const key of Object.keys(net)) {
          assert.ok(key in keys, `network claim key '${key}' is not in the schema`);
        }
        for (const port of [...(net.udpPorts ?? []), ...(net.tcpPorts ?? [])]) {
          assert.ok(Number.isInteger(port) && port >= 1 && port <= 65535, `port ${port}`);
        }
        const pattern = new RegExp(keys.mdns.items.pattern);
        for (const t of net.mdns ?? []) {
          assert.match(t, pattern);
        }
        // The plugin's DiscoveryService browses exactly the claimed mDNS types (its constructor
        // only sets fields, so no globals are needed).
        const plugin = await import(pathToFileURL(join(plugins, dir, manifest.entry)).href);
        assert.equal(String(plugin.Type()).toLowerCase(), "network");
        assert.equal(typeof plugin.DiscoveryService, "function", "network plugins export DiscoveryService");
        const discovery = new plugin.DiscoveryService();
        const browsed = (discovery.MDns ?? []).map((t) => String(t).replace(/\.local\.?$/, ""));
        assert.deepEqual([...browsed].sort(), [...(net.mdns ?? [])].sort(), "MDns equals the manifest mdns");
        for (const p of manifest.permissions) {
          assert.ok(p.startsWith("network-") || ["mdns", "settings"].includes(p), `permission '${p}'`);
        }
        assert.equal(manifest.permissions.includes("mdns"), browsed.length > 0, "mdns permission iff MDns");
        continue;
      }
      if ("hidUsage" in claim) {
        for (const key of Object.keys(claim)) {
          assert.ok(key in schema.$defs.hidUsageClaim.properties, `hid usage claim key '${key}' is not in the schema`);
        }
        assert.match(claim.hidUsage.usagePage, usbId);
        assert.match(claim.hidUsage.usage, usbId);
        continue;
      }
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
