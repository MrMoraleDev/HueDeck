<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Lightloom crates

Workspace members (`Cargo.toml` → `members = ["crates/*"]`). Shared versions are pinned once in the
root `[workspace.dependencies]`; every crate uses `{ workspace = true }` and lists only what it uses.
Status as of `develop` 2026-10-25; the milestone view is `docs/status.md`.

## Processes

| Binary | Crate | Runs as | Job |
|---|---|---|---|
| `lightloomd` | `lightloom-daemon` | the user | device sessions, canvas, renderer supervision, inputs (audio, screen, sensors), SQLite state, the loopback API and the dashboard |
| `lightloomw` | `lightloom-daemon` | the user (Windows) | starts `lightloomd` without a window, its output in `<data>\logs\lightloomd.log`; `--stop`, `--autostart on/off`; starts the tray |
| `lightloom` | `lightloom-cli` | the user | headless runs (`light`, `rainbow`, `effect`, `bench`), `devices`, `effects`, `plugins`, `compat dry-run`/`replay`, `url`, `action`, `udev generate`, `shm-dump` |
| `lightloom-renderer` | `lightloom-renderer` (feature `cef`) | the user | one windowless CEF per effect, frames into shared memory |
| `lightloom-iobroker` | `lightloom-iobroker` | LocalSystem (optional, Windows) | chipset SMBus through PawnIO, over a named pipe |
| `lightloom-setup` | `lightloom-setup` | Administrator, once (optional, Windows) | installs PawnIO on request, registers / removes the I/O broker |
| `lightloom-tray` | `lightloom-tray` | the user (Windows) | notification-area icon: open the dashboard, pause, brightness presets, start at sign-in, quit; talks only to the loopback API |

## Dependency graph

```text
                              lightloom-hal  (traits + mock; no OS code)
      ┌──────────────┬──────────────┬─────┴────────┬──────────────┐
      ▼              ▼              ▼              ▼              ▼
 hal-windows     hal-linux      lightloom-nvapi  lightloom-adl  lightloom-plugin-host
 (HID, WinUSB,   (hidraw, udev, (NVIDIA GPU I2C) (AMD GPU I2C)  (QuickJS host; also engine,
  serial, broker  i2c-dev,                                        hid-desc)
  client, toasts, hwmon, tty)
  LampArray)
      ▲    ▲         ▲    ▲
      │    └─────────┴────┴── lightloom-hid-desc (report descriptors)
      └───────────────────── lightloom-protocol (shm frame ring, broker ops) ◄── lightloom-iobroker ◄── lightloom-setup
                                                                    ◄── lightloom-renderer ◄── lightloom-render

 lightloom-effects (<meta> parser)  ──► lightloom-renderer, lightloom-runtime, lightloom-daemon, lightloom-cli
 lightloom-engine (colour, frames, layout, sampler, components, direct LEDs)
                                  ──► render, plugin-host, api, runtime, daemon, cli
 lightloom-inputs (audio FFT, screen zones and meters; no Lightloom dependencies) ──► daemon

 lightloom-runtime  ◄── hal, hal-windows, hal-linux, nvapi, adl, protocol, plugin-host, engine, effects
 lightloom-api      ◄── engine
 lightloom-macros   ◄── hal, plugin-host
 lightloom-daemon   ◄── api, effects, engine, hal, inputs, macros, plugin-host, render, runtime, tray
 lightloom-cli      ◄── api, engine, effects, hal, hal-windows, plugin-host, render, renderer, runtime
 lightloom-tray     ◄── api, runtime
 lightloom-winres   (build-dependency of every Windows executable)
```

`A ◄── B` reads "B depends on A". OS-specific dependencies sit in `cfg(windows)` /
`cfg(target_os = "linux")` target tables, so every crate builds and tests on both OSes.

## Crate summary

| crate | kind | status | contents |
|---|---|---|---|
| `lightloom-hal` | lib | done | `HidBackend/HidHandle`, `Notifier`, `UsbBackend`, `SerialBackend`, `SmbusBus` + `SmbusBusDescriptor{kind}`, `HotplugSource`, `SensorBackend`, `CaptureBackend`, `AudioBackend`, `LampArrayBackend` (+ `plugin_owner`), `PowerSource`, HID record/replay (`trace`), SMBIOS facts, `HalError`, `DeviceKey`; feature `mock`: `MockHid`, `MockUsb`, `MockSmbus`, `MockSerial`, `TransactionRecorder` |
| `lightloom-hal-windows` | lib | done (WinUSB, serial and LampArray untested on hardware) | `path` (HID interface paths → `{interface, collection}`), `WindowsHidBackend` (hidapi `windows-native`), `NativeHidBackend` (HID class driver, GET_REPORT(Input)), `WinUsbBackend` (`nusb`, WinUSB-bound interfaces only, no INF), `group`, `hotplug` (HID and USB device interfaces), `power` (sleep/resume, lock, display), `broker::BrokerSmbus` (I/O broker client as `SmbusBus`), PawnIO presence and the `lightloom-setup` contract, `elevate::run_elevated` (UAC on a user action only), `service::service_state` (read-only), process and port probes, toast notifications, `lamparray` (WinRT Dynamic Lighting), `serial` (COM ports with their USB VID:PID; I/O over `serialport`) |
| `lightloom-hal-linux` | lib | done, untested on Linux hardware (CI only) | `sysfs` enumeration and `tlc` split (same endpoint tuples as Windows), raw `hidraw` ioctls, `hotplug`/`uevent` (netlink udev monitor), `i2c` (`/dev/i2c-*` by adapter name, SPD writes refused), `hwmon` sensors, `tty` serial ports, `procfs`, `fixtures`. nusb (raw USB) not yet |
| `lightloom-hid-desc` | lib | done | HID 1.11 report-descriptor parser: items, top-level collections, report ids and sizes, report fields (LampArray) |
| `lightloom-protocol` | lib | done | `shm` frame ring (`"HDFR"`, 3 slots, seqlock), `broker` operations (protocol 3: `BusLock`, `Batch`, `SpdRead`), postcard framing |
| `lightloom-engine` | lib | done | `Rgb`, `Frame`, `FrameStore`, `Layout`/`Placement` (= `schemas/layout.schema.json`), the sampler, brightness math, components and channel instances, `DirectLeds` (OpenRGB SDK colours) |
| `lightloom-render` | lib (+ test bin) | done | `NullRenderer` (solid, rainbow, test patterns), `CefRenderer` (spawns and supervises `lightloom-renderer --serve`, or `--face` at an LCD's size; engine-data sink), `FrameClock` |
| `lightloom-lcd` | lib | done (no LCD hardware tested) | `@SignalRGB/lcd` encoders (JPEG, PNG, BMP, RGBA, RGB, RGB565; NZXT formats a typed error), crops and slices, `LCD.initialize` validation, `LcdProvider` + `MockLcdProvider` (docs/host-api/lcd-lightloom.md) |
| `lightloom-renderer` | lib + bin | done; Chromium sandbox off | CEF windowless host (feature `cef`), shm ring writer/reader, `ipc` (tmpfs ring and unix-socket control on Linux), `window.engine` and control injection, URL filter, control pipe, snapshots and stats |
| `lightloom-plugin-host` | lib | done (rawusb, hybrid, serial mock-tested only) | QuickJS-NG runtime per plugin (64 MiB, deadlines), `js/host.js` compat host, natives (`device`, `children`, `bus`, `net`, `serial`), network services (`service`: DiscoveryService, mDNS, settings) and network device sessions, `@SignalRGB/*` / `@lightloom/*` resolver, catalogue and VID:PID index, device sessions, runtime parameters (`plugin_ui`), the `battery` feature, save to device and exit modes (`save`), the built-in LampArray session, SMBus policy and SPD check, `compat dry-run` |
| `lightloom-effects` | lib | done | `parse_effect_meta(html)` (= `schemas/effect-meta.schema.json`), control coercion, meters, JS initialisers |
| `lightloom-macros` | lib | done; untested with real input | macro runtime: QuickJS runtime and thread per Action/Input script, `system`/`signalrgb`/`keys`/`mouse`/`block`/`console` globals (`js/macro_host.js`), options, key names, event bus, opt-in input hooks, emergency stop (`docs/macros.md`) |
| `lightloom-inputs` | lib | done on Windows; Linux backends missing | `audio`: WASAPI loopback of an output device (cpal) + FFT analyzer (200 bands, level, width, density), `output_devices()`; `screen`: DXGI duplication of one display, 28x20 zones, 160x100 thumbnail, area/linear/colormean meters, `displays()` |
| `lightloom-api` | lib | done | axum routers: native `/v1` REST + WS + dashboard (`docs/api/openapi.yaml`), SignalRGB-compatible REST on 16038 and canvas events on 16034 (`docs/api/compat-openapi.yaml`), `actions` (`lightloom://` / `signalrgb://` URLs), `logs` (filtering and token redaction of `/v1/logs`), the OpenRGB SDK server (`openrgb`, opt-in, 6742), loopback / Origin / token guard, movable native listener |
| `lightloom-runtime` | lib | done | what CLI and daemon share: HID backend and device matching, SMBus buses and scans, hotplug, power events, conflict detection, sensors (system load, NvAPI, hwmon, LibreHardwareMonitor), LampArray ownership, the SignalRGB setup reader, data directories, system info, notifications, PawnIO / RAM lighting facts and the elevated helper run (`pawnio`) |
| `lightloom-nvapi` | lib | done | dynamic `nvapi64.dll` binding, one NvAPI worker thread, `NvapiBus: SmbusBus` per GPU, thermal sensors |
| `lightloom-adl` | lib | done; untested on hardware | dynamic `atiadlxx.dll` (legacy ADL) binding, one ADL worker thread, `AdlBus: SmbusBus` (kind `AmdGpu`) per AMD GPU |
| `lightloom-iobroker` | lib + bin | done (AMD PIIX4; Intel I801 untested on hardware) | LocalSystem service: PawnIO IOCTLs, pinned modules (`pins`, checked against `tools/package/fetch-pawnio-modules.ps1` by `tests/pawnio_pins.rs`), PIIX4 / I801 transfers, global SMBus mutex, static policy and rate limit, named pipe with DACL, audit log; proptest and cargo-fuzz harness |
| `lightloom-setup` | bin | done | the elevated helper of RAM lighting: `--pawnio-install` (verified download of the official `PawnIO_setup.exe` 2.2.0: size, SHA-256, Authenticode signer `namazso.eu`), `--smbus` (pinned modules and the broker, registers `LightloomIoBroker`), `--uninstall`, `--elevate`, `--pause`, `--plan` |
| `lightloom-tray` | lib + bin | done, untested live | `Shell_NotifyIconW` icon and menu over the loopback API, the `HKCU\...\Run` sign-in entry, one instance per session; prints "no tray icon on this platform yet" elsewhere |
| `lightloom-winres` | lib (build) | done | build-script helper: icon (`packaging/wix/lightloom.ico`) and version resource of the Windows executables (`winresource`, Windows hosts only) |
| `lightloom-daemon` | lib + bins `lightloomd`, `lightloomw` | done | single instance, token, port choice, engine thread (sessions, canvas, renderer, SQLite, playlists, actions, power), audio/screen/sensor feeds and the dashboard's input choice, battery alerts, the log history (`logs`: in-memory ring and `lightloomw`'s files), API serving, a shutdown that restores devices, Settings → PawnIO / RAM lighting |
| `lightloom-cli` | bin `lightloom` | done | `devices`, `light`, `rainbow`, `effect`, `effects`, `bench` (`--mock`), `plugins`, `compat dry-run` / `replay`, `url`, `action`, `udev generate`, `shm-dump` |

Test counts (`#[test]` and `#[tokio::test]` per crate, 2026-10-25): plugin-host 192, iobroker 79,
daemon 69, hal-windows 58, api 48, hal 42, runtime 39, hal-linux 39, inputs 37, renderer 36,
engine 28, cli 28, nvapi 24, adl 24, setup 16, protocol 16, tray 14, effects 13, render 11,
hid-desc 8, winres 2.

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
  examples by unit tests, the native API routes against `docs/api/openapi.yaml`
  (`crates/lightloom-api/tests/openapi.rs`) and the compatible API against
  `docs/api/compat-openapi.yaml` (`tests/compat_openapi.rs`). Keep both sides in sync.
* Never copy code from SignalRGB/WhirlwindFX files; facts about the plugin contract come from
  `docs/host-api/*.md`. OpenRGB code only from its GPL-2.0-or-later files, with credit.
