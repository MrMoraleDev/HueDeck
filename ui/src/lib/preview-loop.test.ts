// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import { PreviewLoop } from "./preview-loop.ts";

/** A loop with hand-driven timers and fake object URLs. */
function rig(answers: (Blob | null | Error)[]) {
  const pending: (() => void)[] = [];
  const shown: (string | null)[] = [];
  const revoked: string[] = [];
  let made = 0;
  let loads = 0;
  const loop = new PreviewLoop({
    every: 500,
    load: async () => {
      const a = answers[Math.min(loads++, answers.length - 1)];
      if (a instanceof Error) throw a;
      return a;
    },
    show: (u) => shown.push(u),
    createUrl: () => `blob:${++made}`,
    revokeUrl: (u) => revoked.push(u),
    schedule: (fn) => {
      pending.push(fn);
      return pending.length;
    },
    cancel: () => {
      pending.length = 0;
    },
  });
  const settle = () => new Promise((r) => setTimeout(r, 0));
  const fire = async () => {
    const fn = pending.shift();
    assert.ok(fn, "a fetch is scheduled");
    fn();
    await settle();
  };
  return { loop, shown, revoked, pending, settle, fire, loads: () => loads };
}

test("fetches one picture at a time and swaps object URLs without a gap", async () => {
  const png = new Blob(["png"]);
  const r = rig([png, png, png]);
  r.loop.start();
  r.loop.start();
  await r.settle();
  assert.equal(r.loads(), 1, "start twice: one fetch");
  assert.deepEqual(r.shown, ["blob:1"]);
  assert.equal(r.pending.length, 1, "the next fetch waits for the interval");
  await r.fire();
  assert.deepEqual(r.shown, ["blob:1", "blob:2"]);
  assert.deepEqual(r.revoked, ["blob:1"], "the old picture is released after the new one shows");
});

test("stops while hidden and survives failures", async () => {
  const png = new Blob(["png"]);
  const r = rig([new Error("offline"), png, null]);
  r.loop.start();
  await r.settle();
  assert.deepEqual(r.shown, [], "a failed fetch shows nothing");
  assert.equal(r.pending.length, 1, "and tries again");
  await r.fire();
  assert.deepEqual(r.shown, ["blob:1"]);
  r.loop.stop();
  assert.equal(r.pending.length, 0, "stopped: nothing scheduled");
  assert.equal(r.loop.active, false);
  r.loop.start();
  await r.settle();
  assert.deepEqual(r.shown, ["blob:1", null], "no LCD any more: no picture");
  assert.deepEqual(r.revoked, ["blob:1"]);
  r.loop.dispose();
  assert.equal(r.pending.length, 0);
});
