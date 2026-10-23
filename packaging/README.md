<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/

Distribution artifacts for Lightloom. Release builds are produced by
`.github/workflows/release.yml` (cargo-dist planned; cargo-auditable + CycloneDX SBOM
today). Nothing here is signed yet; Authenticode via SignPath OSS is the planned step
for Windows (see the SIGNING block in the release workflow).

`wix/` (Windows MSI), `winget/` (manifest templates), `systemd/` and `udev/` are real;
the Linux bundles (M7) are placeholders with the file names already decided.
`wix/lightloom.ico` is the app icon (rendered from `docs/brand/lightloom-icon.svg`), also
embedded in every Windows executable by `crates/lightloom-winres`.

| Directory   | Target               | Planned files                                                                 | Milestone |
|-------------|----------------------|-------------------------------------------------------------------------------|-----------|
| `wix/`      | Windows MSI (WiX v5)  | `lightloom.wxs` (package), `LightloomUI.wxs` (dialogs), `Lightloom.en-us.wxl` / `Lightloom.it-it.wxl` (strings), `License.rtf`, `lightloom.ico`, `build.ps1` (present) | M4 |
| `winget/`   | winget manifest       | `template/Lightloom.Lightloom{,.installer,.locale.en-US}.yaml` and `render.ps1`, which writes `manifests/h/Lightloom/Lightloom/<version>/` from a released MSI (present) | M4 |
| `systemd/`  | Linux user service    | `lightloomd.service` (present)                                                  | M7 |
| `udev/`     | Linux device access   | `70-lightloom.rules.template` (present); generated `70-lightloom.rules` via `tools/gen-udev` | M7 |
| `appimage/` | AppImage              | `AppRun`, `lightloom.desktop`, `lightloom.svg`, `build-appimage.sh` (linuxdeploy)   | M7 |
| `flatpak/`  | Flatpak               | `org.lightloom.Lightloom.yml` manifest, `org.lightloom.Lightloom.metainfo.xml`, `cargo-sources.json` (flatpak-cargo-generator) | M7 |

## Windows (MSI, winget)

### What gets built

- `tools/package/stage-windows.ps1` stages one directory: `lightloomd.exe`, `lightloomw.exe`
  (starts the daemon without a window), `lightloom.exe`, `lightloom-renderer.exe` and its CEF
  runtime, `lightloom-setup.exe` and `lightloom-iobroker.exe` (RAM lighting), `pawnio-modules/`
  (PawnIO.Modules 0.2.11: `SmbusPIIX4.bin`, `Echo.bin`, `COPYING` and their sources, every
  file SHA-256 pinned, fetched by `tools/package/fetch-pawnio-modules.ps1`), `ui/`,
  `plugins/`, `effects/`, the licence texts, `privacy.md` and
  `THIRD-PARTY/CEF-Chromium-CREDITS.html`. About 400 MB, most of it Chromium.
- The **portable zip** is that directory: unzip anywhere and run `lightloomw.exe --open`
  (or `lightloomd.exe --open` to see the log in a console).
- The **MSI** (`packaging/wix/build.ps1 -Stage <dir>`, WiX Toolset 5.0.2 as a .NET tool)
  packs the same directory. `release.yml` builds both for a tag; `ci.yml` checks that the
  WiX sources build on a stand-in stage.

### Installer design

The user's decisions: it works out of the box; Lightloom itself installs per user without
administrator rights; RAM lighting (the PawnIO driver) is the user's choice, offered and
checked by default.

- **Per-user MSI** (`Scope="perUser"`), installed to `%LOCALAPPDATA%\Programs\Lightloom`; no
  UAC prompt for the install itself. Settings stay in `%LOCALAPPDATA%\Lightloom` and
  `%APPDATA%\Lightloom` (shared with the portable build).
- **Options page**: *RAM and motherboard lighting (installs the PawnIO driver)*, with a short
  explanation, checked by default; *Start Lightloom when I sign in* (an `HKCU\...\Run` value
  starting `lightloomw.exe`, which runs the daemon with a hidden console and logs to
  `%LOCALAPPDATA%\Lightloom\logs`, like the portable `start-lightloom.ps1` did), checked by
  default. A Start-menu shortcut *Lightloom* (`lightloomw.exe --open`: starts the daemon if needed
  and opens the dashboard) is always created. The Finish page offers *Start Lightloom now*.
  English and Italian strings (`-Culture it-IT`).
- **RAM lighting** is the only privileged part. When the option is checked, pressing Finish
  starts `lightloom-setup.exe --pawnio-install --smbus --elevate --pause`, which asks Windows
  for permission once (UAC) and, in its own window: reads PawnIO's version from its uninstall
  entry and, when it is missing or older than 2.2.0, downloads the official
  `PawnIO_setup.exe` 2.2.0 from `github.com/namazso/PawnIO.Setup` into
  `%ProgramFiles%\Lightloom\downloads` (administrators-only), checks its size, SHA-256
  (`1f519a22…3032`, pinned in `lightloom-iobroker/src/pins.rs`) and Authenticode signer
  (`namazso.eu`), runs it with `-install -silent` and deletes it; then copies the bundled
  pinned modules and the broker to `%ProgramFiles%\Lightloom` and registers the
  `LightloomIoBroker` service. A declined prompt, no network or a failed check changes nothing:
  the install itself has already succeeded, RAM lighting stays off, and the dashboard offers
  it again (Settings → PawnIO / RAM lighting → Install). A running daemon notices the service
  when it starts and opens its buses.
- **Why not a per-machine MSI or a Burn bundle?** A per-machine MSI would need administrator
  rights for Lightloom itself, which the user did not want. A WiX Burn bundle that chains a
  per-user MSI with a per-machine package registers itself per machine and asks for
  elevation for the whole chain, and its option checkbox would need a custom bootstrapper
  theme. Running the one elevated step from the per-user MSI through the same helper the
  dashboard uses keeps a single code path (Rust, tested) for both, and the elevation visible
  and separate.
- **Silent installs** (`msiexec /qn`, winget) install Lightloom only: they never raise a UAC
  prompt. RAM lighting is then one click in Settings.
- **Upgrades** (major upgrade, same `UpgradeCode`) first ask a running daemon to stop
  (`lightloomw.exe --stop`: `POST /v1/shutdown`, so plugins restore their devices). With the
  option checked, Finish runs the helper again, which updates the broker (stop, replace,
  register) and keeps PawnIO when it is current.
- **Uninstall** stops the daemon, then, when the `LightloomIoBroker` service exists, runs
  `lightloom-setup.exe --smbus --uninstall --elevate` (one UAC prompt) to remove the service and
  its files under `%ProgramFiles%\Lightloom`. **PawnIO stays installed**, because other programs
  (fan and monitoring tools) use it; the dashboard and `docs/privacy.md` say so. If that prompt
  is declined, `sc.exe delete LightloomIoBroker` from an elevated prompt removes the service.
- **Validation**: ICE38/ICE64/ICE91 are suppressed (`build.ps1`): they assume per-machine
  locations and flag every harvested file under the user profile.
- **Signing**: not yet; SignPath (OSS) signs the executables and the MSI in `release.yml`
  once enabled. Until then Windows SmartScreen and the UAC prompt show an unknown publisher.

### Privacy

The PawnIO download is the only network access Lightloom makes on its own; see
[`docs/privacy.md`](../docs/privacy.md).

### winget

`winget/render.ps1 -Msi <signed msi> -Url <release url>` fills the templates (version,
`ProductCode` read from the MSI, SHA-256) into `winget/manifests/h/Lightloom/Lightloom/<version>/`;
validate with `winget validate` and submit to `microsoft/winget-pkgs` (for example
`wingetcreate submit`). The manifests declare `Scope: user`.

## Linux (systemd user unit, udev)

- `systemd/lightloomd.service` is a `systemd --user` unit: copy to
  `~/.config/systemd/user/`, then `systemctl --user enable --now lightloomd`. It expects the
  daemon at `~/.local/bin/lightloomd` (the AppImage/Flatpak builds ship their own unit with
  the correct path).
- `udev/70-lightloom.rules.template` documents the two rule shapes the daemon needs:
  `TAG+="uaccess"` per supported VID:PID (hidraw/USB access for the logged-in seat user)
  and `GROUP="i2c"` on `/dev/i2c-*` for SMBus plugins. The concrete rules file is
  generated by `tools/gen-udev` from the loaded plugin manifests and installed with
  `lightloom udev install` (writes `/etc/udev/rules.d/70-lightloom.rules`, needs sudo once).

## AppImage / Flatpak

- AppImage bundles CEF and the renderer; built with linuxdeploy in CI (M7).
- Flatpak uses the `org.freedesktop.Platform` runtime; device access through the
  `--device=all` permission (hidraw) and the `/dev/i2c-*` rule above. Portals are used for
  screen capture (`engine.zone`) and audio (`engine.audio`).
