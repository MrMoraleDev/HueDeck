<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/appimage

| File | Purpose |
|---|---|
| `build-appimage.sh` | builds `Lightloom-<version>-x86_64.AppImage` with linuxdeploy from a staged build |
| `AppRun` | launcher: opens the dashboard (starting `lightloomd` when none runs), `--daemon`, `--cli`, `--udev-rules`, `--setup-help`, `--systemd-unit` |
| `lightloom.desktop` | desktop entry (also used, renamed, by the Flatpak) |
| `lightloom.svg` | icon: `docs/brand/lightloom-mark.svg` on a square canvas (`viewBox="0 -38 412 412"`); regenerate it when the mark changes |

## Build

```
cargo build --release -p lightloom-daemon -p lightloom-cli
(cd ui && npm ci --ignore-scripts && npm run build)
tools/package/stage-linux.sh --release target/release --ui ui/dist --out dist/lightloom
packaging/appimage/build-appimage.sh --stage dist/lightloom --out dist
```

The AppDir keeps the staged tree under `usr/lib/lightloom/` (lightloomd looks for `plugins/`,
`effects/` and `ui/` next to its executable) and symlinks the programs into `usr/bin/`.
linuxdeploy runs with `--deploy-deps-only usr/lib/lightloom`, so the libraries of the
renderer and libcef.so are bundled too once WP-L5 adds them.

## udev rules

An AppImage runs as the user and cannot install udev rules, a group or a kernel module.
`Lightloom-*.AppImage --setup-help` prints the one-time commands, and
`--udev-rules | sudo tee /etc/udev/rules.d/70-lightloom.rules` installs the bundled rules
(the same approach as OpenRGB's AppImage). With imported plugins, generate rules that
cover them: `Lightloom-*.AppImage --cli udev generate --plugins <dir> ...`.

## Notes

- The AppImage runtime needs FUSE 2 (`libfuse2` / `fuse2`); without it run with
  `APPIMAGE_EXTRACT_AND_RUN=1` or `--appimage-extract`.
- Chromium's sandbox (renderer, WP-L5) cannot use the SUID helper from an AppImage and
  needs unprivileged user namespaces; on Ubuntu 24.04 and later these are restricted by
  AppArmor. Decided with WP-L5.
