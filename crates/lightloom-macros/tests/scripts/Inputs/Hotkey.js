// SPDX-License-Identifier: GPL-3.0-or-later
// Test Input: a key combination triggers the chain and is swallowed.
export function Input() {
  this.Name = "Hotkey";
  this.Description = "Triggers on a key combination";
  this.Options = [
    { property: "Target", label: "Keys", type: "keycode", default: "Ctrl + F1" },
    { property: "Eat", label: "Discard the key", type: "boolean", default: true },
  ];
  this.onKey = function (event) {
    if (event.data.released) return false;
    if (event.data.keyString === this.Target || event.Data.key === "G1") {
      block.trigger();
      return this.Eat;
    }
    return false;
  };
}
