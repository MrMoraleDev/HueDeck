// SPDX-License-Identifier: GPL-3.0-or-later
//
// sdk/simulator: the lifecycle order, the recorded HID and SMBus traffic for a solid colour,
// parameter defaults and overrides, failures, and the CLI.

import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { formatRun, parseColor, simulate } from "../simulate.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const sdk = join(here, "..", "..");
const strip = join(sdk, "examples", "hello-strip");
const smbusFixture = join(sdk, "lint", "tests", "fixtures", "good-smbus");
const PRESENT = { kind: "intel", registers: { 0x39: { 0x00: 0xa5 } } };

test("parseColor", () => {
  assert.deepEqual(parseColor("#FF8000"), [255, 128, 0]);
  assert.deepEqual(parseColor("102030"), [16, 32, 48]);
  assert.deepEqual(parseColor([1, 2, 3, 4]), [1, 2, 3]);
  assert.throws(() => parseColor("red"));
});

test("a HID plugin: Validate, Initialize, Render x N, Shutdown, with the bytes it writes", async () => {
  const run = await simulate(strip, { color: "#FF8000", frames: 2 });
  assert.equal(run.ok, true, formatRun(run));
  assert.deepEqual(run.phases.map((p) => p.phase), [
    "Validate(interface 0, usage 0x0001, usage_page 0xFF00, collection 0)",
    "Initialize()",
    "Render() #1",
    "Render() #2",
    "Shutdown(false)",
  ]);
  assert.equal(run.vendorId, 0x1209);
  assert.equal(run.productId, 0x0001);
  const render = run.phases[2].traffic;
  assert.match(render[0], /^write +65 B {2}00 01 08 (FF 80 00 ){8}00/);
  assert.equal(render[1], "(8 canvas samples)");
  assert.match(run.phases[4].traffic[0], /^write +65 B {2}00 01 08 (00 00 00 ){8}/);
});

test("parameter defaults are globals before Initialize; overrides win", async () => {
  const forced = await simulate(strip, { params: { LightingMode: "Forced", forcedColor: "#010203" }, suspending: true });
  assert.match(forced.phases[2].traffic[0], /00 01 08 (01 02 03 ){8}/);
  assert.equal(forced.phases.at(-1).phase, "Shutdown(true)");
  assert.equal(globalThis.LightingMode, undefined, "parameter globals are removed after the run");
});

test("Validate refusing every endpoint stops the run", async () => {
  const run = await simulate(strip, { endpoints: [{ interface: 2, usage: 1, usage_page: 0xff00, collection: 0 }] });
  assert.equal(run.ok, false);
  assert.equal(run.phases.length, 1);
  assert.match(run.log.join("\n"), /accepted no endpoint/);
});

test("an SMBus plugin: Scan on the free bus, then the session on the fixed bus", async () => {
  const run = await simulate(smbusFixture, { color: "#102030", bus: PRESENT });
  assert.equal(run.ok, true, formatRun(run));
  assert.equal(run.address, 0x39);
  assert.deepEqual(run.phases[0].result, [0x39]);
  assert.deepEqual(run.phases[2].traffic.slice(0, 4), [
    "WriteByte                @0x39 reg 0x10 <- 0x10 [locked]",
    "WriteByte                @0x39 reg 0x11 <- 0x20 [locked]",
    "WriteByte                @0x39 reg 0x12 <- 0x30 [locked]",
    "WriteByte                @0x39 reg 0x20 <- 0x01 [locked]",
  ]);
});

test("an SMBus plugin whose Scan finds nothing fails without a session", async () => {
  const run = await simulate(smbusFixture, { bus: { kind: "nvidia" } });
  assert.equal(run.ok, false);
  assert.deepEqual(run.phases.map((p) => p.phase), ["Scan(bus)"]);
  assert.match(run.log.join("\n"), /found no device at 0x39/);
});

test("a throwing lifecycle call is reported and ends the run", async () => {
  const run = await simulate(join(sdk, "lint", "tests", "fixtures", "bad-exports"));
  assert.equal(run.ok, false);
  const init = run.phases.find((p) => p.phase === "Initialize()");
  assert.match(init.error, /ReferenceError: setTimeout is not defined/, formatRun(run));
});

test("the CLI prints the run and exits 0, or 1 when it failed", () => {
  const cli = join(sdk, "simulator", "cli.mjs");
  const text = execFileSync(process.execPath, [cli, strip, "--color", "#00FF00", "--param", "LightingMode=Canvas"], { encoding: "utf8" });
  assert.match(text, /^Hello Strip: Hid 1209:0001, canvas #00FF00/);
  assert.match(text, /00 01 08 (00 FF 00 ){8}/);
  assert.match(text, /\nok\n?$/);
  const json = JSON.parse(execFileSync(process.execPath, [cli, strip, "--json", "--frames", "0"], { encoding: "utf8" }));
  assert.deepEqual(json.phases.map((p) => p.phase).slice(1), ["Initialize()", "Shutdown(false)"]);
  assert.throws(() => execFileSync(process.execPath, [cli, smbusFixture], { stdio: "pipe" }), (e) => e.status === 1);
  assert.throws(() => execFileSync(process.execPath, [cli, "--frames"], { stdio: "pipe" }), (e) => e.status === 2);
});
