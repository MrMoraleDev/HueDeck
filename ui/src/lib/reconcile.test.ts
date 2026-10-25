// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import { reconcile } from "./reconcile.ts";

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

test("changes only the leaves that differ and keeps every object", () => {
  const target = {
    version: "0.1.0",
    lighting: { brightness: 80, renderer: { fps: 30 } },
    devices: [
      { key: "a", frames: 1, parameters: [{ property: "mode", values: ["x", "y"] }] },
      { key: "b", frames: 5, parameters: [] },
    ],
  };
  const lighting = target.lighting;
  const devices = target.devices;
  const [a, b] = devices;
  const values = a.parameters[0].values;
  const next = clone(target);
  next.devices[0].frames = 2;
  next.lighting.renderer.fps = 29;
  reconcile(target, next);
  assert.deepEqual(target, next);
  assert.equal(target.lighting, lighting);
  assert.equal(target.devices, devices);
  assert.equal(target.devices[0], a);
  assert.equal(target.devices[1], b);
  assert.equal(target.devices[0].parameters[0].values, values);
});

test("re-ordered keyed elements keep their identity; new and removed ones follow", () => {
  const target = { devices: [{ key: "a", n: 1 }, { key: "b", n: 2 }, { key: "c", n: 3 }] };
  const [a, , c] = target.devices;
  reconcile(target, { devices: [{ key: "c", n: 3 }, { key: "d", n: 4 }, { key: "a", n: 9 }] });
  assert.deepEqual(target.devices, [{ key: "c", n: 3 }, { key: "d", n: 4 }, { key: "a", n: 9 }]);
  assert.equal(target.devices[0], c);
  assert.equal(target.devices[2], a);
});

test("an unchanged keyed array is not replaced", () => {
  const target = { list: [{ id: "x", v: 1 }] };
  const list = target.list;
  reconcile(target, { list: [{ id: "x", v: 2 }] });
  assert.equal(target.list, list);
  assert.equal(target.list[0].v, 2);
});

test("arrays without identity grow, shrink and update by index", () => {
  const target = { values: ["a", "b", "c"], entries: [{ effect: "e" }, { effect: "e" }] };
  const values = target.values;
  const first = target.entries[0];
  reconcile(target, { values: ["a", "z"], entries: [{ effect: "f" }, { effect: "e" }, { effect: "g" }] });
  assert.deepEqual(target, { values: ["a", "z"], entries: [{ effect: "f" }, { effect: "e" }, { effect: "g" }] });
  assert.equal(target.values, values);
  assert.equal(target.entries[0], first);
});

test("keys missing from the source are removed; type changes replace", () => {
  const target: Record<string, unknown> = { a: 1, gone: true, nested: { x: 1 }, list: [1] };
  reconcile(target, { a: 1, nested: null, list: { now: "object" } });
  assert.deepEqual(target, { a: 1, nested: null, list: { now: "object" } });
});

test("duplicate ids fall back to index order", () => {
  const target = { list: [{ id: "x", v: 1 }, { id: "x", v: 2 }] };
  reconcile(target, { list: [{ id: "x", v: 3 }, { id: "x", v: 4 }] });
  assert.deepEqual(target.list, [{ id: "x", v: 3 }, { id: "x", v: 4 }]);
});
