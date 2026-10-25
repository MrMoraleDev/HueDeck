<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Lightloom crates

Workspace members (`Cargo.toml` → `members = ["crates/*"]`). Shared versions are pinned once in the
root `[workspace.dependencies]`; every crate uses `{ workspace = true }` and lists only what it uses.
Status as of `develop` 2026-10-23; the milestone view is `docs/status.md`.

## Processes

| Binary | Crate | Runs as | Job |
|---|---|---|---|
| `lightloomd` | `lightloom-daemon` | the user | device sessions, canvas, renderer supervision, SQLite state, the loopback API and the dashboard |
| `lightloom` | `lightloom-cli` | the user | headless runs (`light`, `rainbow`, `bench`, `devices`, `plugins`, `compat dry-run`) and `url` |
| `lightloom-renderer` | `lightloom-renderer` (feature `cef`) | the user | one windowless CEF per effect, frames into shared memory |
| `lightloom-iobroker` | `lightloom-iobroker` | LocalSystem (optional) | chipset SMBus through PawnIO, over a named pipe |
| `lightloom-setup` | `lightloom-setup` | Administrator, once (optional) | registers / removes the I/O broker |

## Dependency graph

```text
                         lightloom-hal  (traits + mock; no OS code)
          ┌──────────────┬───────┴─────────┬──────────────────┐
          ▼              ▼                 ▼                  ▼
 lightloom-hal-windows  lightloom-hal-linux  lightloom-nvapi   lightloom-plugin-host
  (hidapi + native     (stub, M7)        (NvAPI GPU I2C)
   HID, broker client)
     ▲    ▲
     │    └── lightloom-hid-desc (report descriptors; also used by plugin-host)
     └─────── lightloom-protocol (shm frame ring, broker ops) ◄── lightloom-iobroker ◄── lightloom-setup
                                                          ◄── lightloom-renderer ◄── lightloom-render

 lightloom-effects (<meta> parser)      ──► lightloom-renderer, lightloom-runtime, lightloom-cli
 lightloom-engine (colour, frames, layout, sampler, components)
                                      ──► render, plugin-host, api, runtime, daemon, cli

 lightloom-runtime  ◄── hal, hal-windows, nvapi, plugin-host, engine, effects
 lightloom-api      ◄── engine
 lightloom-daemon   ◄── api, engine, hal, plugin-host, render, runtime
 lightloom-cli      ◄── api, engine, effects, hal, hal-windows, plugin-host, render, runtime
 lightloom-inputs   (shapes only; nothing depends on it yet)
```

`A ◄── B` reads "B depends on A". OS-specific dependencies sit in `cfg(windows)` /
`cfg(target_os = "linux")` target tables, so every crate builds and tests on both OSes.

## Crate summary

| crate | kind | status | contents |
|---|---|---|---|
| `lightloom-hal` | lib | done | `HidBackend/HidHandle`, `Notifier`, `UsbBackend`, `SerialBackend`, `SmbusBus` + `SmbusBusDescriptor{kind}`, `HotplugSource`, `SensorBackend`, `CaptureBackend`, `AudioBackend`, `LampArrayBackend` (+ `plugin_owner`), `HalError`, `DeviceKey`; feature `mock`: `MockHid`, `MockSmbus`, `MockSerial`, `TransactionRecorder` |
| `lightloom-hal-windows` | lib | HID and serial done; USB missing | `path` (HID interface paths → `{interface, collection}`), `WindowsHidBackend` (hidapi `windows-native`), `NativeHidBackend` (HID class driver, GET_REPORT(Input)), `group`, `hotplug`, `broker::BrokerSmbus` (I/O broker client as `SmbusBus`), PawnIO presence, process and port probes, toast notifications, `lamparray` (WinRT Windows Dynamic Lighting backend), `serial` (`WindowsSerialBackend`: COM ports with their USB VID:PID; port I/O over `serialport`, shared with Linux) |
| `lightloom-hal-linux` | lib | stub (M7) | placeholder for hidraw, nusb, serialport, udev, `/dev/i2c-*`, hwmon |
| `lightloom-hid-desc` | lib | done | HID 1.11 report-descriptor parser: items, top-level collections, report ids and sizes, report fields (LampArray) |
| `lightloom-protocol` | lib | done | `shm` frame ring (`"HDFR"`, 3 slots, seqlock), `broker` operations (protocol 3: `BusLock`, `Batch`, `SpdRead`), postcard framing |
| `lightloom-engine` | lib | done | `Rgb`, `Frame`, `FrameStore`, `Layout`/`Placement` (= `schemas/layout.schema.json`), the sampler, brightness math, components and channel instances |
| `lightloom-render` | lib (+ test bin) | done | `NullRenderer` (solid, rainbow, test patterns), `CefRenderer` (spawns and supervises `lightloom-renderer --serve`), `FrameClock` |
| `lightloom-renderer` | lib + bin | done; Chromium sandbox off | CEF windowless host (feature `cef`), shm ring writer, `window.engine` and control injection, URL filter, control pipe, snapshots and stats |
| `lightloom-plugin-host` | lib | HID, SMBus, network and serial done; rawusb missing | QuickJS-NG runtime per plugin (64 MiB, deadlines), `js/host.js` compat host, natives (`device`, `children`, `bus`, `net`, `serial`), network services (`service`: DiscoveryService, mDNS, settings) and network device sessions (LAN-only sockets and HTTP), `@SignalRGB/*` / `@lightloom/*` resolver, catalogue and VID:PID index, device sessions, the `battery` feature, SMBus policy and SPD check, `compat dry-run` |
| `lightloom-effects` | lib | done | `parse_effect_meta(html)` (= `schemas/effect-meta.schema.json`), control coercion, JS initialisers |
| `lightloom-inputs` | lib | shapes only (M6) | `AudioSnapshot`, `ZoneSnapshot`, `InputSource` |
| `lightloom-api` | lib | native done; compat partial | axum routers: native `/v1` REST + WS + dashboard (`docs/api/openapi.yaml`), SignalRGB-compatible REST and canvas events, loopback / Origin / token guard, movable native listener |
| `lightloom-runtime` | lib | done | what CLI and daemon share: HID backend and device matching, SMBus buses and scans, hotplug, conflict detection, the SignalRGB setup reader, data directories, system info |
| `lightloom-nvapi` | lib | done | dynamic `nvapi64.dll` binding, one NvAPI worker thread, `NvapiBus: SmbusBus` per GPU |
| `lightloom-iobroker` | lib + bin | done (AMD PIIX4; Intel I801 untested on hardware) | LocalSystem service: PawnIO IOCTLs, pinned modules, PIIX4 / I801 transfers, global SMBus mutex, static policy and rate limit, named pipe with DACL, audit log |
| `lightloom-setup` | bin | done | `--smbus` (checks PawnIO, never installs it; copies the broker and modules, registers the service), `--uninstall`, `--plan` |
| `lightloom-daemon` | lib + bin `lightloomd` | done | single instance, token, port choice, engine thread (sessions, canvas, renderer, SQLite), battery thresholds and alerts, API serving, a shutdown that restores devices |
| `lightloom-cli` | bin `lightloom` | done | `devices`, `light`, `rainbow`, `bench` (`--mock`), `plugins`, `compat dry-run`, `url` |

Test counts (`#[test]` and `#[tokio::test]` per crate, 2026-10-23): plugin-host 128, iobroker 52,
hal-windows 44, renderer 28, engine 26, hal 26, daemon 22, nvapi 19, cli 17, api 16, protocol 15,
effects 12, runtime 12, render 10, hid-desc 8, setup 5, inputs 1, hal-linux 0.

## Outside the crates

- `plugins/`: first-party plugins (JS) and their Node tests; `plugins/_testkit` holds the mocks.
- `prelude/`: the JS shims embedded into the plugin host.
- `sdk/`: plugin typings (`lightloom.d.ts`), `sdk/lint`, `sdk/simulator` (`sdk/README.md`).
- `effects/`: first-party lightscripts (CC0). `schemas/`: JSON Schemas. `ui/`: the dashboard.
- `docs/security.md`: the threat model and what is in place.

## Conventions

* Every file starts with `SPDX-License-Identifier: GPL-3.0-or-later` (adapted OpenRGB files
  also carry a credit header naming the source files and commit).
* `#![forbid(unsafe_code)]` everywhere except OS backends and FFI (`hal-windows`, `nvapi`,
  `iobroker`, `renderer`), where each `unsafe` block carries a `// SAFETY:` comment.
* Keep each crate's dependency list to what it actually uses; add workspace pins in the root `Cargo.toml` first.
* The core is device-agnostic: brand and protocol knowledge lives in plugins and manifests only.
* Serde names of `Layout`/`Placement` and `EffectMeta` are checked against the `schemas/*.schema.json`
  examples by unit tests, and the native API routes against `docs/api/openapi.yaml`
  (`crates/lightloom-api/tests/openapi.rs`). Keep both sides in sync.
* Never copy code from SignalRGB/WhirlwindFX files; facts about the plugin contract come from
  `docs/host-api/*.md`. OpenRGB code only from its GPL-2.0-or-later files, with credit.
