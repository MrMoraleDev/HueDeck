<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Lightloom on Linux

Part 1 is for users: how to install Lightloom and give it access to your devices.
Part 2 is for developers: how the Linux port is put together and what is still missing.

Status (M7, see `docs/status.md`): the hidraw, raw USB, serial, udev hotplug and `/dev/i2c-*`
backends, and the desktop integration (sleep and lock through logind, notifications, the tray
icon), compile and are unit-tested, but none has run on real Linux hardware or a Linux desktop
yet. The CEF effect
renderer and its daemon-side client are ported (WP-L5: built and run headless by CI's
`renderer-cef-linux` job) and ship in the tarball and the AppImage with the CEF runtime next to
them; the release workflow runs the staged renderer once headless, but it has not run on a
Linux desktop yet. The Flatpak does not include the renderer yet and plays the native effects
only.
PipeWire audio and screen capture are not ported yet.

## Part 1: installing

### Which build

| Build | For | Device setup | Updates |
|---|---|---|---|
| Tarball `lightloom-<version>-linux-x86_64.tar.xz` | any x86_64 distribution with glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later) | manual, once (below) | replace the directory |
| AppImage `Lightloom-<version>-x86_64.AppImage` | the same, one file, no install | manual, once (`--setup-help`) | replace the file |
| Flatpak `org.lightloom.Lightloom` | sandboxed install | manual, once, on the host | `flatpak update` |
| From source | developers | manual, once | `git pull` |

Every build runs as your user. No build installs a kernel driver, and none needs root at
run time. Root is needed only once, to install the udev rules that grant your user access
to the devices.

### Tarball

```sh
mkdir -p ~/.local/lib ~/.local/bin
tar -xJf lightloom-<version>-linux-x86_64.tar.xz -C ~/.local/lib
mv ~/.local/lib/lightloom-<version>-linux-x86_64 ~/.local/lib/lightloom
ln -sf ~/.local/lib/lightloom/lightloomd ~/.local/lib/lightloom/lightloom ~/.local/bin/
```

`lightloomd` looks for its plugins, effects and dashboard next to its real executable, so
keep the directory together and use symlinks (not copies) in `~/.local/bin`.

The CEF effect renderer (`lightloom-renderer`, with `libcef.so` and the rest of the Chromium
runtime beside it) is in the same directory. It uses the system libraries every Chromium
browser needs (NSS, libgbm, libdrm, ALSA, CUPS, X11/xkbcommon, Pango/Cairo, ATK); a desktop
with a browser installed has them. On a server without them, install your distribution's
`chromium` dependencies, or the daemon plays the native effects only. The Chromium sandbox is
off (as on Windows until releases are signed), so `chrome-sandbox` is not shipped.

Then do the one-time device setup below, and start the daemon:

```sh
lightloomd --open                 # foreground; opens the dashboard in your browser
```

or start it with your session through systemd:

```sh
mkdir -p ~/.config/systemd/user
cp ~/.local/lib/lightloom/packaging/systemd/lightloomd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now lightloomd
journalctl --user -u lightloomd -f        # logs
lightloom url                             # where the dashboard is
```

### AppImage

```sh
chmod +x Lightloom-<version>-x86_64.AppImage
./Lightloom-<version>-x86_64.AppImage --setup-help    # prints the one-time setup commands
./Lightloom-<version>-x86_64.AppImage                 # starts the daemon, opens the dashboard
```

| Command | Does |
|---|---|
| `Lightloom.AppImage` | opens the dashboard; starts the daemon first when none is running |
| `Lightloom.AppImage --daemon [ARGS]` | runs `lightloomd` in the foreground |
| `Lightloom.AppImage --cli ARGS` | runs the `lightloom` CLI, e.g. `--cli devices` |
| `Lightloom.AppImage --udev-rules` | prints the bundled `70-lightloom.rules` |
| `Lightloom.AppImage --systemd-unit` | prints a systemd user unit that runs this AppImage |

An AppImage cannot install udev rules itself; install them with
`./Lightloom-*.AppImage --udev-rules | sudo tee /etc/udev/rules.d/70-lightloom.rules`, then
follow the rest of the setup below. If you move the AppImage, regenerate the systemd
unit. The AppImage needs FUSE 2 (`libfuse2` on Debian and Ubuntu, `fuse-libs` on Fedora);
without it, set `APPIMAGE_EXTRACT_AND_RUN=1`. The AppImage contains the CEF effect renderer
and takes Chromium's system libraries from the host, as the tarball does.

### Flatpak

```sh
flatpak run org.lightloom.Lightloom --setup-help        # run the printed commands on the host
flatpak run org.lightloom.Lightloom
```

The Flatpak has `--device=all`, but the kernel still checks the host's permissions on each
device node, so the udev rules, the `i2c` group and `i2c-dev` must be set up on the host
exactly as below. Its settings live in `~/.var/app/org.lightloom.Lightloom/`, separate from a
tarball install. More caveats: `packaging/flatpak/README.md`.

### One-time device setup

From the tarball directory (`~/.local/lib/lightloom/packaging`) or a source checkout
(`packaging/`):

```sh
# 1. USB/HID devices (keyboards, mice, controllers, AIOs, LED strips):
sudo install -m 0644 udev/70-lightloom.rules /etc/udev/rules.d/70-lightloom.rules

# 2. RAM, motherboard and graphics-card lighting over SMBus/I2C only:
sudo install -m 0644 sysusers.d/lightloom.conf /etc/sysusers.d/lightloom.conf
sudo systemd-sysusers                       # creates group i2c (kept if it exists)
sudo usermod -aG i2c "$USER"
sudo install -m 0644 modules-load.d/lightloom.conf /etc/modules-load.d/lightloom.conf
sudo modprobe i2c-dev                       # creates /dev/i2c-*, also loaded at every boot

# 3. Apply:
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Log out and back in after joining `i2c`. Distribution packages put the same files in
`/usr/lib/udev/rules.d/`, `/usr/lib/sysusers.d/` and `/usr/lib/modules-load.d/`.

What the rules do:

- **HID and USB devices** get `TAG+="uaccess"`: systemd-logind gives the user at the
  active local seat an ACL on the device's `hidraw` (or `usb_device`, `tty`) node. Only
  the VID:PIDs the plugins claim are tagged, and never their input nodes, so Lightloom cannot
  read your keystrokes. Over SSH, or with lingering and nobody logged in at the seat,
  `uaccess` does not apply.
- **SMBus/I2C adapters** go to group `i2c`, mode 0660, matched by the kernel adapter name:
  AMD PIIX4 ports 0 and 1, Intel i801, NVIDIA i2c adapter 1, and the AMDGPU OEM bus. DDC
  (monitor) buses, touchpads, embedded controllers and the other PIIX4 ports stay
  root-only.

### Raw USB devices

Plugins of type `rawusb` (and the USB side of `hybrid` ones) talk to the device through usbfs,
the `/dev/bus/usb/BBB/DDD` node the rules tag `uaccess`. Which interfaces Lightloom may use:

- an interface no kernel driver is bound to: always;
- an interface a kernel driver holds: only for a `rawusb` plugin and only when it is not HID.
  Lightloom detaches the driver when the plugin first uses the interface and attaches it again
  when the device is closed (Lightloom quits, the plugin stops, the device sleeps);
- a HID interface (`usbhid`): never. Hybrid devices keep all their kernel drivers; their HID
  side goes through hidraw.

When nothing the plugin needs is free, the device list shows "USB interface held by a kernel
driver" with the interface and its driver (`ls -l /sys/bus/usb/devices/*/driver` shows the
bindings). Lightloom never installs, blacklists or unbinds drivers otherwise. "Permission
denied" on `/dev/bus/usb/...` means the udev rules are missing for that VID:PID.

With imported plugins (SignalRGB-format or community ones), generate rules that cover them
too: `lightloom udev generate --plugins <dir> --plugins <other dir> --out 70-lightloom.rules`.

Check the setup:

```sh
lightloom devices                  # lists HID devices; "permission denied" means no rule
getfacl /dev/hidraw* | grep "user:$USER"
ls -l /dev/i2c-*                 # the Lightloom adapters show group i2c
i2cdetect -l                     # adapter names (i2c-tools); never run a bus scan on a GPU bus
```

### Desktop integration

- **Tray icon** (`lightloom-tray`): a StatusNotifierItem with the Windows menu (open the
  dashboard, pause, brightness, profiles, quit; a left click opens the dashboard). KDE Plasma and
  most panels show it; GNOME needs the AppIndicator and KStatusNotifierItem Support extension
  (`gnome-shell-extension-appindicator`). Without a host it prints that and exits. Start at
  sign-in is not in the menu: use the systemd user unit.
- **Notifications** (low battery and the like) go to the desktop's notification server
  (`org.freedesktop.Notifications`); without one they are dropped and logged once.
- **Sleep and lock** come from systemd-logind (`docs/power.md`): the daemon takes a `delay`
  sleep inhibitor (`systemd-inhibit --list` shows it) so the devices are put away before the
  system sleeps, and follows the lock state of your session. Without logind (some containers)
  the daemon runs without them.

### PawnIO is not needed

On Windows, SMBus access goes through PawnIO and the `lightloom-iobroker` service, because
Windows has no user-mode SMBus interface. On Linux the kernel's own drivers (`i2c-piix4`,
`i2c-i801`, `nvidia`, `amdgpu`) own the controllers and `i2c-dev` exposes them as
`/dev/i2c-*`. Lightloom uses those nodes directly. There is no broker, no helper process and
no kernel module to install; `lightloom-setup` and `lightloom-iobroker` are Windows-only.

### Graphics cards

- **NVIDIA, proprietary or open kernel modules:** the driver registers one I2C adapter
  per GPU port, named `NVIDIA i2c adapter <port> at <bus>:<slot>.<fn>`. The RGB
  controller on the boards Lightloom knows (MSI, OpenRGB evidence) is on port 1, and only
  that port gets the `i2c` group. You need `i2c-dev` loaded; nothing else. Some driver
  versions and laptops expose no adapters at all; `i2cdetect -l` shows whether yours does.
  `nouveau` registers its buses under other names, which the rules do not match, so
  NVIDIA lighting works only with the NVIDIA driver. `i2c-nvidia-gpu` is the USB-C
  controller bus, not lighting.
- **AMD (`amdgpu`):** the `AMDGPU DM i2c OEM bus` used for board lighting appears from
  kernel 6.15. Older kernels have no lighting bus for Lightloom to use.

### Motherboard SMBus notes

- If `dmesg` shows an ACPI resource conflict for the SMBus controller, the firmware has
  claimed it and the kernel leaves it alone. Lightloom does not ask you to override that
  (`acpi_enforce_resources=lax` is at your own risk).
- AMD boards: since kernel 6.13, boards with an ASF controller may get an `AMD ASF adapter`
  instead of the PIIX4 aux port. It cannot read, so Lightloom does not use it.

### Coexistence

OpenRGB, liquidctl, openrazer and similar tools can drive the same devices. hidraw has no
exclusive access, so two programs writing at once make the lights flicker; quit the other
program or exclude the device in one of them. Lightloom never kills other processes.

### Uninstall

```sh
systemctl --user disable --now lightloomd; rm ~/.config/systemd/user/lightloomd.service
rm -r ~/.local/lib/lightloom ~/.local/bin/lightloomd ~/.local/bin/lightloom
rm -r ~/.local/share/lightloom ~/.config/lightloom        # settings and state
sudo rm /etc/udev/rules.d/70-lightloom.rules /etc/modules-load.d/lightloom.conf /etc/sysusers.d/lightloom.conf
```

## Part 2: porting notes

### Build

```sh
sudo apt-get install libudev-dev libusb-1.0-0-dev pkg-config   # Debian/Ubuntu
cargo build --release -p lightloom-daemon -p lightloom-cli
cargo build --release -p lightloom-renderer --features cef     # optional; downloads CEF
(cd ui && npm ci --ignore-scripts && npm run build)
tools/package/stage-linux.sh --release target/release --ui ui/dist --out dist/lightloom
```

`stage-linux.sh` picks up the renderer and the CEF runtime that cef-dll-sys copied next to it
(`*.so`, `*.pak`, `*.bin`, `*.dat`, `vk_swiftshader_icd.json`, `locales/`, and `CREDITS.html`
as `THIRD-PARTY/CEF-Chromium-CREDITS.html`). `--require-renderer` (used by the release) fails
when any of it is missing, or when the renderer does not link `libcef.so` through an
`$ORIGIN` rpath.

Test without hardware: `LIGHTLOOM_HOME=$(mktemp -d) lightloomd --no-hardware --port 53001`.

### Where the Linux code lives

| Area | Code | Work package |
|---|---|---|
| hidraw (enumeration, report descriptor collections, feature reports) | `crates/lightloom-hal-linux/src/{hidraw,sysfs,tlc}.rs` | WP-L1, done |
| hotplug (kernel uevents) | `crates/lightloom-hal-linux/src/{hotplug,uevent}.rs` | WP-L2, done |
| udev rules from manifests | `crates/lightloom-cli/src/udev.rs` (`lightloom udev generate`), `packaging/udev/` | WP-L2, done |
| `/dev/i2c-*` SMBus, hwmon sensors | `crates/lightloom-hal-linux/src/{i2c,hwmon,sys}.rs`, `lightloom-runtime/src/smbus.rs` | WP-L4, done |
| raw USB (usbfs through nusb: sysfs enumeration, kernel-driver policy, transfers) | `crates/lightloom-hal-linux/src/usb.rs`, `lightloom-runtime/src/hardware.rs` | WP-L3, done |
| serial ports | `crates/lightloom-hal-linux/src/tty.rs` | done |
| sleep/lock (logind), notifications, tray (StatusNotifierItem) | `crates/lightloom-hal-linux/src/{dbus,logind,notifications}.rs`, `lightloom-runtime/src/{power,notify}.rs`, `crates/lightloom-tray/src/sni.rs` | WP-L8, done |
| CEF renderer, shm, control socket | `lightloom-renderer/src/ipc.rs` (tmpfs ring, unix socket), `lightloom-render/src/cef.rs`; see `crates/lightloom-renderer/README.md` "Linux" | WP-L5, done (CI only); in the tarball and AppImage, not in the Flatpak |
| PipeWire audio, ScreenCast portal | `lightloom-inputs` | WP-L6 |
| packaging, release | `packaging/{systemd,appimage,flatpak}`, `tools/package/stage-linux.sh`, `.github/workflows/release.yml` | WP-L7 |

Background research and primary sources: `docs/research/m7-linux-backend.md`.

Rules for Linux code:

- The daemon stays unprivileged. Access comes from udev rules, never from setuid, file
  capabilities or polkit.
- D-Bus goes through zbus (pure Rust) and the blocking helpers in
  `lightloom_hal_linux::dbus`. Never call them from a method of a served interface (those run on
  zbus's runtime): queue the work for another thread, as the tray does.
- Tests of D-Bus code use peer-to-peer connections over a socket pair with a fake service on
  the other end (`logind.rs`, `notifications.rs`, `sni.rs`); they never touch the real session
  or system bus. Any new device class adds a rule shape to `lightloom udev
  generate` and a line in `packaging/udev/README.md`.
- Regenerate `packaging/udev/70-lightloom.rules` when a manifest changes:
  `lightloom udev generate --plugins plugins --out packaging/udev/70-lightloom.rules`.
  `udev::tests::first_party_rules_match_the_packaged_file` fails when it is stale.
- Never match I2C adapters by number: adapter numbers change between boots. Match by
  adapter name and, where it matters, the parent PCI device.
- Resources (plugins, effects, ui, components) are found next to the real executable
  (`crates/lightloom-daemon/src/paths.rs`). `/proc/self/exe` resolves symlinks, which is
  what makes the `~/.local/bin` symlinks and the AppImage `usr/bin` symlinks work.
- State follows XDG: `$XDG_DATA_HOME/lightloom` and `$XDG_CONFIG_HOME/lightloom`
  (`crates/lightloom-runtime/src/dirs.rs`); `LIGHTLOOM_HOME` overrides both.

### systemd user unit

`packaging/systemd/lightloomd.service`:

- `ExecStart=%h/.local/bin/lightloomd`, `Restart=on-failure` with a 2 s delay, at most 5
  starts a minute, and `WantedBy=default.target` ordered `After=graphical-session.target`.
  `default.target` rather than `graphical-session.target` so window-manager-only and
  headless sessions, which never reach the latter, still get lighting.
- Hardening is limited to options a user manager can apply without a mount namespace
  (`NoNewPrivileges`, `RestrictSUIDSGID`, `RestrictRealtime`, `LockPersonality`,
  `SystemCallArchitectures=native`, `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
  AF_NETLINK`, `UMask=0077`). The unit lists what is left out and why: mount-namespace
  options fail on systems without unprivileged user namespaces and `PrivateDevices` would
  hide hidraw and i2c; `DeviceAllow` needs a cgroup controller that user managers do not
  get; `MemoryDenyWriteExecute`, `RestrictNamespaces` and `SystemCallFilter` would break
  the Chromium renderer.
- Checked with `systemd-analyze verify` and `systemd-analyze --user verify` (systemd 255).
  Not yet run as a real user service on Linux hardware.
- Open: `Type=notify` with `WatchdogSec=` (needs `sd_notify` in the daemon). The logind
  sleep inhibitor is taken by the daemon itself (`lightloom_hal_linux::logind`).

### AppImage and Flatpak

- `packaging/appimage/`: linuxdeploy with a custom `AppRun`; the staged tree lives in
  `usr/lib/lightloom` and linuxdeploy bundles the daemon's and CLI's libraries with
  `--deploy-deps-only`. The renderer and the CEF runtime are moved aside while linuxdeploy
  runs (it would patch the rpath of libcef.so and the renderer and bundle NSS and Mesa) and
  put back before linuxdeploy's AppImage plugin writes the image. Details:
  `packaging/appimage/README.md`.
- `packaging/flatpak/`: Freedesktop 25.08 runtime with the rust-stable and node24 SDK
  extensions, offline cargo and npm sources generated by
  `packaging/flatpak/generate-sources.sh` (flatpak-builder-tools, pinned). Built weekly by
  `.github/workflows/flatpak.yml`. Details and caveats: `packaging/flatpak/README.md`.
- Both reuse one launcher (`packaging/appimage/AppRun`) and one desktop entry.
- Chromium sandbox: off in every Linux build, as on Windows (`cef_host.rs`). Turning it on
  would need the SUID helper (impossible in an AppImage or a tarball run as the user) or
  unprivileged user namespaces (restricted by AppArmor on Ubuntu 24.04 and later); the
  Flatpak needs zypak. A `.deb`/`.rpm` with `chrome-sandbox` root 4755 and an AppArmor
  profile is the fully sandboxed option.

### Release

`.github/workflows/release.yml` runs on a `v*` tag: its `build` job calls the reusable
`.github/workflows/release-build.yml`, and `publish` makes a draft GitHub release from the
result. `ci.yml`'s `release-dry-run` calls the same reusable workflow weekly (schedule, default
branch) and on manual dispatch, without publishing, so a tag never runs untested steps.
`.github/workflows/flatpak.yml` builds the Flatpak on its own weekly schedule.

`release-build.yml` jobs:

- `meta`: the version. A `v1.2.3` tag gives `1.2.3` and must match Cargo.toml's
  `[workspace.package]` version (pre-release suffix aside), or the build stops: the in-app
  updater compares the version the binaries report with the tag. Other refs give
  `<workspace version>-<short sha>`.
- `windows`: the workspace and the CEF renderer with cargo-auditable, `stage-windows.ps1`,
  `lightloom-x86_64-pc-windows-msvc.zip`, CycloneDX SBOMs.
- `msi`: `Lightloom-<x.y.z>-x64.msi` from the same staged tree (the name the updater looks
  for).
- `linux`: on `ubuntu-22.04` (glibc 2.35 baseline), the daemon and CLI, then the renderer with
  `--features cef` in a separate cargo run (so the `cef` feature cannot change the binaries
  `linux-reproducible` compares), all with cargo-auditable; the dashboard; `stage-linux.sh
  --require-renderer`; the systemd unit and dashboard check; a 60-frame headless smoke run
  of the staged renderer (Xvfb fallback, as in CI); `lightloom-<version>-linux-x86_64.tar.xz`
  (sorted, owner 0, mtimes from the commit, single-threaded `xz -6`);
  `Lightloom-<version>-x86_64.AppImage`; CycloneDX SBOMs.
- `linux-reproducible`: builds the daemon and CLI twice from the same checkout into two
  target directories, with `--remap-path-prefix` for the workspace, `CARGO_HOME` and the
  target directory, and fails when the SHA-256 of either binary differs.
- `assemble`: checks that the zip, the MSI, the tarball, the AppImage and both sets of SBOMs
  are there under their expected names, writes one `SHA256SUMS` over every file (the updater
  verifies its download against it) and uploads them as the `lightloom-release` artifact.
  It waits for `linux-reproducible`, so a build that is not reproducible is not released.

Release builds use no rust-cache: they start clean, and a restored cache would skip
cef-dll-sys's build script, which copies the CEF runtime next to the renderer. Only the CEF
distribution is cached, keyed by its version. cargo-auditable and cargo-cyclonedx are pinned
(`CARGO_AUDITABLE_VERSION`, `CARGO_CYCLONEDX_VERSION`).

The `x86_64-unknown-linux-gnu.tar.gz` of the old `build` matrix (two binaries, no dashboard)
is gone; the updater still accepts that name as a last fallback.

Reproducibility limits: both builds use the same checkout path (`env!("CARGO_MANIFEST_DIR")`
in `paths.rs` embeds it and `--remap-path-prefix` does not rewrite `env!` strings), the
same toolchain and the same `Cargo.lock`. The renderer, the AppImage and the dashboard bundle
are not checked yet (linuxdeploy writes the squashfs with its own timestamps).

### cargo-dist evaluation

WP-L7 planned to replace the hand-written `release.yml` with a cargo-dist generated one.
Deferred, for these reasons:

- `dist generate` owns the whole workflow file and rewrites it; parallel branches edit
  `release.yml` now (the installer work adds the MSI job). Switching while those are open
  would turn every merge into a conflict.
- The release is mostly not cargo artifacts: the CEF runtime next to the renderer, the
  dashboard build, plugins and effects, `stage-windows.ps1`, the WiX v4 MSI from
  `packaging/wix` (cargo-dist's own MSI installer uses its WiX v3 template), the AppImage
  and the Flatpak. All of these would be custom jobs or build-setup steps inside the
  generated file.
- What cargo-dist would give us for free is already in place: cargo-auditable, CycloneDX
  SBOMs, checksums, a draft GitHub release.

Migration plan, after the installer branch and WP-L5 are merged into develop:

1. `dist init` on a branch with `dist-workspace.toml`: targets
   `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`, `cargo-auditable = true`,
   `cargo-cyclonedx = true`, `github-build-setup` for the CEF and Node steps, `include`
   for `ui/dist`, `plugins`, `effects`, `components` and the license texts, `dist = false`
   on every crate except `lightloom-daemon`, `lightloom-cli` and `lightloom-renderer`.
2. Move the MSI, AppImage, Flatpak and reproducibility jobs into reusable workflows and
   hook them in with `local-artifacts-jobs` / `global-artifacts-jobs`; keep
   `stage-windows.ps1` and `stage-linux.sh` as the staging steps.
3. Run the generated workflow with `workflow_dispatch` (dry run) next to the current one
   and compare archive contents file by file.
4. Switch, and record the `dist` version in `dist-workspace.toml` so `dist generate`
   is reproducible; CI checks `dist plan` so hand edits to the generated file fail.
