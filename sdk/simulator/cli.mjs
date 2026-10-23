#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-or-later
//
// huedeck-sim: run a plugin against mock hardware and print what it sends.
//
//   node sdk/simulator/cli.mjs <plugin-dir> [--color #RRGGBB] [--frames N] [--param name=value]...
//        [--pid 0x1234] [--endpoint iface:usage:usagePage[:collection]]... [--suspend]
//        [--module file.js] [--bus bus.json] [--json]
//
// --bus takes a JSON file of plugins/_testkit/mock-bus.mjs createMockBus() options for SMBus
// plugins, e.g. {"kind": "intel", "registers": {"57": {"0": 165}}}.
// Exit code: 0 the run went through, 1 a lifecycle call threw or nothing was found, 2 bad arguments.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { formatRun, simulate } from "./simulate.mjs";

const USAGE =
  "usage: huedeck-sim <plugin-dir> [--color #RRGGBB] [--frames N] [--param name=value]... [--pid ID] " +
  "[--endpoint iface:usage:usagePage[:collection]]... [--suspend] [--module file.js] [--bus bus.json] [--json]";

function parseValue(text) {
  if (text === "true" || text === "false") return text === "true";
  if (/^-?\d+(\.\d+)?$/.test(text) || /^0x[0-9a-f]+$/i.test(text)) return Number(text);
  return text;
}

function parseArgs(argv) {
  const opts = { params: {} };
  const endpoints = [];
  let dir;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) throw new Error(`${a} needs a value`);
      return argv[++i];
    };
    switch (a) {
      case "--color": opts.color = next(); break;
      case "--frames": opts.frames = Number(next()); break;
      case "--pid": opts.productId = Number(next()); break;
      case "--suspend": opts.suspending = true; break;
      case "--module": opts.module = next(); break;
      case "--json": opts.json = true; break;
      case "--bus": opts.bus = JSON.parse(readFileSync(next(), "utf8")); break;
      case "--param": {
        const [name, ...rest] = next().split("=");
        if (!name || rest.length === 0) throw new Error("--param takes name=value");
        opts.params[name] = parseValue(rest.join("="));
        break;
      }
      case "--endpoint": {
        const parts = next().split(":").map(Number);
        if (parts.length < 3 || parts.some(Number.isNaN)) throw new Error("--endpoint takes iface:usage:usagePage[:collection]");
        endpoints.push({ interface: parts[0], usage: parts[1], usage_page: parts[2], collection: parts[3] ?? 0 });
        break;
      }
      case "-h":
      case "--help":
        opts.help = true;
        break;
      default:
        if (a.startsWith("-") || dir !== undefined) throw new Error(`unexpected argument ${a}`);
        dir = a;
    }
  }
  if (endpoints.length) opts.endpoints = endpoints;
  return { dir, opts };
}

async function main(argv) {
  let parsed;
  try {
    parsed = parseArgs(argv);
  } catch (e) {
    console.error(`${e.message}\n${USAGE}`);
    return 2;
  }
  if (parsed.opts.help) {
    console.log(USAGE);
    return 0;
  }
  if (parsed.dir === undefined) {
    console.error(USAGE);
    return 2;
  }
  let run;
  try {
    run = await simulate(resolve(parsed.dir), parsed.opts);
  } catch (e) {
    console.error(e.message);
    return 2;
  }
  console.log(parsed.opts.json ? JSON.stringify(run, null, 2) : formatRun(run));
  return run.ok ? 0 : 1;
}

process.exitCode = await main(process.argv.slice(2));
