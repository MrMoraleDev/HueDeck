// SPDX-License-Identifier: GPL-3.0-or-later
// Test Action: presses the configured key combination.
export function Action() {
  this.Name = "Press Combo";
  this.Description = "Presses a key combination";
  this.Options = [{ property: "Combo", label: "Keys", type: "keycode", default: "Ctrl + C" }];
  this.onExecute = function () {
    keys.press(this.Combo);
  };
}
