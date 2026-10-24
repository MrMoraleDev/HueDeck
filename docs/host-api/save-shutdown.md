<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Save to devices, and what devices show when HueDeck closes

Two HueDeck extensions of the plugin contract. SignalRGB has neither; SignalRGB-compatible
plugins keep working unchanged (they ignore the extra `Shutdown()` argument and export none of
the new functions). Host code: `crates/huedeck-plugin-host/src/save.rs` (types, rate limit,
fallback), `session.rs` (the calls), `manager.rs` (`DeviceManager::save`), `smbus.rs` (the save
window); daemon: `engine.rs` (`Request::SaveToDevices`, settings); dashboard:
`ui/src/components/SaveDialog.svelte`, Settings and Devices.

## Save to devices

Many controllers keep a lighting configuration in their own flash or EEPROM and show it at
power-on, before (or without) any software. "Save to devices" writes the current lighting there.
Canvas effects cannot be stored: a device gets either the colours of the current frame (as it can
hold them, usually one colour per zone) or one of its built-in effects.

### Plugin side

```js
// What the plugin can store (static: the frame's colours; effects: built-in effects).
export function SaveCapabilities() { return { static: true, effects: ["spectrum", "breathing"] }; }

export function SaveToDevice(request) {
  // request = { action: "preview" | "save", effect: null | "spectrum" | ..., colors: [[r, g, b], ...] }
  // returns  { effect, zones: [{ name, color: "#rrggbb" }], note }  or  { unsupported: "why" }
  // throws on a failed write (the host reports it).
}
```

* `effects` names: `spectrum` (colour cycle), `breathing`, `wave`, `rainbow`. Others are dropped
  with a warning when the plugin is read.
* `request.colors`: the device's LEDs on the frame it showed last, in `LedPositions()` order
  (every grid cell, row by row, when there are none). For subdevices and channels the plugin samples
  `device.subdeviceColor()` / `channel.getColors()` itself: the frame does not change during the call.
* `action: "preview"` must not write. The host enforces it anyway: a preview runs with every HID and
  SMBus write recorded and never sent (dry run). The result fills the dashboard's preview.
* A device the plugin cannot save (an unsupported board variant, a protocol without a storage flag)
  answers `{ unsupported: "why" }` at preview and save alike.
* After a save the plugin puts the device back in its streaming mode, so the live effect continues;
  what the device now holds should become what `Shutdown(false, "restore")` gives back.
* A first-party manifest repeats `SaveCapabilities()` as `save` and `ShutdownModes()` as
  `shutdownModes` (`schemas/plugin-manifest-v2.schema.json`); `plugins/_testkit/manifests.test.mjs`
  checks they match and that a plugin with `save` exports `SaveToDevice`.

### What the host guarantees

* **Only on request.** Nothing calls `SaveToDevice` except `POST /v1/save` (`Request::SaveToDevices`),
  which is refused (400) without `"confirm": true` and without named devices. The dashboard sends it
  only after its confirmation dialog (`window.confirm`). `POST /v1/save/preview` never writes.
* **Rate limit.** At most one save per device per 60 s (`save::SAVE_INTERVAL`), counted when the
  save is handed to the session, whatever its outcome; kept in memory by the device manager (a
  restart of the daemon clears it). Previews are not limited. The snapshot shows the wait
  (`DeviceView.save.waitSecs`).
* **Between frames.** The session runs the call between two `Render()` calls, under its own budget
  (10 s), never during `Initialize()` or `Shutdown()`.
* **SMBus: flash registers locked outside the save.** The registers of `SmbusDeniedRegisters()`
  (MSI GPU: `0x3F`, `MSI_GPU_REG_SAVE`) are refused on every write, except while the session runs
  the `SaveToDevice({action: "save"})` the user asked for (`GuardedBus::set_save_window`); the
  window closes when the call returns, throws or panics. Every other bus rule (address, SPD range)
  still applies inside it. Tests: `smbus::tests::denied_registers_open_only_inside_the_save_window`,
  `tests/session_save.rs`.
* **HID: plugin discipline, tested.** HID reports carry the save flag in their payload, which the host
  cannot tell from colour data. First-party plugins therefore keep the flag in exactly one code path
  (`SaveToDevice` with `action: "save"`), and their tests drive every other path (frames, previews,
  every exit mode, suspend) and assert the flag never appears: `msi-mystic-light-7c91` and
  `msi-mystic-light` (save byte of report 0x52), `razer` (storage byte 0x01).
* **Dry run.** Under `--dry-run` a save runs like a preview and is reported as such.
* **Audit log.** Each save attempt (stored, failed or refused by the rate limit) adds one line to
  `save-audit.log` in HueDeck's data directory and to the event log, e.g.
  `2026-10-23T10:00:00Z save smbus:10de:2504:1462:3903@pci01:68 (MSI GPU) static colours [GPU=#123456]: stored`.

### API

`POST /v1/save/preview` `{devices?: {<key>: {effect: null | "<name>"}}}` (every device when empty)
and `POST /v1/save` `{confirm: true, devices: {...}}` both answer
`{preview, devices: [{key, name, ok, dryRun, result, error}]}`. `DeviceView.save` is
`{static, effects, waitSecs}` or null.

### Plugins

| Plugin | Static | Effects | How | Notes |
|---|---|---|---|---|
| `msi-gpu` | one colour | spectrum, breathing | OpenRGB `DeviceUpdateMode()`, `DeviceUpdateLEDs()`, `MSIGPUController::Save()` (`0x3F ← 1`) | docs/hardware/msi-gpu.md |
| `msi-mystic-light-7c91` | one colour per zone | breathing | OpenRGB `MSIMotherboard185Controller::Update(true)` | docs/hardware/msi-mystic-light-185.md |
| `msi-mystic-light`, `-common` | one colour per zone | breathing | `Update(true)` (185), `MSIMotherboard162Controller::Update(true)` (162); 112-byte boards refused | docs/hardware/msi-mystic-light.md |
| `razer` | one colour per device | spectrum | extended-matrix effect + brightness reports with storage `0x01` (OpenRazer VARSTORE); standard matrix, ARGB controller, Kraken V4 refused | docs/hardware/razer.md |
| `nzxt-hue2` | — | — | no documented persistent store (OpenRGB marks Hue 2 saving "automatic": nothing to send; Kraken X3 has none) | |
| `crucial-ballistix` | — | — | no save register (OpenRGB `@save :x:`) | |
| Sony, SteelSeries Arctis and the others | — | — | OpenRGB `@save :x:` or no known store | |

Everything above is untested on hardware.

## When HueDeck closes

`Shutdown(SystemSuspending, mode)` gets a second argument (HueDeck extension):

| `mode` | Meaning |
|---|---|
| `"restore"` (default; `undefined` from SignalRGB) | What plugins always did: the device's own lighting (its built-in effect, or the state read at start, or whatever the plugin did before). |
| `"keep"` | The last colours HueDeck showed stay on the device. Volatile: never stored (no plugin may use its save path for it); gone after a power cycle. |
| `"off"` | Lights off (black). Volatile as well. |

The mode is the dashboard's Settings → "When HueDeck closes" (`settings.shutdownMode`), with an
optional per-device override in the device's settings (`PATCH /v1/devices/{key}`
`{shutdownMode: "restore" | "keep" | "off" | "default"}`). Changes reach running sessions at once
(`Control::ShutdownMode`). `SystemSuspending = true` (suspend) keeps its SignalRGB meaning.

**Plugins that implement a mode** list it in `ShutdownModes()` (`["restore", "keep", "off"]`) and get
`Shutdown(false, mode)` as is.

**Fallback, SignalRGB-compatible.** For a mode the plugin does not list, the host uses what
SignalRGB plugins already honour: when the plugin has a `shutdownColor` parameter, it sets
`shutdownColor` to the last colour (the average of the device's LEDs on the last frame) for
`keep`, or `#000000` for `off`, and `shutdownMode = "SignalRGB"` when the plugin has that
SignalRGB parameter (whose other value, `Hardware`, would skip the colour); then calls
`Shutdown(false, mode)`. A plugin with neither does what it always does, and the session report
says so; the dashboard offers such a device only "its own lighting".

| Plugin | restore | keep | off |
|---|---|---|---|
| `msi-gpu` | startup registers back (or as last saved) | static, last colour | static black |
| `msi-mystic-light-7c91`, `msi-mystic-light`, `-common` | startup 0x52 image back (or as last saved) | Static mode, last colour per zone, save byte 0 | Static black, save byte 0 |
| `razer` | spectrum cycle / hardware mode (recorded DeathAdder V2 Pro: black frame, hardware mode) | last frame, left in custom-frame mode | black frame |
| `crucial-ballistix` | `shutdownColor` (default black) | through `shutdownColor` | through `shutdownColor` |
| `nzxt-hue2`, `sony-gamepad`, `steelseries-*`, `razer-hanbo`, `razer-kraken`, `hid-lamparray`, `evision-keyboard` | their usual exit (black, or the device's hardware effect / autonomous mode) | not supported: as restore | not supported: as restore |
