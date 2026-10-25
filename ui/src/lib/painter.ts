// SPDX-License-Identifier: GPL-3.0-or-later
// The LED painter's logic, free of the DOM (unit-tested in painter.test.ts): the
// coalescing write queue, undo, grid geometry and box selection.
//
// Painting is local first: the component changes its own colour array at once (one
// rect re-renders), and the queue sends the accumulated changes as one PATCH at most
// every `delay` ms, never two at a time. The daemon shows them from its next frame and
// saves them to the database after the burst.

import type { LedEdit, LedSpot } from "./types";

/** Black: what "Clear" paints. */
export const BLACK = "#000000";

/**
 * Coalesces LED edits into few requests. `fill` replaces what is pending (the daemon
 * applies `fill` before `set`, so later single-LED changes still win); `mode` rides
 * along with the next request.
 */
export class EditQueue {
  private fillColor: string | null = null;
  private pending = new Map<number, string>();
  private mode: "custom" | "effect" | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private inFlight: Promise<void> | null = null;
  private send: (edit: LedEdit) => Promise<unknown>;
  private delay: number;
  private onError: (e: unknown) => void;

  constructor(send: (edit: LedEdit) => Promise<unknown>, delay = 40, onError: (e: unknown) => void = () => {}) {
    this.send = send;
    this.delay = delay;
    this.onError = onError;
  }

  /** Changes waiting to be sent. */
  get size(): number {
    return this.pending.size + (this.fillColor === null ? 0 : 1) + (this.mode === null ? 0 : 1);
  }

  set(changes: Iterable<[number, string]>) {
    for (const [slot, color] of changes) this.pending.set(slot, color);
    this.schedule();
  }

  fill(color: string) {
    this.fillColor = color;
    this.pending.clear();
    this.schedule();
  }

  setMode(mode: "custom" | "effect") {
    this.mode = mode;
    this.schedule();
  }

  private schedule() {
    if (this.timer !== null || this.inFlight) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.flush();
    }, this.delay);
  }

  /** Send what is pending now (after the request in flight, if any). */
  async flush(): Promise<void> {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    while (this.inFlight) await this.inFlight;
    if (this.size === 0) return;
    const edit: LedEdit = {};
    if (this.fillColor !== null) edit.fill = this.fillColor;
    if (this.pending.size) edit.set = Object.fromEntries(this.pending);
    if (this.mode !== null) edit.mode = this.mode;
    this.fillColor = null;
    this.pending = new Map();
    this.mode = null;
    this.inFlight = (async () => {
      try {
        await this.send(edit);
      } catch (e) {
        this.onError(e);
      }
    })();
    await this.inFlight;
    this.inFlight = null;
    // Changes made while the request was out go next.
    if (this.size) this.schedule();
  }

  /** Drop the timer (the painter closes); `flush()` first to keep the changes. */
  dispose() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}

/** One undoable step: the colours the slots had before it. */
export type UndoStep = Map<number, string>;

/** Undo history, newest last, at most `limit` steps. */
export class UndoStack {
  private steps: UndoStep[] = [];
  private limit: number;

  constructor(limit = 100) {
    this.limit = limit;
  }

  get length(): number {
    return this.steps.length;
  }

  push(step: UndoStep) {
    if (step.size === 0) return;
    this.steps.push(step);
    if (this.steps.length > this.limit) this.steps.shift();
  }

  pop(): UndoStep | undefined {
    return this.steps.pop();
  }

  clear() {
    this.steps = [];
  }
}

/** Record the old colours of `slots` into `step` (the first change of a slot wins). */
export function remember(step: UndoStep, colors: readonly string[], slots: Iterable<number>) {
  for (const s of slots) if (!step.has(s)) step.set(s, colors[s] ?? BLACK);
}

/** The cells on the straight line from `a` to `b` (Bresenham), both ends included. */
export function cellsBetween(a: [number, number], b: [number, number]): [number, number][] {
  let [x0, y0] = a;
  const [x1, y1] = b;
  const dx = Math.abs(x1 - x0);
  const dy = -Math.abs(y1 - y0);
  const sx = x0 < x1 ? 1 : -1;
  const sy = y0 < y1 ? 1 : -1;
  let err = dx + dy;
  const out: [number, number][] = [];
  for (let guard = 0; guard < 10_000; guard++) {
    out.push([x0, y0]);
    if (x0 === x1 && y0 === y1) break;
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      x0 += sx;
    }
    if (e2 <= dx) {
      err += dx;
      y0 += sy;
    }
  }
  return out;
}

/** Slots by cell (`"x,y"`): more than one LED may sit in a cell. */
export function slotsByCell(leds: readonly LedSpot[]): Map<string, number[]> {
  const m = new Map<string, number[]>();
  for (const l of leds) {
    const k = `${l.x},${l.y}`;
    const list = m.get(k);
    if (list) list.push(l.slot);
    else m.set(k, [l.slot]);
  }
  return m;
}

/** Slots of the LEDs whose cell lies in the box between two cell corners (inclusive). */
export function slotsInBox(leds: readonly LedSpot[], a: [number, number], b: [number, number]): number[] {
  const [x0, x1] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const [y0, y1] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  return leds.filter((l) => l.x >= x0 && l.x <= x1 && l.y >= y0 && l.y <= y1).map((l) => l.slot);
}

/** Names keyboards give their keys; a few of them mean the positions are a keyboard. */
const KEY_NAMES = new Set(
  ["esc", "escape", "space", "spacebar", "enter", "return", "tab", "caps lock", "backspace", "left shift", "right shift", "f1", "a", "w", "s", "d"],
);

/**
 * Draw the endpoint as key caps (labelled squares) rather than dots: a keyboard
 * device, or LEDs named like keys. The positions are drawn as they are; nothing is
 * laid out by name.
 */
export function looksLikeKeyboard(deviceType: string, leds: readonly LedSpot[]): boolean {
  if (deviceType.toLowerCase() === "keyboard") return true;
  let hits = 0;
  for (const l of leds) {
    if (l.name && KEY_NAMES.has(l.name.trim().toLowerCase())) hits++;
    if (hits >= 4) return true;
  }
  return false;
}

/** A short label for a key cap: the name without "Left"/"Right", cut to five characters. */
export function capLabel(name: string | undefined): string {
  const n = (name ?? "").trim().replace(/^(left|right)\s+/i, "");
  return n.length <= 5 ? n : n.slice(0, 4);
}
