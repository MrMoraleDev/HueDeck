// SPDX-License-Identifier: GPL-3.0-or-later
// Test Input: application events, with both casings of the payload.
export function Input() {
  this.Name = "Effect Watch";
  this.Description = "Logs application events";
  this.Options = [];
  this.onApplication = function (event) {
    const d = event.Data;
    console.log(`${event.EventHandler}|${event.EventType}|${d === event.data}|${JSON.stringify(event.data)}`);
    if (event.EventType === "Effect Changed") block.trigger();
  };
}
