<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# HueDeck crates

Workspace members (`Cargo.toml` → `members = ["crates/*"]`). Shared versions are pinned once in the
root `[workspace.dependencies]`; every crate uses `{ workspace = true }` and lists only what it uses.
Status as of `develop` 2026-10-23; the milestone view is `docs/status.md`.

## Processes

| Binary | Crate | Runs as | Job |
|---|---|---|---|
| `huedeckd` | `huedeck-daemon` | the user | device sessions, canvas, renderer supervision, SQLite state, the loopback API and the dashboard |
| `huedeck` | `huedeck-cli` | the user | headless runs (`light`, `rainbow`, `bench`, `devices`, `plugins`, `compat dry-run`) and `url` |
| `huedeck-renderer` | `huedeck-renderer` (feature `cef`) | the user | one windowless CEF per effect, frames into shared memory |
| `huedeck-iobroker` | `huedeck-iobroker` | LocalSystem (optional) | chipset SMBus through PawnIO, over a named pipe |
| `huedeck-setup` | `huedeck-setup` | Administrator, once (optional) | registers / removes the I/O broker |

## Dependency graph

```text
                         huedeck-hal  (traits + mock; no OS code)
          ┌──────────────┬───────┴─────────┬──────────────────┐
          ▼              ▼                 ▼                  ▼
 huedeck-hal-windows  huedeck-hal-linux  huedeck-nvapi   huedeck-plugin-host
  (hidapi + native     (stub, M7)        (NvAPI GPU I2C)
   HID, broker client)
     ▲    ▲
     │    └── huedeck-hid-desc (report descriptors; also used by plugin-host)
     └─────── huedeck-protocol (shm frame ring, broker ops) ◄── huedeck-iobroker ◄── huedeck-setup
                                                          ◄── huedeck-renderer ◄── huedeck-render

 huedeck-effects (<meta> parser)      ──► huedeck-renderer, huedeck-runtime, huedeck-cli
 huedeck-engine (colour, frames, layout, sampler, components)
                                      ──► render, plugin-host, api, runtime, daemon, cli

 huedeck-runtime  ◄── hal, hal-windows, nvapi, plugin-host, engine, effects
 huedeck-api      ◄── engine
 huedeck-daemon   ◄── api, engine, hal, plugin-host, render, runtime
 huedeck-cli      ◄── api, engine, effects, hal, hal-windows, plugin-host, render, runtime
 huedeck-inputs   (shapes only; nothing depends on it yet)
```

`A ◄── B` reads "B depends on A". OS-specific dependencies sit in `cfg(windows)` /
`cfg(target_os = "linux")` target tables, so every crate builds and tests on both OSes.

## Crate summary

| crate | kind | status | contents |
|---|---|---|---|
| `huedeck-hal` | lib | done | `HidBackend/HidHandle`, `Notifier`, `UsbBackend`, `SerialBackend`, `SmbusBus` + `SmbusBusDescriptor{kind}`, `HotplugSource`, `SensorBackend`, `CaptureBackend`, `AudioBackend`, `LampArrayBackend`, `HalError`, `DeviceKey`; feature `mock`: `MockHid`, `MockSmbus`, `TransactionRecorder` |
| `huedeck-hal-windows` | lib | HID done; USB and serial missing | `path` (HID interface paths → `{interface, collection}`), `WindowsHidBackend` (hidapi `windows-native`), `NativeHidBackend` (HID class driver, GET_REPORT(Input)), `group`, `hotplug`, `broker::BrokerSmbus` (I/O broker client as `SmbusBus`), PawnIO presence and the `huedeck-setup` contract (arguments, exit codes, quoting), `elevate::run_elevated` (UAC `runas`, on a user action only), `service::service_state` (read-only), process and port probes, toast notifications |
| `huedeck-hal-linux` | lib | stub (M7) | placeholder for hidraw, nusb, serialport, udev, `/dev/i2c-*`, hwmon |
| `huedeck-hid-desc` | lib | done | HID 1.11 report-descriptor parser: items, top-level collections, report ids and sizes, report fields (LampArray) |
| `huedeck-protocol` | lib | done | `shm` frame ring (`"HDFR"`, 3 slots, seqlock), `broker` operations (protocol 3: `BusLock`, `Batch`, `SpdRead`), postcard framing |
| `huedeck-engine` | lib | done | `Rgb`, `Frame`, `FrameStore`, `Layout`/`Placement` (= `schemas/layout.schema.json`), the sampler, brightness math, components and channel instances |
| `huedeck-render` | lib (+ test bin) | done | `NullRenderer` (solid, rainbow, test patterns), `CefRenderer` (spawns and supervises `huedeck-renderer --serve`), `FrameClock` |
| `huedeck-renderer` | lib + bin | done; Chromium sandbox off | CEF windowless host (feature `cef`), shm ring writer, `window.engine` and control injection, URL filter, control pipe, snapshots and stats |
| `huedeck-plugin-host` | lib | HID and SMBus done; rawusb, serial, network missing | QuickJS-NG runtime per plugin (64 MiB, deadlines), `js/host.js` compat host, natives (`device`, `children`, `bus`), `@SignalRGB/*` / `@huedeck/*` resolver, catalogue and VID:PID index, device sessions, the `battery` feature, SMBus policy and SPD check, `compat dry-run` |
| `huedeck-effects` | lib | done | `parse_effect_meta(html)` (= `schemas/effect-meta.schema.json`), control coercion, JS initialisers |
| `huedeck-inputs` | lib | shapes only (M6) | `AudioSnapshot`, `ZoneSnapshot`, `InputSource` |
| `huedeck-api` | lib | native done; compat partial | axum routers: native `/v1` REST + WS + dashboard (`docs/api/openapi.yaml`), SignalRGB-compatible REST and canvas events, loopback / Origin / token guard, movable native listener |
| `huedeck-runtime` | lib | done | what CLI and daemon share: HID backend and device matching, SMBus buses and scans, hotplug, conflict detection, the SignalRGB setup reader, data directories, system info, PawnIO / RAM lighting facts and the elevated helper run (`pawnio`) |
| `huedeck-nvapi` | lib | done | dynamic `nvapi64.dll` binding, one NvAPI worker thread, `NvapiBus: SmbusBus` per GPU |
| `huedeck-iobroker` | lib + bin | done (AMD PIIX4 only) | LocalSystem service: PawnIO IOCTLs, pinned modules, PIIX4 transfers, global SMBus mutex, static policy and rate limit, named pipe with DACL, audit log |
| `huedeck-setup` | bin | done | the elevated helper of RAM lighting: `--pawnio-install` (verified WinHTTP download of the official `PawnIO_setup.exe` 2.2.0: size, SHA-256, Authenticode signer `namazso.eu`; `-install -silent`; skipped when PawnIO >= 2.2.0), `--smbus` (pinned modules, default `pawnio-modules` next to it, and the broker under `%ProgramFiles%`, registers and starts `HueDeckIoBroker`, replacing an older one), `--uninstall` (PawnIO stays), `--elevate` (relaunch through UAC), `--pause`, `--plan` |
| `huedeck-winres` | lib (build) | done | build-script helper: icon (`packaging/wix/huedeck.ico`) and version resource of the Windows executables (`winresource`, Windows hosts only) |
| `huedeck-daemon` | lib + bins `huedeckd`, `huedeckw` | done | single instance, token, port choice, engine thread (sessions, canvas, renderer, SQLite), battery thresholds and alerts, API serving, a shutdown that restores devices, Settings → PawnIO / RAM lighting (`pawnio`); `huedeckw` starts the daemon without a window and `--stop`s it |
| `huedeck-cli` | bin `huedeck` | done | `devices`, `light`, `rainbow`, `bench` (`--mock`), `plugins`, `compat dry-run`, `url` |

Test counts (`#[test]` and `#[tokio::test]` per crate, 2026-10-23): plugin-host 128, iobroker 52,
hal-windows 44, renderer 28, engine 26, hal 26, daemon 22, nvapi 19, cli 17, api 16, protocol 15,
effects 12, runtime 12, render 10, hid-desc 8, setup 5, inputs 1, hal-linux 0.

## Outside the crates

- `plugins/`: first-party plugins (JS) and their Node tests; `plugins/_testkit` holds the mocks.
- `prelude/`: the JS shims embedded into the plugin host.
- `sdk/`: plugin typings (`huedeck.d.ts`), `sdk/lint`, `sdk/simulator` (`sdk/README.md`).
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
  (`crates/huedeck-api/tests/openapi.rs`). Keep both sides in sync.
* Never copy code from SignalRGB/WhirlwindFX files; facts about the plugin contract come from
  `docs/host-api/*.md`. OpenRGB code only from its GPL-2.0-or-later files, with credit.
