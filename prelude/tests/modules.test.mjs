// SPDX-License-Identifier: GPL-3.0-or-later
// Every @SignalRGB/* facade in prelude/modules: import succeeds, declared
// members match the corpus-derived list, each throws HostApiMissing naming
// module and member, and each gap is logged once.
import { test, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { HostApiMissing, setLogger, resetLogOnce } from "../HostApiMissing.js";

import udp, { MEMBERS as UDP } from "../modules/udp.js";
import tcp, { MEMBERS as TCP } from "../modules/tcp.js";
import serial, { MEMBERS as SERIAL } from "../modules/serial.js";
import LCD, { MEMBERS as LCDM } from "../modules/lcd.js";
import permissions, { MEMBERS as PERM } from "../modules/permissions.js";
import systeminfo, { MEMBERS as SYS } from "../modules/systeminfo.js";
import DeviceDiscovery, { MEMBERS as DD } from "../modules/DeviceDiscovery.js";
import performance, { MEMBERS as PERF } from "../modules/performance.js";
import appInfo, { MEMBERS as APP } from "../modules/appInfo.js";

const EXPECTED = {
  "@SignalRGB/udp": { mod: udp, members: UDP, want: ["createSocket", "send"] },
  "@SignalRGB/tcp": { mod: tcp, members: TCP, want: ["createSocket"] },
  "@SignalRGB/serial": {
    mod: serial,
    members: SERIAL,
    want: ["connect", "disconnect", "isConnected", "write", "read", "readAll", "getDeviceInfo", "getPortName", "getBaudRate", "availablePorts"],
  },
  "@SignalRGB/lcd": { mod: LCD, members: LCDM, want: ["initialize", "getFrame", "getSlicedFrame", "getImageBuffer"] },
  "@SignalRGB/permissions": { mod: permissions, members: PERM, want: ["permissions", "setCallback"] },
  "@SignalRGB/systeminfo": { mod: systeminfo, members: SYS, want: ["GetMotherboardInfo", "GetBiosInfo", "GetRamInfo"] },
  "@SignalRGB/DeviceDiscovery": { mod: DeviceDiscovery, members: DD, want: ["foundVirtualDevice"] },
  "@SignalRGB/performance": { mod: performance, members: PERF, want: ["StartTimer", "EndTimer", "LogReport", "GetTimers"] },
  "@SignalRGB/appInfo": { mod: appInfo, members: APP, want: [] },
};

let logs;
beforeEach(() => {
  logs = [];
  setLogger((m) => logs.push(m));
  resetLogOnce();
});
afterEach(() => {
  setLogger(null);
  resetLogOnce();
});

test("the modules directory contains exactly the nine facades plus README", () => {
  const dir = join(dirname(fileURLToPath(import.meta.url)), "..", "modules");
  const files = readdirSync(dir).sort();
  assert.deepEqual(files, [
    "DeviceDiscovery.js",
    "README.md",
    "appInfo.js",
    "lcd.js",
    "performance.js",
    "permissions.js",
    "serial.js",
    "systeminfo.js",
    "tcp.js",
    "udp.js",
  ]);
});

for (const [name, { mod, members, want }] of Object.entries(EXPECTED)) {
  test(`${name}: facade shape and declared members`, () => {
    assert.ok(mod, "default export exists");
    assert.deepEqual([...members], want);
    assert.ok(Object.isFrozen(members));
    assert.equal(mod.__lightloomFacade, name);
    assert.deepEqual([...mod.__lightloomMembers], want);
  });

  if (want.length > 0) {
    test(`${name}: every declared member throws HostApiMissing naming module and member, logged once`, () => {
      for (const m of want) {
        assert.equal(typeof mod[m], "function", m);
        assert.throws(
          () => mod[m](),
          (e) => e instanceof HostApiMissing && e.code === "API_GAP" && e.module === name && e.member === m && e.message.includes(`${name}.${m}`),
          `${name}.${m}`,
        );
        assert.throws(() => mod[m](1, "two", { three: 3 }), HostApiMissing);
      }
      assert.equal(logs.length, want.length, "one log line per member");
      for (const m of want) {
        assert.ok(logs.some((l) => l.includes(`API_GAP: ${name}.${m}`)), m);
      }
    });
  }
}

test("corpus call shapes fail loudly, not silently", () => {
  assert.throws(() => LCD.initialize({ width: 480, height: 480, circular: true }), (e) => e.member === "initialize");
  assert.throws(() => LCD.getFrame({ format: "NZXT::Q565" }), (e) => e.member === "getFrame");
  assert.throws(() => permissions.setCallback(() => {}), (e) => e.member === "setCallback");
  assert.throws(() => systeminfo.GetMotherboardInfo()?.model, (e) => e.member === "GetMotherboardInfo");
  assert.throws(
    () => DeviceDiscovery.foundVirtualDevice({ type: "aio", name: "x", supported: false, vendorId: 0x0b05 }),
    (e) => e.member === "foundVirtualDevice",
  );
  assert.throws(() => serial.connect({ baudRate: 115200, dataBits: 8, stopBits: "One", parity: "None" }), (e) => e.member === "connect");
  assert.throws(() => udp.send("192.168.1.2", 4003, [1, 2, 3], true), (e) => e.member === "send");
  assert.throws(() => udp.createSocket(), (e) => e.member === "createSocket");
  assert.throws(() => tcp.createSocket(), (e) => e.member === "createSocket");
});

test("undeclared members on a facade still throw with their own name", () => {
  assert.throws(() => serial.flush(), (e) => e instanceof HostApiMissing && e.member === "flush" && e.module === "@SignalRGB/serial");
  assert.throws(() => LCD.somethingNew(), (e) => e.member === "somethingNew");
});

test("appInfo throws on any property access since no member is evidenced", () => {
  assert.throws(() => appInfo.version, (e) => e instanceof HostApiMissing && e.module === "@SignalRGB/appInfo" && e.member === "version");
  assert.throws(() => appInfo.compat, (e) => e.member === "compat");
  assert.equal(typeof appInfo.then, "undefined");
});

test("importing a facade never logs or throws by itself", () => {
  assert.equal(logs.length, 0);
  assert.doesNotThrow(() => JSON.stringify({ udp, tcp, serial, LCD, permissions, systeminfo, DeviceDiscovery, performance, appInfo }));
  assert.equal(logs.length, 0);
});
