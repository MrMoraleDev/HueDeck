<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# packaging/appimage

| File | Purpose |
|---|---|
| `build-appimage.sh` | builds `Lightloom-<version>-x86_64.AppImage` with linuxdeploy (pinned release) from a staged build |
| `AppRun` | launcher: opens the dashboard (starting `lightloomd` when none runs), `--daemon`, `--cli`, `--udev-rules`, `--setup-help`, `--systemd-unit` |
| `lightloom.desktop` | desktop entry (also used, renamed, by the Flatpak) |
| `lightloom.svg` | icon: `docs/brand/lightloom-mark.svg` on a square canvas (`viewBox="0 -38 412 412"`); regenerate it when the mark changes |

## Build

```
cargo build --release -p lightloom-daemon -p lightloom-cli
cargo build --release -p lightloom-renderer --features cef
(cd ui && npm ci --ignore-scripts && npm run build)
tools/package/stage-linux.sh --require-renderer --release target/release --ui ui/dist --out dist/lightloom
packaging/appimage/build-appimage.sh --stage dist/lightloom --out dist
```

The AppDir keeps the staged tree under `usr/lib/lightloom/` (lightloomd looks for `plugins/`,
`effects/`, `faces/`, `ui/` and `lightloom-renderer` next to its executable) and symlinks the
programs into `usr/bin/`. The script runs linuxdeploy in two steps:

1. Deploy only (no `--output`), with `--deploy-deps-only usr/lib/lightloom`, while the CEF
   effect renderer and its runtime (`lightloom-renderer`, `*.so`, `*.pak`, `*.bin`, `*.dat`,
   `vk_swiftshader_icd.json`, `locales/`) are moved out of the AppDir. linuxdeploy patches the
   rpath of every ELF file it finds under `usr/lib` and would replace the renderer's `$ORIGIN`
   rpath, run patchelf on libcef.so, and bundle Chromium's system libraries (NSS, which loads
   its own modules at run time, and Mesa's libgbm), which have to come from the host.
2. The CEF files go back next to `lightloomd`; `ldd` must resolve libcef.so from there. Then
   linuxdeploy's own AppImage plugin (`usr/bin/linuxdeploy-plugin-appimage` inside the
   extracted linuxdeploy AppImage) writes the image from the finished AppDir.

Like the tarball, the AppImage expects the libraries every Chromium browser needs on the host
(NSS, libgbm, libdrm, ALSA, CUPS, X11/xkbcommon, Pango/Cairo, ATK). Without them only the
renderer fails, and the daemon plays the native effects.

## udev rules

An AppImage runs as the user and cannot install udev rules, a group or a kernel module.
`Lightloom-*.AppImage --setup-help` prints the one-time commands, and
`--udev-rules | sudo tee /etc/udev/rules.d/70-lightloom.rules` installs the bundled rules
(the same approach as OpenRGB's AppImage). With imported plugins, generate rules that
cover them: `Lightloom-*.AppImage --cli udev generate --plugins <dir> ...`.

## Notes

- The AppImage runtime needs FUSE 2 (`libfuse2` / `fuse2`); without it run with
  `APPIMAGE_EXTRACT_AND_RUN=1` or `--appimage-extract`.
- The Chromium sandbox is off (`crates/lightloom-renderer/src/cef_host.rs`), so
  `chrome-sandbox` is not shipped: an AppImage cannot carry a root-owned SUID helper, and the
  user-namespace sandbox is restricted by AppArmor on Ubuntu 24.04 and later.
