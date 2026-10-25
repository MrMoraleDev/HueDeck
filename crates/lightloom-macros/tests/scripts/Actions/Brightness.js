// SPDX-License-Identifier: GPL-3.0-or-later
// Test Action: signalrgb.* brightness and the canvas pause.
export function Action() {
  this.Name = "Brightness";
  this.Description = "Sets the global brightness";
  this.Options = [
    { property: "Level", label: "Level", type: "number", min: 0, max: 100, default: 40 },
    { property: "Pause", label: "Pause", type: "boolean", default: "false" },
  ];
  this.onExecute = function () {
    signalrgb.SetGlobalBrightness(this.Level);
    signalrgb.SetCanvasPauseState(this.Pause);
    console.log(`was ${signalrgb.GetGlobalBrightness()} on ${signalrgb.CurrentEffectName()}`);
  };
}
