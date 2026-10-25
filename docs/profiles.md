<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Profiles

A profile is a named snapshot of the whole lighting setup, as OpenRGB has them: one click (or one
`lightloom://` link, a macro, the tray) brings everything back. Code: `crates/lightloom-daemon/src/engine/profiles.rs`,
`store.rs` (table `profiles`, migration 5); model `lightloom_api::model::Profile`; dashboard
`ui/src/components/Profiles.svelte` and the header's Profile select; tray: the Profiles submenu.

## What a profile holds

| Part | What | Applied as |
|---|---|---|
| `lighting` | the effect id, its control values, the global brightness, the canvas pause | values stored on the effect, then the effect shown (the current one stays when it is not in the library) |
| `layout` | the current layout: every placement (position, scale, rotation, flips, per-device brightness and enable, lighting mode) and the layout brightness | saved under its name (replacing a layout of that name) and switched to |
| `devices` | one open object per device Lightloom has seen, by device key | per device, below |

Per device (`ProfileDevice`): `enabled`, `parameters` (plugin parameters), `channels` (component arrays
by channel), `shutdownMode` (absent: the setting's), `hardwareMode` (docs/host-api/hardware-modes.md;
absent: follow the effect) and `customLeds` (per-LED custom colours, mode and zones by endpoint uid, as
in the state export; endpoints left out follow the effect; docs/custom-leds.md).

The per-device object is **open**: a key this build does not know is kept as it is through save,
export, import, rename and duplicate, and ignored when applied. A later per-device setting therefore
goes into the same document with no schema change; `format` stays `lightloom-profile`, `version` 1.

Applying a profile stops a playlist. Devices the profile names but Lightloom has never seen are skipped
and listed in the answer's `notes`; devices Lightloom knows but the profile does not name are left alone.
Changed channel components restart the device's session, as editing them does.

## Managing them

* Save the current setup (new name, or "Update" an existing profile), apply, rename, duplicate, delete.
* Export one profile as a `.lightloom-profile.json` file (`GET /v1/profiles/{name}`) and import it on
  another machine (`POST /v1/profiles`; a taken name becomes `Name (2)`). Device keys are the machine's own
  (`vid:pid:serial`, SMBus and network keys), so on another machine only matching devices get settings.
* The whole-state export (`/v1/export`) carries every profile.
* **Default profile** (`PATCH /v1/profiles {"default": name | null}`, the "At start" radio): applied at
  start, before the canvas and the devices start; without one Lightloom starts with the setup as it was
  left. Deleting the default profile clears the setting; renaming it follows.
* The header shows the profile applied or saved last (`Lighting.profile`); any later change keeps that
  name until another profile is applied or saved.

## API

`GET /v1/profiles` (list), `POST /v1/profiles` (import), `PATCH /v1/profiles` (default),
`GET|PUT|DELETE /v1/profiles/{name}` (document, save the current setup, delete),
`POST /v1/profiles/{name}/apply`, `/rename {name}`, `/duplicate {name}` (docs/api/openapi.yaml).

## Actions and macros

* `lightloom://profile/apply/<name>` (docs/integrations/actions.md); a macro calls it with
  `system.action("profile/apply/<name>")`.
* SignalRGB macros switch "layouts" (`Activate Layout`: `system.action("layout/apply/<name>")`), and a
  SignalRGB layout holds more than placements. So `layout/apply/<name>` applies the profile `<name>` when
  there is no layout of that name but such a profile.
* The macro option type `profilename` (a profile name, entered as text) exists for Lightloom-written macros.
