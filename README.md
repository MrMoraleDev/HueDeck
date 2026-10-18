<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# Lightloom

**Open-source RGB manager for Windows and Linux, compatible with the SignalRGB
plugin, effect and component ecosystem.**

Status: **pre-alpha, M2 (compatible plugin host)**. Headless from the command
line: Lightloom drives an MSI Mystic Light board with its own plugin, and runs
unmodified SignalRGB plugins from a local SignalRGB installation on real devices
(MSI board, Razer mouse, SteelSeries headset, NZXT AIO lighting), with subdevices,
channels and components, hotplug, and HID record/replay. No effect renderer
pipeline, dashboard or installer yet. See [Milestones](#milestones).

## What Lightloom is

Most of what makes an RGB manager useful is not its binary: it is the hundreds of
small JavaScript device plugins (one per device family), the JSON component
descriptors for fans and strips on ARGB headers, and the HTML5-canvas "lightscripts"
that draw the animation. Lightloom implements the **same host API** those files were
written against, so they run unmodified, and wraps it in a small, unprivileged,
open daemon:

- `lightloomd` - per-user daemon: device registry, layout and sampling, plugin host,
  effect pipeline, persistence, local HTTP/WS API, dashboard.
- `lightloom` - command-line client (`devices`, `light`, `rainbow`, `bench`,
  `compat dry-run`, `import`, `udev`).
- `lightloom-renderer` - effects run in real Chromium (CEF, offscreen) in their own
  process and are delivered to the daemon as 320x200 frames through shared memory.
- `lightloom-iobroker` - **optional** Windows service for SMBus / GPU-I2C / SuperIO
  through [PawnIO](https://pawnio.eu/); installed only on explicit opt-in. Not
  needed on Linux and not needed for HID, USB, serial or network devices.

No custom kernel driver, no account, no telemetry, no process killing, no
marketplace.

## Architecture summary

```
+---------------------------------------------------------------------------------+
| Dashboard (any browser, http://127.0.0.1:16138) | 3rd-party tools (:16038/:16034)|
+-----------------------+---------------------------------------------------------+
                        | loopback HTTP/WS (native API token-authed; compat shape-copied)
+-----------------------v---------------------------------------------------------+
| lightloomd   per-user, UNPRIVILEGED, tokio                                        |
|   lightloom-api ---> lightloom-engine: registry | layouts + sampling | FrameStore    |
|                    scheduler | channels/components | fan curves | SQLite | events |
|   lightloom-plugin-host   one rquickjs runtime per plugin OS thread               |
|                         device.* / bus.* / channel / service / controller       |
|                         @SignalRGB/* shims (plain JS prelude), compat-v1 profile |
|   lightloom-inputs        audio FFT (200 bins) | screen zones 28x20 | sensors      |
|   lightloom-render        RendererClient: null (M1, CI) | cef (shm frames)         |
|   lightloom-hal           traits + mock; hal-windows (hidapi, nusb/WinUSB,         |
|                         serialport, CM_Register_Notification, LampArray)         |
|                         hal-linux (hidraw, nusb, serialport, udev, i2cdev, hwmon) |
+-----------+----------------------------------------+----------------------------+
            | shm: 3 x 320x200 BGRA + engine data    | named pipe / unix socket
            | + seq, control pipe                    | postcard, closed op enum
+-----------v--------------------+    +--------------v-----------------------------+
| lightloom-renderer (user)        |    | lightloom-iobroker (Windows service,         |
|  CEF offscreen Chromium        |    |  LocalSystem, OPTIONAL, opt-in install)    |
|  320x200 @ 30 fps, sandbox on  |    |  PawnIO client (pinned modules), NvAPI I2C |
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

Lightloom is a **compatible runtime**, not a plugin collection:

1. **Your own plugin and effect copies.** If you already use SignalRGB, the import
   wizard reads the plugins, effects, components, channel assignments and layouts
   from *your* installation and copies them into Lightloom's data directory, marking
   them `imported / unsigned`. Lightloom never ships, downloads or redistributes
   WhirlwindFX-authored files.
2. **Community repositories as sources.** Public plugin repositories (GitHub /
   GitLab addon repos, SRGBmods, QMK plugin packs, ...) are pre-listed as *sources*.
   Each is enabled only after you acknowledge its license; contents are hash-pinned
   and updated only when you ask.
3. **First-party plugins and effects.** A small set of MIT plugins (`plugins/`)
   written in the same shape, CC0 starter effects (`effects/`) and an own component
   library (`components/`) live in this repository and go through exactly the same
   loader.
4. **The Host API v1 spec** (`docs/host-api/`, Apache-2.0 OR MIT) documents the
   contract so new
   plugins can be written against Lightloom directly (`@lightloom/*` modules, or the
   `@SignalRGB/*` specifiers for interoperability).

Plugins the host does not understand fail loudly: an unknown host member is logged
once as `API_GAP` and then throws, and `lightloom compat dry-run <dir>` reports the
gaps for a whole directory before you depend on it.

## Quick start

There is no release yet. From source (stable Rust toolchain, Windows for now):

```text
cargo build --release
target/release/lightloom devices                        # read-only: devices, collections, report lengths
target/release/lightloom light red --dry-run            # read the device, print the packets, write nothing
target/release/lightloom light "#00ff00" --restore-check  # hold a colour until Ctrl-C, then restore and verify
target/release/lightloom rainbow --duration 60          # scrolling hues through the null renderer
target/release/lightloom bench --mock 1462:7c91         # latency percentiles against a scripted device
target/release/lightloom plugins --signalrgb            # which plugin would drive each connected device
target/release/lightloom rainbow --signalrgb            # also use your SignalRGB plugins and channel setup (read in place)
target/release/lightloom light red --record t.json      # record every HID operation ...
target/release/lightloom compat replay t.json           # ... and replay it against the plugins, no hardware
target/release/lightloom compat dry-run "<your SignalRGB plugin directory>"
```

`light`, `rainbow` and `bench` look for plugins in `./plugins` (or `--plugins DIR`,
`LIGHTLOOM_PLUGINS`); `--signalrgb` adds the plugins of a local SignalRGB
installation, read where they are (never copied), and its component setup per
channel. Devices plugged in, out or back in during a run are followed. Close
other RGB software that owns the same devices first: Lightloom reports conflicting
processes and refuses to start, it never terminates them.
The effect renderer (`lightloom-renderer`, CEF) is built separately with
`cargo build -p lightloom-renderer --features cef`; see its README.

## Milestones

| Milestone | Scope | Status |
|---|---|---|
| M0 | Workspace, pins, cargo-deny, CI (mock HAL), HAL traits, ADRs, Host API spec, CEF and rquickjs spikes | done |
| M1 | First light, headless: MSI Mystic Light board from the CLI | done |
| M2 | Compatible plugin host, corpus harness, hotplug, conformance suite | in progress: imported plugins on 4 devices, subdevices/channels/components, hotplug, record/replay |
| M3 | Effects pipeline: CEF renderer, layouts, sampling, `lightloom bench` | planned |
| M4 | Dashboard, SQLite persistence, local API, import wizard, MSI/winget | planned |
| M5 | Privileged bus via `lightloom-iobroker` + PawnIO, NvAPI I2C, sensors | planned |
| M6 | Audio/screen inputs, network plugins, LCD faces, macros, LampArray, OpenRGB SDK server | planned |
| M7 | Linux backend, AppImage/Flatpak, 1.0 | planned |

## Repository layout

```
crates/      Rust workspace crates (lightloom-hal, -plugin-host, -engine, -render, -renderer, -iobroker, ...)
prelude/     plain-JS host shims (@SignalRGB/* facades, BinaryUtils, Errors, base64)
plugins/     first-party MIT plugins        effects/     CC0 starter lightscripts
components/  own component JSON library     schemas/     JSON Schemas + OpenAPI
sdk/         lightloom.d.ts, Node simulator   tests/       conformance + integration
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
inclusion in Lightloom by you shall be dual licensed as above, without any additional
terms or conditions.

## Trademarks

SignalRGB is a trademark of Whirlwind Virtual Realities Inc. Lightloom is an
independent project, not affiliated with, endorsed by or sponsored by Whirlwind
Virtual Realities Inc.; the name is used only nominatively to describe
compatibility. OpenRGB, PawnIO, Chromium, NVIDIA, AMD and all device vendor names
are trademarks of their respective owners.
