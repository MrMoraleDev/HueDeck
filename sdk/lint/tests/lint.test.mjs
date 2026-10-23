// SPDX-License-Identifier: GPL-3.0-or-later
//
// sdk/lint on its fixtures: clean plugins pass, each broken one reports the expected findings,
// the schema subset and the source scan behave, and the CLI exits with the right code.

import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { formatResult, lintPlugin, pluginDirs, scanForbiddenGlobals, stripCode } from "../lint.mjs";
import { validate } from "../schema.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const fixtures = join(here, "fixtures");
const sdk = join(here, "..", "..");
const codes = (r, level) => r.findings.filter((f) => !level || f.level === level).map((f) => f.code);
const has = (r, code, re) => r.findings.some((f) => f.code === code && (!re || re.test(`${f.where ?? ""} ${f.message}`)));

test("the SDK example plugin is clean", async () => {
  const r = await lintPlugin(join(sdk, "examples", "hello-strip"));
  assert.deepEqual(r.findings, [], formatResult(r));
  assert.equal(r.id, "example.hello-strip");
  assert.equal(r.entry, "plugin.js");
});

test("an SMBus plugin whose exports match its manifest is clean", async () => {
  const r = await lintPlugin(join(fixtures, "good-smbus"));
  assert.deepEqual(r.findings, [], formatResult(r));
});

test("sibling plugin modules count toward the manifest's USB claims; helpers do not", async () => {
  const r = await lintPlugin(join(fixtures, "multi-module"));
  assert.deepEqual(r.findings, [], formatResult(r));
  assert.deepEqual(r.modules, ["plugin.js", "second.js"]);
});

test("manifest problems: schema, entry, licence, tested ids, SMBus range", async () => {
  const r = await lintPlugin(join(fixtures, "bad-manifest"));
  assert.ok(has(r, "schema", /^\/id .*does not match/));
  assert.ok(has(r, "schema", /\/version/));
  assert.ok(has(r, "schema", /\/devices\/0\/pids\/1/));
  assert.ok(has(r, "schema", /teleport/));
  assert.ok(has(r, "schema", /key 'colour' is not allowed/));
  assert.ok(has(r, "entry-missing"));
  assert.ok(has(r, "license-parens"));
  assert.ok(has(r, "tested-unclaimed", /0x0003/));
  assert.ok(has(r, "smbus-address", /0x02/));
  assert.ok(!has(r, "smbus-address", /0x40/), "0x40 is a valid address");
  assert.ok(has(r, "permission", /'smbus'/));
});

test("export problems and manifest mismatches", async () => {
  const r = await lintPlugin(join(fixtures, "bad-exports"));
  const errors = codes(r, "error");
  for (const code of ["export-throws", "missing-export", "led-count", "parameters", "pid-unclaimed", "pid-unexported", "forbidden-global"]) {
    assert.ok(errors.includes(code), `${code} in ${errors}`);
  }
  assert.ok(has(r, "missing-export", /Shutdown/));
  assert.ok(has(r, "export-throws", /Version/));
  assert.ok(has(r, "led-count", /3 names but .* 2 positions/));
  assert.ok(has(r, "led-overlap"));
  assert.ok(has(r, "parameters", /missing required key 'values'/));
  assert.ok(has(r, "parameters", /declared twice/));
  assert.ok(has(r, "parameters", /replace the host global/));
  assert.ok(has(r, "parameters", /slider/));
  assert.ok(has(r, "pid-unclaimed", /0x0004/));
  assert.ok(has(r, "pid-unexported", /0x0003/));
  assert.ok(has(r, "device-type", /keyboard/));
  assert.ok(has(r, "permission", /'hid'/));
  // window in a comment, document in a string and the typeof guard do not count.
  const globals = r.findings.filter((f) => f.code === "forbidden-global").map((f) => f.where);
  assert.deepEqual(globals, ["plugin.js:25", "plugin.js:29"]);
});

test("a plugin without a manifest is linted from its exports, with @SignalRGB imports resolved", async () => {
  const r = await lintPlugin(join(fixtures, "no-manifest"));
  assert.deepEqual(codes(r, "error"), [], formatResult(r));
  assert.deepEqual(codes(r, "warning").sort(), ["missing-export", "no-manifest"]);
});

test("pluginDirs finds the plugins of a directory", () => {
  const names = pluginDirs(fixtures).map((d) => d.slice(fixtures.length + 1)).sort();
  assert.deepEqual(names, ["bad-exports", "bad-manifest", "good-smbus", "multi-module", "no-manifest"]);
  assert.deepEqual(pluginDirs(join(fixtures, "good-smbus")), [join(fixtures, "good-smbus")]);
});

test("schema subset: types, refs, oneOf, if/then, additionalProperties", () => {
  const schema = {
    type: "object",
    required: ["a"],
    properties: {
      a: { $ref: "#/$defs/id" },
      b: { type: "array", uniqueItems: true, items: { type: "integer", minimum: 0, maximum: 9 } },
      c: { oneOf: [{ type: "string" }, { type: "number" }] },
      d: { type: "object", properties: { kind: { enum: ["x", "y"] } }, if: { properties: { kind: { const: "x" } } }, then: { required: ["n"] } },
    },
    additionalProperties: false,
    $defs: { id: { type: "string", pattern: "^[a-z]+$", minLength: 2 } },
  };
  assert.deepEqual(validate(schema, { a: "ok", b: [1, 2], c: "s", d: { kind: "y" } }), []);
  const errs = validate(schema, { a: "X", b: [1, 1, 10], c: true, d: { kind: "x" }, e: 1 });
  const paths = errs.map((e) => e.path);
  for (const p of ["/a", "/b/1", "/b/2", "/c", "/d", "/e"]) assert.ok(paths.includes(p), `${p} in ${paths}`);
  assert.deepEqual(validate(schema, {}).map((e) => e.message), ["is missing required key 'a'"]);
});

test("stripCode keeps line numbers and blanks comments, strings and regex literals", () => {
  const src = 'const a = "window"; // window\n/* document\n */ const r = /window/g; const t = `fetch`;\nwindow.x = a / 2;';
  const out = stripCode(src);
  assert.equal(out.split("\n").length, src.split("\n").length);
  assert.ok(!/window|document|fetch/.test(out.split("\n").slice(0, 3).join("\n")));
  assert.deepEqual(scanForbiddenGlobals(src), [{ name: "window", line: 4 }]);
});

test("scanForbiddenGlobals ignores locals, properties, object keys and typeof guards", () => {
  const src = [
    "import { process } from './x.js';",
    "export function Image() { return ''; }",
    "const o = { document: 1, fetch: 2 };",
    "o.window = 3; o?.navigator;",
    "if (typeof require === 'function') {}",
    "process(); Image();",
    "Buffer.from([]);",
  ].join("\n");
  assert.deepEqual(scanForbiddenGlobals(src), [{ name: "Buffer", line: 7 }]);
});

test("the CLI reports and exits 1 on errors, 0 on a clean plugin, 2 on bad arguments", () => {
  const cli = join(sdk, "lint", "cli.mjs");
  const ok = execFileSync(process.execPath, [cli, join(sdk, "examples", "hello-strip")], { encoding: "utf8" });
  assert.match(ok, /hello-strip \(example\.hello-strip 0\.1\.0\)\n {2}ok/);
  let failed;
  try {
    execFileSync(process.execPath, [cli, "--json", join(fixtures, "bad-exports")], { encoding: "utf8" });
  } catch (e) {
    failed = e;
  }
  assert.equal(failed?.status, 1);
  const report = JSON.parse(failed.stdout);
  assert.equal(report.results.length, 1);
  assert.ok(report.errors > 0);
  let bad;
  try {
    execFileSync(process.execPath, [cli], { encoding: "utf8", stdio: "pipe" });
  } catch (e) {
    bad = e;
  }
  assert.equal(bad?.status, 2);
  // --strict: warnings fail too.
  assert.throws(() => execFileSync(process.execPath, [cli, "--strict", join(fixtures, "no-manifest")], { stdio: "pipe" }));
});

test("save and exit modes: the first-party MSI GPU plugin is clean, mismatches are errors", async () => {
  const { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const gpu = join(sdk, "..", "plugins", "msi-gpu");
  const clean = await lintPlugin(gpu);
  assert.deepEqual(clean.findings.filter((f) => f.level === "error"), [], formatResult(clean));
  const dir = mkdtempSync(join(tmpdir(), "huedeck-lint-save-"));
  try {
    cpSync(gpu, dir, { recursive: true });
    const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8"));
    manifest.save = { static: true, effects: ["spectrum"] };
    manifest.shutdownModes = ["restore"];
    writeFileSync(join(dir, "manifest.json"), JSON.stringify(manifest));
    const r = await lintPlugin(dir);
    assert.ok(has(r, "save-mismatch"), formatResult(r));
    assert.ok(has(r, "shutdown-modes-mismatch"), formatResult(r));
    const source = readFileSync(join(dir, "plugin.js"), "utf8").replace("export function SaveToDevice(", "function SaveToDevice(");
    writeFileSync(join(dir, "plugin.js"), source);
    assert.ok(has(await lintPlugin(dir), "save-without-export"));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
