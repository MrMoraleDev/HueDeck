// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import { EventBus } from "./events.ts";
import { hasNamePicker, nameChoices } from "./macro-options.ts";

const snapshot = { layouts: ["Default", "Desk"], profiles: [{ name: "Gaming" }, { name: "Night" }] };

test("layout and profile options pick from the snapshot", () => {
  assert.deepEqual(nameChoices("layoutname", "Desk", snapshot), [
    { name: "Default", missing: false },
    { name: "Desk", missing: false },
  ]);
  assert.deepEqual(
    nameChoices("profilename", "", snapshot)?.map((c) => c.name),
    ["Gaming", "Night"],
  );
  assert.equal(nameChoices("textfield", "x", snapshot), null);
  assert.ok(hasNamePicker("profilename") && hasNamePicker("layoutname") && !hasNamePicker("effectname"));
});

test("a saved name that is gone stays first and is marked", () => {
  assert.deepEqual(nameChoices("profilename", "Old", snapshot)?.[0], { name: "Old", missing: true });
  assert.deepEqual(nameChoices("layoutname", "Old", null), [{ name: "Old", missing: true }]);
  assert.deepEqual(nameChoices("layoutname", 3, { layouts: [] }), []);
});

test("the event bus hands events to the listeners of their kind", () => {
  const bus = new EventBus();
  const got: unknown[] = [];
  const off = bus.on("macros", (d) => got.push(d));
  bus.on("macros", () => {
    throw new Error("a broken listener");
  });
  const error = console.error;
  console.error = () => {};
  try {
    bus.emit("macros", { scripts: [] });
    bus.emit("state", 1);
  } finally {
    console.error = error;
  }
  assert.deepEqual(got, [{ scripts: [] }]);
  off();
  bus.emit("macros", 2);
  assert.equal(got.length, 1);
  assert.ok(bus.has("macros"), "the other listener is still there");
});
