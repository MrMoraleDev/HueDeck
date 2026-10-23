#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-or-later
//
// lightloom-lint: check Lightloom plugin directories.
//
//   node sdk/lint/cli.mjs <dir>... [--json] [--strict]
//
// A <dir> is a plugin directory (manifest.json and/or plugin.js) or a directory of them
// (e.g. `plugins/`). Exit code: 0 clean, 1 errors found (with --strict: warnings too),
// 2 bad arguments.

import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { formatResult, lintPlugin, pluginDirs } from "./lint.mjs";

const USAGE = "usage: lightloom-lint <plugin-dir>... [--json] [--strict]";

async function main(argv) {
  const json = argv.includes("--json");
  const strict = argv.includes("--strict");
  const unknown = argv.filter((a) => a.startsWith("-") && !["--json", "--strict", "--help", "-h"].includes(a));
  const dirs = argv.filter((a) => !a.startsWith("-"));
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log(USAGE);
    return 0;
  }
  if (unknown.length > 0 || dirs.length === 0) {
    console.error(unknown.length > 0 ? `unknown option ${unknown[0]}\n${USAGE}` : USAGE);
    return 2;
  }
  const targets = [];
  for (const d of dirs) {
    const abs = resolve(d);
    if (!existsSync(abs)) {
      console.error(`no such directory: ${d}`);
      return 2;
    }
    const found = pluginDirs(abs);
    if (found.length === 0) {
      console.error(`no plugin in ${d} (expected manifest.json or plugin.js)`);
      return 2;
    }
    targets.push(...found);
  }
  const results = [];
  for (const dir of targets) results.push(await lintPlugin(dir));
  const errors = results.reduce((n, r) => n + r.findings.filter((f) => f.level === "error").length, 0);
  const warnings = results.reduce((n, r) => n + r.findings.filter((f) => f.level === "warning").length, 0);
  if (json) {
    console.log(JSON.stringify({ results, errors, warnings }, null, 2));
  } else {
    const color = process.stdout.isTTY === true && !process.env.NO_COLOR;
    console.log(results.map((r) => formatResult(r, { color })).join("\n\n"));
    if (results.length > 1) console.log(`\n${results.length} plugins: ${errors} error(s), ${warnings} warning(s)`);
  }
  return errors > 0 || (strict && warnings > 0) ? 1 : 0;
}

process.exitCode = await main(process.argv.slice(2));
