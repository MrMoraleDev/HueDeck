<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# HueDeck

**A standalone, open-source RGB manager for Windows (Linux next): one download,
your lights, no account, no telemetry, no ads.** Devices are driven by HueDeck's own
plugin library; effects are HTML5-canvas lightscripts; a local dashboard runs in
your browser. Compatibility with SignalRGB's plugins and effects is an optional
extra, never a requirement.

Status: **pre-alpha, M4 (daemon and dashboard)**. `huedeckd` drives the connected
devices with their plugins, renders effects in offscreen Chromium, keeps settings in
SQLite and serves the dashboard and a local API on `127.0.0.1:16138`. Its own plugin
library covers only an MSI Mystic Light board so far; growing it is the current
priority (see [How device support works](#how-device-support-works)). No installer
yet. See [Milestones](#milestones).

## What HueDeck is

- `huedeckd` - per-user daemon, unprivileged: device registry and plugin host,
  layout and sampling, effect pipeline, persistence (SQLite), local HTTP/WS API and
  the dashboard (`http://127.0.0.1:16138`).
- `huedeck` - command-line tool (`devices`, `light`, `rainbow`, `effect`, `bench`,
  `record`/`replay`, `compat dry-run`).
- `huedeck-renderer` - effects run in real Chromium (CEF, offscreen) in their own
  process and reach the daemon as 320x200 frames through shared memory.
- `huedeck-iobroker` - **optional** Windows service for SMBus / GPU-I2C / SuperIO
  through [PawnIO](https://pawnio.eu/); installed only on explicit opt-in. Not
  needed on Linux and not needed for HID, USB, serial or network devices.

No custom kernel driver, no account, no telemetry, no ads, no process killing, no
marketplace.

## Architecture summary

```
+---------------------------------------------------------------------------------+
| Dashboard (any browser, http://127.0.0.1:16138) | 3rd-party tools (:16038/:16034)|
+-----------------------+---------------------------------------------------------+
                        | loopback HTTP/WS (native API token-authed; compat shape-copied)
+-----------------------v---------------------------------------------------------+
| huedeckd   per-user, UNPRIVILEGED, tokio                                        |
|   huedeck-api ---> huedeck-engine: registry | layouts + sampling | FrameStore    |
|                    scheduler | channels/components | fan curves | SQLite | events |
|   huedeck-plugin-host   one rquickjs runtime per plugin OS thread               |
|                         device.* / bus.* / channel / service / controller       |
|                         @SignalRGB/* shims (plain JS prelude), compat-v1 profile |
|   huedeck-inputs        audio FFT (200 bins) | screen zones 28x20 | sensors      |
|   huedeck-render        RendererClient: null (M1, CI) | cef (shm frames)         |
|   huedeck-hal           traits + mock; hal-windows (hidapi, nusb/WinUSB,         |
|                         serialport, CM_Register_Notification, LampArray)         |
|                         hal-linux (hidraw, nusb, serialport, udev, i2cdev, hwmon) |
+-----------+----------------------------------------+----------------------------+
            | shm: 3 x 320x200 BGRA + engine data    | named pipe / unix socket
            | + seq, control pipe                    | postcard, closed op enum
+-----------v--------------------+    +--------------v-----------------------------+
| huedeck-renderer (user)        |    | huedeck-iobroker (Windows service,         |
|  CEF offscreen Chromium        |    |  LocalSystem, OPTIONAL, opt-in install)    |
|  320x200 @ 30 fps              |    |  PawnIO client (pinned modules), NvAPI I2C |
|  URL filter = effect dir only  |    |  one global mutex per SMBus transaction,   |
|  +1 browser per LCD face       |    |  allowlists, SPD write denial, audit log   |
+--------------------------------+    +--------------------------------------------+
```

Frame path: Chromium paints the effect -> shm slot + sequence number -> daemon
FrameStore (triple buffer) -> each plugin thread wakes, samples its LEDs through a
precomputed offset table, applies brightness/lighting mode, calls the plugin's
`Render()` -> HID/USB/serial/SMBus write on that same thread. Slow devices skip to
the latest frame; nothing queues.

Long form: [docs/architecture.md](docs/architecture.md) (concise, current) and
[docs/architecture-synthesis.md](docs/architecture-synthesis.md) (panel synthesis,
decision log D1-D18). Decisions: [docs/adr/](docs/adr/).

## How device support works

A device plugin is one small JavaScript file per device *family*: it says which USB
ids it drives and turns the LED colours HueDeck samples from the canvas into the
packets the device expects. Supporting another model of a family HueDeck already
has usually means adding its product id to that file. The host API is the one
SignalRGB plugins are written against (documented in [docs/host-api/](docs/host-api/)),
so both kinds run on the same loader.

1. **HueDeck's own plugin library** (`plugins/`, MIT) is what ships with HueDeck and
   needs nothing else installed. Its protocols come from the devices' own traffic
   (HueDeck records and replays every HID exchange: `huedeck light ... --record`,
   `huedeck compat replay`), public standards (HID LampArray, QMK/VIA) and vendors'
   published APIs (network lights). No code is copied from other RGB software.
   Today it covers the MSI Mystic Light 185-byte boards (tested on `1462:7C91`);
   Razer, NZXT Kraken, SteelSeries headsets, generic HID LampArray devices and
   network lights (WLED, Philips Hue, Govee, Nanoleaf, LIFX, Yeelight) are next.
2. **Your own plugins and effects**: `%APPDATA%\HueDeck\plugins` and `...\effects`.
3. **Optional: your SignalRGB installation.** With the setting on, HueDeck also
   uses the plugins, effects and components of a local SignalRGB installation, read
   where they are (never copied, never changed), and can import its canvas layout
   and channel setup. HueDeck never ships, downloads or redistributes
   WhirlwindFX-authored files.

Plugins the host does not understand fail loudly: an unknown host member is logged
once as `API_GAP` and then throws, and `huedeck compat dry-run <dir>` reports the
gaps for a whole directory before you depend on it.

## Quick start

There is no release yet. From source (stable Rust toolchain, Windows for now):

```text
cargo build --release                          # huedeckd and huedeck
cargo build --release -p huedeck-renderer --features cef   # effects (CMake + Ninja on PATH)
cd ui && npm ci && npm run build               # the dashboard (ui/dist)
target/release/huedeckd --open                 # devices, effects, API; opens http://127.0.0.1:16138
```

`huedeckd` finds the renderer, the dashboard, its plugins and starter effects next
to its executable (in a source checkout: the tree's `plugins/`, `effects/` and
`ui/dist`). Settings live in `%LOCALAPPDATA%\HueDeck` (database, API token) and
`%APPDATA%\HueDeck\config.toml` (ports, frame rate, extra plugin directories,
devices to leave alone); `--home DIR` keeps everything in one directory,
`--dry-run` never writes to devices. Close other RGB software that owns the same
devices: HueDeck leaves them alone while it runs and tells you so, it never
terminates other applications.

The command-line tool drives devices without the daemon (stop `huedeckd` first):

```text
target/release/huedeck devices                        # read-only: devices, collections, report lengths
target/release/huedeck light red --dry-run            # read the device, print the packets, write nothing
target/release/huedeck light "#00ff00" --restore-check  # hold a colour until Ctrl-C, then restore and verify
target/release/huedeck effect effects/plasma.html --prop speed=60   # render an effect onto every device
target/release/huedeck light red --record t.json      # record every HID operation ...
target/release/huedeck compat replay t.json           # ... and replay it against the plugins, no hardware
target/release/huedeck bench --mock 1462:7c91         # latency percentiles against a scripted device
```

`--signalrgb` adds a local SignalRGB installation's plugins and channel setup
(read in place). Effects results on the reference machine are in
[docs/hardware/effects-m3.md](docs/hardware/effects-m3.md); the daemon and dashboard
in [docs/hardware/daemon-m4.md](docs/hardware/daemon-m4.md).

## Milestones

| Milestone | Scope | Status |
|---|---|---|
| M0 | Workspace, pins, cargo-deny, CI (mock HAL), HAL traits, ADRs, Host API spec, CEF and rquickjs spikes | done |
| M1 | First light, headless: MSI Mystic Light board from the CLI | done |
| M2 | Compatible plugin host, corpus harness, hotplug, conformance suite | done (physical unplug test pending) |
| M3 | Effects pipeline: CEF renderer, layouts, sampling, `huedeck bench` | done |
| M4 | Daemon, SQLite persistence, local API, dashboard, import, installer | in progress: daemon, API, dashboard, SignalRGB import done; installer next |
| Devices | HueDeck's own plugin library: Razer, NZXT, SteelSeries, MSI families, HID LampArray, network lights | in progress |
| M5 | Privileged bus via `huedeck-iobroker` + PawnIO, NvAPI I2C, sensors | planned |
| M6 | Audio/screen inputs, network device support, LCD faces, macros, OpenRGB SDK server | planned |
| M7 | Linux backend, AppImage/Flatpak, 1.0 | planned |

## Repository layout

```
crates/      Rust workspace crates (huedeck-hal, -plugin-host, -engine, -render, -renderer, -iobroker, ...)
prelude/     plain-JS host shims (@SignalRGB/* facades, BinaryUtils, Errors, base64)
plugins/     first-party MIT plugins        effects/     CC0 starter lightscripts
components/  own component JSON library     schemas/     JSON Schemas + OpenAPI
sdk/         huedeck.d.ts, Node simulator   tests/       conformance + integration
docs/        host-api/ (spec), adr/, architecture.md, hardware/
packaging/   wix, winget, systemd, udev, appimage, flatpak    tools/   gen-udev, pawnio-hash-pin, shm-dump
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) (clean-room rule, DCO sign-off, conventional
commits, how to run tests), [SECURITY.md](SECURITY.md) and
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

Code and the Host API specification: **Apache-2.0 OR MIT**, at your option
([LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT)). First-party plugins
under `plugins/` are MIT; starter effects under `effects/` are CC0-1.0. Pawn modules
for PawnIO are LGPL-2.1 and upstreamed to PawnIO.Modules, never kept in this tree.
Details in [ADR-0006](docs/adr/0006-license-apache-mit.md).

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in HueDeck by you shall be dual licensed as above, without any additional
terms or conditions.

## Trademarks

SignalRGB is a trademark of Whirlwind Virtual Realities Inc. HueDeck is an
independent project, not affiliated with, endorsed by or sponsored by Whirlwind
Virtual Realities Inc.; the name is used only nominatively to describe
compatibility. OpenRGB, PawnIO, Chromium, NVIDIA, AMD and all device vendor names
are trademarks of their respective owners.
