<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/flatpak

| File | Purpose |
|---|---|
| `org.lightloom.Lightloom.yml` | flatpak-builder manifest (Freedesktop 24.08 runtime, rust-stable and node20 SDK extensions) |
| `org.lightloom.Lightloom.metainfo.xml` | AppStream metadata, brand colours from `docs/brand/README.md` |
| `generate-sources.sh` | writes `cargo-sources.json` and `node-sources.json` (git-ignored) from the lock files |

The desktop entry and icon are the AppImage ones (`../appimage/lightloom.desktop`,
`../appimage/lightloom.svg`), renamed to the app id at install time. The launcher is
`../appimage/AppRun`, installed as `/app/bin/lightloom-launch`.

## Build

```
packaging/flatpak/generate-sources.sh
flatpak install --user flathub org.freedesktop.Sdk//24.08 \
  org.freedesktop.Sdk.Extension.rust-stable//24.08 org.freedesktop.Sdk.Extension.node20//24.08
flatpak-builder --user --install --force-clean build-flatpak packaging/flatpak/org.lightloom.Lightloom.yml
flatpak run org.lightloom.Lightloom --setup-help
flatpak run org.lightloom.Lightloom
```

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
- **Renderer:** the CEF effect renderer is not in the Flatpak until it builds on Linux
  (WP-L5). The Chromium sandbox then needs zypak (`ZYPAK_CEF_LIBRARY_PATH`), as in
  `org.electronjs.Electron2.BaseApp`.
- **Flathub** needs a pinned `git` source (commented in the manifest) instead of `dir`,
  and the generated source lists committed in the Flathub repository.
- Data and config live in `~/.var/app/org.lightloom.Lightloom/{data,config}/lightloom`, not in
  `~/.local/share/lightloom`: a Flatpak and a tarball install do not share settings.
