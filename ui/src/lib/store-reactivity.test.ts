// SPDX-License-Identifier: GPL-3.0-or-later
// The dashboard's refresh path under Svelte's real runtime: a refetched snapshot that
// differs only in counters must not re-run what reads the rest (lists, selects, sliders).
// Runs with `node --conditions=browser` (Svelte's client runtime).
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { flushSync } from "svelte";
import { compileModule } from "svelte/compiler";

const here = fileURLToPath(new URL(".", import.meta.url));
// Compiled inside node_modules so that `svelte/internal/client` resolves.
const outDir = fileURLToPath(new URL("../../node_modules/.lightloom-test/", import.meta.url));

const SOURCE = `
import { reconcile } from ${JSON.stringify(pathToFileURL(`${here}reconcile.ts`).href)};
export function setup(initial, replace) {
  const box = $state({ s: initial });
  const runs = { keys: 0, brightness: 0, frames: 0, options: 0 };
  const stop = $effect.root(() => {
    $effect(() => { box.s.devices.map((d) => d.key); runs.keys++; });
    $effect(() => { box.s.lighting.brightness; runs.brightness++; });
    $effect(() => { box.s.devices[0].frames; runs.frames++; });
    $effect(() => { box.s.devices[0].parameters[0].values.join(); runs.options++; });
  });
  const update = (next) => (replace ? (box.s = next) : reconcile(box.s, next));
  return { runs, stop, update };
}
`;

async function load(): Promise<{
  setup: (initial: unknown, replace: boolean) => { runs: Record<string, number>; stop: () => void; update: (n: unknown) => void };
}> {
  mkdirSync(outDir, { recursive: true });
  const { js } = compileModule(SOURCE, { filename: "harness.svelte.js", generate: "client" });
  const file = `${outDir}harness.mjs`;
  writeFileSync(file, js.code);
  return import(pathToFileURL(file).href);
}

function snapshot(frames: number) {
  return {
    lighting: { brightness: 80 },
    devices: [
      { key: "1462:7c91:x", frames, parameters: [{ property: "mode", values: ["Static", "Breathing"] }] },
      { key: "1038:1836:y", frames: 3, parameters: [{ property: "mode", values: ["Static"] }] },
    ],
  };
}

test("a reconciled refresh re-runs only what reads the changed counter", async () => {
  const { setup } = await load();
  const h = setup(snapshot(1), false);
  flushSync();
  assert.deepEqual(h.runs, { keys: 1, brightness: 1, frames: 1, options: 1 });
  for (let f = 2; f <= 10; f++) {
    h.update(snapshot(f));
    flushSync();
  }
  assert.deepEqual(h.runs, { keys: 1, brightness: 1, frames: 10, options: 1 });
  h.stop();
});

test("replacing the snapshot (the old refresh) re-ran everything each time", async () => {
  const { setup } = await load();
  const h = setup(snapshot(1), true);
  flushSync();
  for (let f = 2; f <= 10; f++) {
    h.update(snapshot(f));
    flushSync();
  }
  assert.deepEqual(h.runs, { keys: 10, brightness: 10, frames: 10, options: 10 });
  h.stop();
});

test("re-ordered and new devices under the runtime: one list update, state intact", async () => {
  const { setup } = await load();
  const h = setup(snapshot(1), false);
  flushSync();
  const next = snapshot(1);
  next.devices.reverse();
  next.devices.push({ key: "net:wled:abcd", frames: 0, parameters: [{ property: "p", values: ["a"] }] });
  h.update(next);
  flushSync();
  assert.equal(h.runs.keys, 2);
  assert.equal(h.runs.brightness, 1);
  h.stop();
});
