<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Hardware modes (live built-in effects)

A Lightloom extension of the plugin contract. Many controllers have built-in effects of their own
(an MSI graphics card's rainbow, wave or breathing; a motherboard's static zones). A plugin can offer
them as **hardware modes**: the device runs the effect itself, and Lightloom stops sending it frames
until the user picks "Follow effect" again. SignalRGB has nothing like it; SignalRGB-compatible plugins
keep working unchanged (they export neither function).

Host code: `crates/lightloom-plugin-host/src/hwmode.rs` (declarations, checks), `session.rs`
(`Control::HardwareMode`, `SessionOptions::hardware_mode`, no `Render()` while a mode runs); daemon:
`engine/profiles.rs` (kept per device, `PUT /v1/devices/{key}/mode`, profiles); dashboard:
`ui/src/components/HardwareModePicker.svelte` (the "Mode" select of a device card).

## Plugin side

```js
// The modes, read once when the plugin is loaded (call-if-function, like the other metadata).
export function HardwareModes() {
  return [
    { id: "rainbow", name: "Rainbow", colors: 0, speed: true, brightness: true },
    { id: "breathing", name: "Breathing", colors: 1, speed: true, brightness: true },
    { id: "static", name: "Static (per zone)", colors: 2, colorNames: ["Top", "Bottom"], brightness: true },
  ];
}

export function SetHardwareMode(request) {
  // request = { id, colors: [[r, g, b], ...], speed: 0-100 | null, brightness: 0-100 | null }
  //   or null: back to the streaming mode Render() uses ("Follow effect").
  // Live commands only. Throw when the device refuses the mode.
}
```

* `id`: 1-64 characters, unique. `name` defaults to the id. `colors`: 0-8 colour inputs; `colorNames`
  labels them (ignored unless there are exactly `colors` names). `speed` / `brightness`: the mode has
  one. An entry that does not fit is dropped with a warning when the plugin is read; `HardwareModes()`
  without a `SetHardwareMode` export offers nothing.
* The host checks a request before the plugin sees it: a known `id`, at most `colors` colours (missing
  ones are white), `#rrggbb` colours, speed and brightness 0-100 (defaults 50 and 100), `null` for a
  mode without them. The plugin gets colours as `[r, g, b]`.
* `SetHardwareMode(null)` must put the device back in the mode `Render()` streams to; the next frame is
  sent whole. It is only called when the device runs a hardware mode.

## What the host guarantees

* **Never stored in the device.** `SetHardwareMode` is a live command. On SMBus the plugin's
  `SmbusDeniedRegisters()` stay refused during the call (no save window is opened, unlike
  `SaveToDevice`); first-party HID plugins keep their save flag at 0 and their tests assert it. The mode
  is gone after a power cycle; "Save to devices" (docs/host-api/save-shutdown.md) stays the only way to
  store lighting.
* **No frames while a mode runs.** The session applies controls (parameters, brightness, placements, a
  save) but calls no `Render()`; `LiveSnapshot::hardware_mode` says so. After a "Save to devices" (which
  ends in the plugin's streaming mode) the host sets the hardware mode again.
* **Kept by Lightloom.** The choice is saved per device (`hardwareMode.<key>` in the database) and handed
  to every new session of the device right after `Initialize()`: after a restart, a replug, sleep and
  resume. A refused call leaves the device as it was and the choice unchanged.
* **Exit modes.** `Shutdown(false, "keep")` leaves the hardware mode running (it is volatile anyway);
  "restore" and "off" work as always.
* **Profiles** carry each device's hardware mode (docs/profiles.md).

## API

* `DeviceView.hardwareModes`: the plugin's modes (`{id, name, colors, colorNames, speed, brightness}`).
* `DeviceView.hardwareMode`: the choice, `{id, colors, speed, brightness}` or null ("Follow effect").
* `PUT /v1/devices/{key}/mode` with `{id, colors?, speed?, brightness?}`, or `{"id": null}` to follow the
  effect again. `400` for a mode that does not fit, `404` for an unknown device, `409` when the plugin
  refused it. The answer: `{device, hardwareMode, applied}` (`applied: false` while the device is away; it
  gets the mode when it comes back).

## Plugins

| Plugin | Modes | How | Notes |
|---|---|---|---|
| `msi-gpu` | Static, Rainbow, Raindrop, Magic, Patrolling, Streaming, Lightning, Wave, Meteor, Stack, Rhythm, Flowing, Whirling, Twisting, Laminating, Fade in (2 colours), Breathing, Flashing, Double flashing | OpenRGB `RGBController_MSIGPU::DeviceUpdateMode()` (brightness 0-5 × 20, speed `0x04/0x02/0x01`, mode) then `DeviceUpdateLEDs()` (0x26, colour 1; Fade in colours 2 and 3); never 0x3F | OpenRGB's "Direct" is the canvas; its "Off" is left out (OpenRGB writes nothing for it). docs/hardware/msi-gpu.md |
| `msi-mystic-light-7c91` | Static (per zone: onboard, JRGB1, JRGB2, JRAINBOW1, JRAINBOW2) | the board's static configuration (the "keep" exit mode's), brightness level 1-10, save byte 0 | Animated board modes are left out until checked on the bench. docs/hardware/msi-mystic-light-185.md |

Everything above is mock-tested only (`plugins/*/tests`, `crates/lightloom-plugin-host/tests/session_hwmode.rs`,
`crates/lightloom-daemon/tests/profiles.rs`); none has run on the card or the board yet.
