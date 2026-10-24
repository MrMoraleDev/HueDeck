<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Windows Dynamic Lighting (WinRT LampArray backend)

HueDeck drives Windows Dynamic Lighting devices (LampArrays) that no plugin claims through
WinRT `Windows.Devices.Lights`. No JavaScript plugin is involved. The platform facts behind
this design are in `docs/research/m6-windows-integrations.md` §1.

## Pieces

| Where | What |
|---|---|
| `crates/huedeck-hal/src/lamparray.rs` | The OS-neutral contract. `LampArrayBackend` provides `enumerate`, `open`, `generation` (hotplug counter), `has_identity` and `container_of`. `LampArrayHandle` provides `info`, `available`, `set_colors` and `release`. Also here: the ownership rule `plugin_owner` and the device key `device_key`. |
| `crates/huedeck-hal/src/mock.rs` | `MockLampArray`, the fake provider the tests use: plug and unplug devices, flip availability and identity, set container ids, and record every colour update. |
| `crates/huedeck-hal-windows/src/lamparray.rs` | `WinLampArray`, the WinRT backend. |
| `crates/huedeck-hal-windows/src/probe.rs` | `container_id(path)`: reads `DEVPKEY_Device_ContainerId` through cfgmgr32. |
| `crates/huedeck-plugin-host/src/lamparray.rs` | The built-in session (the "pseudo-plugin"), plus `SessionDevice::lamparray` and `builtin_entry`. |
| `crates/huedeck-runtime/src/lamparray.rs` | `open()`, which returns the OS backend, and `present()`, which applies the ownership rule. |
| `crates/huedeck-daemon/src/engine.rs` | Scans LampArrays on every rescan, rescans when the backend's generation changes, and shows the device views and warning. |
| `packaging/msix-identity/` | The identity package recipe. |

### WinRT backend

**Discovery.**
- A `DeviceWatcher` on `LampArray.GetDeviceSelector()` follows the device list. The
  selector is the HID top-level collection 0x59/0x01, so a LampArray's id is its HID
  interface path.
- The watcher handlers only record ids and bump the generation counter. They run on system
  threads, so they never block and never panic.
- `enumerate()` runs on the engine thread. It opens new ids with `LampArray.FromIdAsync`
  and caches their description: name, VID:PID, kind, bounding box, minimum update
  interval, lamp positions, and which lamps have a fixed colour.
- Until the watcher has finished its first pass, `enumerate()` uses
  `DeviceInformation.FindAllAsync(selector)` instead.

**Writes.**
- The backend makes one `SetColorsForIndices` call per update, covering only the lamps
  that take colours.
- A write that fails while `IsConnected` is false is reported as `Disconnected`, so the
  device manager restarts or stops the session.

**Release.** WinRT has no "give back" call. The handle drops the `LampArray`, and Windows
hands the device to the next controller or to its autonomous mode.

**Identity.** `GetCurrentPackageFullName` tells whether the process runs with package
identity (`has_identity`).

**Availability.**
- Each handle reads `IsAvailable` and follows it through `AvailabilityChanged`.
- Where `ILampArray2` is missing (older Windows), the handle treats the device as
  available exactly when the process has identity.

### The built-in session

The engine turns each unowned LampArray into a `Present` with a synthetic plugin entry:
- path `builtin:lamparray`;
- name "Windows Dynamic Lighting";
- the device type follows `LampArrayKind` (keyboard, mouse, headphones, microphone,
  speakers, case, otherwise other).

The device key is `lamparray:<vid>:<pid>:<instance>`. The instance is taken from the
interface path, with `&` replaced by `-`.

`session::spawn` sees `SessionDevice::lamparray` and runs `lamparray::run` instead of a
plugin. The `DeviceManager` and the API treat it like any other session.

**Layout.** The grid has 32 cells along the longer side of the bounding box. Each lamp sits
in the cell under its position, which is the same layout the `hid-lamparray` plugin uses. A
device therefore keeps its canvas footprint whichever route drives it. LED names are
`Lamp n`.

**Each frame:**
- The session samples the canvas at the lamp cells through the same `DeviceContext` a
  plugin's `device.color(x, y)` reads. Placement, the three brightness sliders, forced
  colours, OpenRGB SDK direct LEDs and the low-battery alert pulse all apply.
- It writes only when Windows gives HueDeck the device, and no sooner than the device's
  `MinUpdateInterval` after the last write.
- Unchanged colours are resent only once a second.
- When control comes back, the next frame is sent in full.

**While unavailable,** the device view shows `state: "paused"` and the session logs why.

**On stop,** the session releases the device. In dry-run mode it never writes.

## Ownership: a plugin always wins

The ownership rule lives in `huedeck_hal::lamparray::plugin_owner` and is applied by
`huedeck_runtime::lamparray::present`. A LampArray is left alone when a plugin-driven HID
device that is present has any of the following, checked in order. "Plugin-driven" means
the catalog matched it, whether or not the user has disabled it.

1. **The same interface path,** compared case-insensitively. The generic `hid-lamparray`
   plugin claims every 0x59/0x01 collection. With it installed, every HID LampArray that
   hidapi can enumerate is the plugin's. A vendor plugin whose device includes the LampArray
   collection wins the same way.
2. **The same container id** (`DEVPKEY_Device_ContainerId`). This covers another interface
   of the same physical device, or another product id of it, such as the wired versus
   dongle PIDs of a mouse. Container ids are compared only when the LampArray has one. The
   null and all-ones GUIDs count as none.
3. **The same VID:PID.** Plugins claim devices by VID:PID, so HueDeck never drives one
   product through two routes when the first two rules cannot decide.

Other rules:
- `[devices] ignore` filters match LampArray keys too, for example `lamparray:1532`.
- The engine logs each LampArray it leaves to a plugin once:
  `… left to the plugin of <key> (Path|Container|VidPid)`.

**Consequence.** With the first-party plugins installed, the WinRT backend drives only
LampArrays that hidapi does not show as a HID 0x59 collection a plugin claims. Examples: a
LampArray whose collection Windows holds exclusively, or a user who removed or ignored the
`hid-lamparray` plugin. This is intended. The HID route needs no identity package and no
Settings changes.

## What Windows requires

Windows gives a LampArray to the foreground app or to the ambient controller listed first
under Background light control. The daemon is never in the foreground, so it can only be an
ambient controller. That requires:

- package identity, from the sparse MSIX `packaging/msix-identity`, whose `<msix>` element
  sits in `huedeckd.exe`'s side-by-side manifest;
- the `com.microsoft.windows.lighting` app extension in that package;
- the user turning on *Use Dynamic Lighting on my devices* and putting HueDeck first under
  *Background light control*.

When unowned LampArrays exist and the process has no identity, the dashboard shows a warning
that points at the recipe. HueDeck never writes `HKCU\Software\Microsoft\Lighting`.

## Tests

| Test | What it covers |
|---|---|
| `huedeck-hal` `lamparray::tests` | Ownership rule order, device keys, container normalisation, kinds |
| `huedeck-hal` `mock::tests::mock_lamparray_records_and_follows_plugs` | The fake provider |
| `huedeck-hal-windows` `lamparray::tests::fixed_colour_lamps_are_left_out` | Index and colour arrays |
| `huedeck-hal-windows` `lamparray_enumerate_this_machine` | Ignored: read-only listing of this machine's LampArrays |
| `huedeck-plugin-host` `lamparray::tests` | Grid and synthetic entry |
| `huedeck-plugin-host` `tests/lamparray_builtin.rs` | Sampling, brightness, no resend of unchanged colours, pause and full resend, dry-run, unplug shown as disconnected, device manager |
| `huedeck-runtime` `lamparray::tests` | Path, container and VID:PID ownership; `[devices] ignore` |
| `huedeck-daemon` `tests/lamparray.rs` | The engine: owned versus driven, `bus`, paused state, identity warning, hotplug in and out, release on shutdown |

## Not verified on hardware

These items need the user's machine. Dynamic Lighting is off there
(`HKCU\Software\Microsoft\Lighting\AmbientLightingEnabled=0`), and HueDeck does not change
that setting.

- **The WinRT code path has never run against a device.** This covers `FromIdAsync`, the
  lamp descriptions, `SetColorsForIndices`, `AvailabilityChanged`, and `container_id` on a
  LampArray path. Only the build and the pure helpers are tested.
- **The ownership rule is unverified on the DeathAdder V2 Pro.** It is the machine's only
  LampArray (PID 007C, MI_03), and the `hid-lamparray` plugin already drives it, so rule 1
  should leave it to the plugin. With `huedeckd` running there, the log should show
  `lamparray:1532:007c:…: Dynamic Lighting device left to the plugin of 1532:007c:… (Path)`.
- **The identity package has never been registered.** `build.ps1` packs it (MakeAppx
  accepts the manifest), but registration, signing and appearance in Background light
  control are all open. The runtime checks in research §1.6 remain to be done.
