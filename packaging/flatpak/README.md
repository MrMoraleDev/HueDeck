<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/flatpak

| File | Purpose |
|---|---|
| `io.github.MrMoraleDev.Lightloom.yml` | flatpak-builder manifest (Freedesktop 25.08 runtime, rust-stable and node24 SDK extensions) |
| `io.github.MrMoraleDev.Lightloom.metainfo.xml` | AppStream metadata (Flathub app id, screenshot, OARS rating, releases), brand colours from `docs/brand/README.md`; also installed by the .deb and .rpm |
| `generate-sources.sh` | writes `cargo-sources.json` and `node-sources.json` (git-ignored) from the lock files, with a pinned flatpak-builder-tools in a throwaway virtualenv |

The desktop entry and icon are the AppImage ones (`../appimage/lightloom.desktop`,
`../appimage/lightloom.svg`), renamed to the app id at install time. The launcher is
`../appimage/AppRun`, installed as `/app/bin/lightloom-launch`.

## Build

```
packaging/flatpak/generate-sources.sh
flatpak install --user flathub org.freedesktop.Sdk//25.08 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08 org.freedesktop.Sdk.Extension.node24//25.08
flatpak-builder --user --install --force-clean build-flatpak packaging/flatpak/io.github.MrMoraleDev.Lightloom.yml
flatpak run io.github.MrMoraleDev.Lightloom --setup-help
flatpak run io.github.MrMoraleDev.Lightloom
```

flatpak-builder runs the build commands without network access: every crate comes from
`cargo-sources.json` (vendored into `cargo/vendor`, with the source replacement in
`$CARGO_HOME/config`), every npm package from `node-sources.json` (an npm cache for
`npm ci --offline`), the rest from the working tree. Regenerate both lists whenever `Cargo.lock`
or `ui/package-lock.json` changes.

CI: `.github/workflows/flatpak.yml` (weekly and on manual dispatch) generates the lists on an
Ubuntu runner, builds the manifest with `flatpak/flatpak-github-actions/flatpak-builder` in
the `ghcr.io/flathub-infra/flatpak-github-actions:freedesktop-25.08` container, checks the
installed tree, runs Flathub's linter (`flatpak-builder-lint` appstream, manifest, builddir,
repo) and leaves `lightloom.flatpak` on the run (artifact `lightloom-flatpak`). It is not
attached to releases.

## Caveats

- **udev rules are a host install.** `--device=all` makes `/dev/hidraw*`, `/dev/bus/usb`,
  `/dev/tty*` and `/dev/i2c-*` visible, but the kernel still checks the host's permissions
  on each node. Without `70-lightloom.rules` (and, for RAM and GPU lighting, the `i2c` group
  and `i2c-dev`) every open fails with "permission denied". `--setup-help` prints the
  commands; they run on the host with sudo, never inside the sandbox.
- **Group membership:** the sandbox keeps the user's supplementary groups, so `i2c`
  works after the user logs in again.
- **Hotplug:** the daemon reads udev's netlink broadcasts (group 2,
  `crates/lightloom-hal-linux/src/uevent.rs`) itself and needs no libudev or `/run/udev`.
  Netlink sockets belong to the network namespace, so this works only with
  `--share=network`. Untested; if a plugged-in device does not appear, use Settings,
  "Look for devices again" in the dashboard.
- **`--share=network`** is also required because the dashboard is served on 127.0.0.1
  and the browser runs outside the sandbox.
- **Renderer:** the CEF effect renderer is in the tarball and the AppImage but not in the
  Flatpak yet. cef-dll-sys downloads the CEF distribution in its build script, which the
  offline build forbids: the manifest needs the CEF minimal archive as a pinned `archive`
  source, laid out the way cef-dll-sys expects (Release and Resources merged, plus the
  `archive.json` it checks) and pointed to by `CEF_PATH`. Running Chromium inside the
  sandbox then needs zypak (`ZYPAK_CEF_LIBRARY_PATH`), as in `org.electronjs.Electron2.BaseApp`.
  Until then the Flatpak plays the native effects only.
- **Flathub**: `packaging/flathub/prepare.sh --tag vX.Y.Z` writes the Flathub manifest (this
  one, with the `dir` source between the `flathub-source` markers replaced by a `git` source
  pinned to the tag and its commit) and the source lists for that commit. Submission steps:
  `packaging/flathub/README.md`.
- Data and config live in `~/.var/app/io.github.MrMoraleDev.Lightloom/{data,config}/lightloom`, not in
  `~/.local/share/lightloom`: a Flatpak and a tarball install do not share settings.
