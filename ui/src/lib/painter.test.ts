// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  EditQueue,
  UndoStack,
  capLabel,
  cellsBetween,
  looksLikeKeyboard,
  remember,
  slotsByCell,
  slotsInBox,
  type UndoStep,
} from "./painter.ts";
import type { LedEdit, LedSpot } from "./types";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

test("a drag becomes one request, and changes during a request wait for it", async () => {
  const sent: LedEdit[] = [];
  let release: () => void = () => {};
  let hold = false;
  const q = new EditQueue(async (e) => {
    sent.push(e);
    if (hold) await new Promise<void>((r) => (release = r));
  }, 10);
  q.set([[0, "#ff0000"]]);
  q.set([[1, "#ff0000"], [0, "#00ff00"]]);
  q.setMode("custom");
  assert.equal(sent.length, 0, "nothing before the delay");
  await sleep(30);
  assert.deepEqual(sent, [{ set: { 0: "#00ff00", 1: "#ff0000" }, mode: "custom" }]);

  hold = true;
  q.set([[2, "#0000ff"]]);
  await sleep(30);
  assert.equal(sent.length, 2, "in flight");
  q.set([[3, "#0000ff"]]);
  q.set([[4, "#0000ff"]]);
  await sleep(30);
  assert.equal(sent.length, 2, "never two requests at a time");
  hold = false;
  release();
  await sleep(40);
  assert.deepEqual(sent[2], { set: { 3: "#0000ff", 4: "#0000ff" } });
  assert.equal(q.size, 0);
});

test("fill replaces what is pending; flush sends at once", async () => {
  const sent: LedEdit[] = [];
  const q = new EditQueue(async (e) => void sent.push(e), 1000);
  q.set([[0, "#ff0000"]]);
  q.fill("#000000");
  q.set([[1, "#ffffff"]]);
  await q.flush();
  assert.deepEqual(sent, [{ fill: "#000000", set: { 1: "#ffffff" } }]);
  await q.flush();
  assert.equal(sent.length, 1, "nothing left");
  q.dispose();
});

test("a failed request is reported and the queue goes on", async () => {
  const errors: unknown[] = [];
  let n = 0;
  const q = new EditQueue(
    async () => {
      n++;
      if (n === 1) throw new Error("busy");
    },
    5,
    (e) => errors.push(e),
  );
  q.set([[0, "#ffffff"]]);
  await q.flush();
  q.set([[1, "#ffffff"]]);
  await q.flush();
  assert.equal(n, 2);
  assert.equal(errors.length, 1);
});

test("undo keeps the first old colour of each slot of a step", () => {
  const colors = ["#111111", "#222222", "#333333"];
  const step: UndoStep = new Map();
  remember(step, colors, [0, 1]);
  colors[0] = "#ffffff";
  remember(step, colors, [0, 2]);
  assert.deepEqual([...step], [[0, "#111111"], [1, "#222222"], [2, "#333333"]]);
  const u = new UndoStack(2);
  u.push(new Map());
  assert.equal(u.length, 0, "empty steps are not kept");
  u.push(step);
  u.push(new Map([[0, "#000000"]]));
  u.push(new Map([[1, "#000000"]]));
  assert.equal(u.length, 2, "the oldest step went");
  assert.deepEqual([...u.pop()!], [[1, "#000000"]]);
});

test("geometry: lines of cells, cells to slots, box selection", () => {
  assert.deepEqual(cellsBetween([0, 0], [3, 1]), [
    [0, 0],
    [1, 0],
    [2, 1],
    [3, 1],
  ]);
  assert.deepEqual(cellsBetween([2, 2], [2, 2]), [[2, 2]]);
  const leds: LedSpot[] = [
    { slot: 0, x: 0, y: 0, name: "Esc" },
    { slot: 1, x: 1, y: 0, name: "F1" },
    { slot: 2, x: 1, y: 0 },
    { slot: 3, x: 0, y: 2 },
  ];
  assert.deepEqual(slotsByCell(leds).get("1,0"), [1, 2]);
  assert.deepEqual(slotsInBox(leds, [1, 1], [0, 0]), [0, 1, 2]);
  assert.deepEqual(slotsInBox(leds, [0, 2], [0, 2]), [3]);
});

test("key caps for keyboards and key-named LEDs only", () => {
  assert.equal(looksLikeKeyboard("keyboard", []), true);
  const strip: LedSpot[] = [1, 2, 3, 4, 5].map((i) => ({ slot: i, x: i, y: 0, name: `Led ${i}` }));
  assert.equal(looksLikeKeyboard("other", strip), false);
  const keys: LedSpot[] = ["Esc", "Tab", "A", "W", "Space"].map((name, i) => ({ slot: i, x: i, y: 0, name }));
  assert.equal(looksLikeKeyboard("other", keys), true);
  assert.equal(capLabel("Left Shift"), "Shift");
  assert.equal(capLabel("Backspace"), "Back");
  assert.equal(capLabel("F12"), "F12");
  assert.equal(capLabel(undefined), "");
});
