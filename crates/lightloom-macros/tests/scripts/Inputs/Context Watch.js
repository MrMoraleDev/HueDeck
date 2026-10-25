// SPDX-License-Identifier: GPL-3.0-or-later
// Test Input: the foreground window (its output is not logged) and sensor updates.
export function Input() {
  this.Name = "Context Watch";
  this.Description = "Reacts to the foreground window and to sensors";
  this.Options = [];
  this.onApplication = function (event) {
    if (event.EventType !== "Foreground App Changed") return;
    console.log(`title ${event.data.title}`);
    if (event.data.exe === "notepad.exe" && event.data.title === "Secret plans") block.trigger();
  };
  this.onSensorUpdate = function (event) {
    const load = event.data.sensors["CPU Load"];
    console.log(`${event.EventHandler}|${event.EventType}|${event.Sender}|${load.value}${load.units}`);
  };
}
