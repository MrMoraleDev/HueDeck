<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# HueDeck status against the plan (M0-M7)

Audit of 2026-10-22 against `develop` at `bfc3190`. The plan is the original M0-M7 plan
(milestones and acceptance criteria quoted from it in short form); the direction note of
2026-10-18 ("fully standalone, SignalRGB optional") overrides it where the two differ.

Legend:

- **Done**: in `develop`, with evidence (tests, bench notes).
- **Partial**: some of it is in `develop`; the gap is named.
- **Missing**: nothing in `develop` beyond a stub or a placeholder.
- **In progress (branch `feature/…`)**: another agent is working on it right now. These
  branches had no commits yet when this audit was written, so the label means "assigned",
  not "nearly done".

The agents in flight when this was written:

| Branch | Scope |
|---|---|
| `feature/api-port` | configurable native API port |
| `feature/openrgb-batch-2` | second batch of family plugins adapted from OpenRGB |
| `feature/network-lights` | network plugins (WLED, Hue, Govee, Nanoleaf, LIFX, Yeelight class): UDP/TCP sockets, discovery |
| `feature/battery` | battery level (`device.addFeature("battery")` and its UI) |
| `feature/bake` | bake to device (stored lighting, user-triggered) and the on-exit behaviour |
| `feature/installer` | Windows installer (WiX MSI) with the PawnIO option |

Test counts in `develop` (`#[test]` per crate): plugin-host 122, iobroker 52, hal-windows 43,
renderer 28, engine 26, hal 26, nvapi 19, cli 16, protocol 15, effects 12, daemon 11,
runtime 10, render 10, hid-desc 8, setup 5, api 2, inputs 1, hal-linux 0. Node tests:
`prelude/tests/*.test.mjs` (7 files), `plugins/*/tests/*.test.mjs`, `plugins/_testkit/*.test.mjs`.

## Summary

| Milestone | Status | The main gaps |
|---|---|---|
| M0 Bootstrap + spikes | **Done** | none worth noting; the crate README still describes M0 |
| M1 First light | **Done** | the "within 100 ms" first light is met on the native backend only (56-58 ms) |
| M2 Compatible plugin host | **Partial** | no rawusb/hybrid/serial sessions (`bulk_transfer`, `control_transfer`, `setProtocol` throw); the Sonix 320F keyboard is not driven (EVision V1); the physical hot-unplug test has not been run |
| M3 Effects pipeline | **Done** | the Chromium sandbox is off until releases are signed |
| M4 Dashboard, persistence, API, packaging | **Partial** | the MSI, winget and Authenticode signing; autostart and tray; the plugin sources manager; the logs page; compat API parts; the fresh-machine acceptance run |
| M5 Privileged bus | **Partial** | PIIX4 and I801 (I801 mock-tested only, no Intel board run); no LpcIO; no AMD GPU (ADLX); no sensors or fans; no fuzz target; no soak with HWiNFO/OpenRGB running; `huedeck-setup` does not install PawnIO or bind WinUSB |
| M6 Inputs, network, extras | **Missing** (network in progress) | audio, screen, meters, LCD faces, macros, actions, LampArray HAL backend, OpenRGB SDK server, runtime parameters UI |
| M7 Linux + 1.0 | **Partial** | hidraw backend, udev hotplug and `huedeck udev generate` (WP-L1, WP-L2) compile-checked for Linux and unit-tested, not yet run on Linux; nusb/serial/i2c/hwmon, CEF, packaging and cargo-dist missing |

## M0 Bootstrap and risk retirement: Done

| Item | Status | Evidence |
|---|---|---|
| Workspace, pins, `rust-toolchain.toml`, `deny.toml` | Done | `Cargo.toml` `[workspace.dependencies]`, `deny.toml` |
| CI on Windows and Ubuntu with the mock HAL | Done | `.github/workflows/ci.yml` job `rust` (matrix `windows-latest`, `ubuntu-latest`: fmt, clippy `-D warnings`, test) |
| `cargo deny check` | Done | `ci.yml` job `deny` (`EmbarkStudios/cargo-deny-action@v2`) |
| HAL traits + mock | Done | `crates/huedeck-hal/src/{hid,usb,serial,smbus,hotplug,sensors,capture,audio,lamparray}.rs`, `mock.rs` (`MockHid`, `MockSmbus`, `TransactionRecorder`), 26 tests |
| ADRs | Done | `docs/adr/0001`-`0010` (0006 Apache/MIT superseded by 0010 GPL-3.0-or-later) |
| `docs/host-api/` seeded from the spec bundle | Done | `docs/host-api/{hid-usb,smbus,components-channels,network-services,effects-engine,macros-system}.md` |
| Spike A: CEF windowless at 30 fps into shm, p99 < 40 ms, software compositing | Done | `crates/huedeck-renderer/README.md` (measurements), commit `2d3dd8f`; re-measured in `docs/hardware/effects-m3.md` |
| Spike B: rquickjs loads corpus plugins as ES modules | Done | commit `b5efc18`: 100 % parse, Initialize 98.5-99.0 % per corpus; `crates/huedeck-plugin-host/tests/loader.rs` |

## M1 First light, headless: Done

| Item | Status | Evidence |
|---|---|---|
| hal-windows HID backend, per-TLC endpoint tuples, USB-parent grouping | Done | `crates/huedeck-hal-windows/src/{hid,native,path,group}.rs` (hidapi and native backends), 43 tests |
| First-party `msi-mystic-light-7c91` | Done | `plugins/msi-mystic-light-7c91/` (fixtures and tests), `docs/hardware/msi-mystic-light-185.md` |
| Null renderer + hi-res frame clock | Done | `crates/huedeck-render/src/{null,clock}.rs` |
| CLI `devices`, `light`, `rainbow`, `bench` | Done | `crates/huedeck-cli/src/main.rs` (also `effect`, `effects`, `plugins`, `compat`) |
| Mock-HAL golden packet test | Done | `crates/huedeck-plugin-host/tests/session_msi.rs` |
| Accept: board lights from the CLI within 100 ms | Done (native backend) | `msi-mystic-light-185.md`: 56-58 ms native, 431 ms with the hidapi backend |
| Accept: 10-min soak, zero HID errors, p99 < 5 ms, RSS < 40 MB | Done | `msi-mystic-light-185.md`: 17 989 frames, 0 errors, p99 4.28 ms, peak RSS 10.6 MB |
| Accept: Ctrl-C restores the prior state | Done | `--restore-check` byte-identical for normal end, `--duration` and Ctrl-Break |
| Accept: tests pass on CI without hardware | Done | `ci.yml` `rust` job |

## M2 Compatible plugin host: Partial

| Item | Status | Evidence / gap |
|---|---|---|
| `huedeck compat dry-run <dir>` + API-gap report | Done | `crates/huedeck-cli/src/compat.rs`, `crates/huedeck-plugin-host/src/dryrun.rs`, commit `337cf1e` |
| `device.*` HID surface (write/read/feature/input reports, endpoints, info) | Done | `crates/huedeck-plugin-host/js/host.js` (device mode), `src/device.rs` |
| Channels, subdevices, components | Done | `host.js`, `tests/session_channels.rs` |
| `bus.*` (SMBus), Free/Fixed wrappers | Done (M5) | `src/smbus.rs`, `tests/session_smbus.rs` |
| `bulk_transfer`, `control_transfer`, `setProtocol` (rawusb, hybrid) | **Missing** | `host.js` lists them under `unimplemented("device", …)`; `catalog.rs` `accept()` refuses `rawusb`/`hybrid`/`serial`; no `UsbBackend` impl anywhere (`nusb` is pinned but unused, `hal-windows/Cargo.toml` `TODO(M2)`) |
| `@SignalRGB/serial` | **Missing** | stub only in dry-run (`host.js` `modules.serial`); no `SerialBackend` impl |
| `@SignalRGB/udp`, `tcp`, `XMLHttpRequest`, `addFeature("udp")` | Done (network plugins) | LAN-only (docs/host-api/network-huedeck.md); `addFeature("dtls")` and `https` missing (no TLS stack) |
| `addFeature("battery")` | In progress (`feature/battery`) | today: recording stub |
| `addFeature("keyboard"/"mouse")` (`sendEvent`, `sendHid`) | Missing | recording stubs; needs the M6 macro/input bus |
| Fan API (`createFanControl`, `setRPM`, …) | Missing | throws in device sessions; `fanControlDisabled()` answers `true` |
| API_GAP log-and-throw | Done | `prelude/HostApiMissing.js`, `host.js` `gap()`, `prelude/tests/HostApiMissing.test.mjs` |
| `huedeck-hid-desc` | Done | `crates/huedeck-hid-desc/src/lib.rs`, 8 tests |
| `set_endpoint` / `getHidEndpoints` | Done | `host.js`, `src/device.rs` |
| Hotplug via `CM_Register_Notification` | Done | `crates/huedeck-hal-windows/src/hotplug.rs`, `crates/huedeck-runtime/src/hotplug.rs`, `tests/manager.rs` |
| Plugin index cache | Done | `crates/huedeck-plugin-host/src/catalog.rs` (cache keyed by path, size, mtime) |
| Watchdog, 64 MiB heap cap | Done | `src/runtime.rs`, `tests/session_robustness.rs`, commit `13514d4` |
| Conformance with HID record/replay | Partial | record/replay done (`crates/huedeck-hal/src/trace.rs`, `huedeck compat replay`, `imported-plugins-m2.md`); there is no `tests/conformance/` corpus runner, and the self-hosted hardware job `conformance-selfhosted.yml` named in `ci.yml` does not exist |
| Accept: 100 % of the corpus parses, >= 95 % Initialize | Done | commit `b5efc18`: 100 % parse; 98.5 % (CDN), 98.7 % (bundled), 99.0 % (addons) |
| Accept: unmodified plugins drive the DeathAdder, Arctis 5 | Done | `docs/hardware/imported-plugins-m2.md` |
| Accept: Sonix 320F keyboard (multi-collection Validate) | **Missing** | no SignalRGB plugin exists for `320F:5064`; HueDeck's `plugins/evision-keyboard` covers V2 only and excludes V1 for flash wear (see `docs/research/evision-v1.md`) |
| Accept: hot-unplug/replug recovers in < 2 s | Partial | mock test `tests/manager.rs`; the physical unplug on the reference machine is still to do (`imported-plugins-m2.md`, "Hotplug") |

## M3 Effects pipeline: Done

| Item | Status | Evidence / gap |
|---|---|---|
| `huedeck-renderer` (CEF OSR), shm ring, control pipe | Done | `crates/huedeck-renderer/src/{cef_host,ring,control,sink}.rs`, 28 tests |
| Supervision and restart holding the last frame | Done | `crates/huedeck-render/src/cef.rs`, `tests/cef_client.rs`; 1522 ms recovery (`effects-m3.md`) |
| `<meta>` parser, control model, load sequence | Done | `crates/huedeck-effects/src/parse.rs`, `crates/huedeck-renderer/src/inject.rs`, `tests/starter_effects.rs` |
| `engine.*` via JSON push, `onCanvasTapped`, `onCanvasApiEvent` | Done | `inject.rs`, `/v1/canvas/event`, `/v1/canvas/tap`, compat `:16034` |
| Layout + sampling, brightness chain | Done | `crates/huedeck-engine/src/{layout,sampler}.rs` |
| 15-fps downshift guard, `huedeck bench` | Done | `crates/huedeck-render/src/cef.rs` (`DOWNSHIFT_WINDOW`), `crates/huedeck-cli/src/run.rs` |
| Accept: 20 lightscripts at 30 fps | Done | `effects-m3.md` (7 SignalRGB, 13 HueDeck CC0) |
| Accept: PSNR >= 40 dB | Done | 40/40 bit-identical |
| Accept: canvas-to-wire p50 <= 10, p99 <= 25 ms, 4 devices | Done | p50 0.9-3.8, p99 2.2-4.3 ms |
| Accept: renderer RSS <= 300 MB | Done (one exception) | 284-291 MB; Aurora in software compositing 411 MB |
| Accept: crash recovered < 3 s | Done | 1522 ms |
| Chromium sandbox | Missing | `crates/huedeck-renderer/README.md` "Chromium sandbox": CEF 154 needs a signed bootstrap executable; waits for signing |

## M4 Dashboard, persistence, local API, packaging: Partial

| Item | Status | Evidence / gap |
|---|---|---|
| SQLite + migrations | Done | `crates/huedeck-daemon/src/store.rs` (`rusqlite` + `rusqlite_migration`; tables kv, devices, device_parameters, layouts, placements, channel_components, effect_params, presets, playlists); export/import round-trip test |
| Native API 16138 (REST + WS, token, Host check) | Done | `crates/huedeck-api/src/server.rs` (`/v1/*`, `/v1/events`, `/v1/preview`); `docs/hardware/daemon-m4.md` |
| Configurable API port | In progress (`feature/api-port`) | today: fixed 16138 |
| Compat REST 16038 | Done | `compat_router()`: every route of SignalRGB's local API (app, new, partners, lighting, global_brightness, enabled, effects, effect, image, apply, presets, next, previous, shuffle, loadEffect, playlists, playlist, play, scenes/current_layout, scenes/layouts); envelope contract test for every route; `docs/integrations/signalrgb-api.md` |
| Compat canvas events 16034 | Done | `server.rs` `compat_canvas_event`; listeners wait while SignalRGB owns the ports |
| Dashboard: devices, layout editor (drag, scale, rotate, flip), effects + controls + presets, channels/components, settings | Done | `ui/src/components/{Devices,Layout,Effects,Preview,Settings,ControlInput}.svelte` |
| Dashboard: logs page | Missing | no log view in `ui/src` |
| Import wizard | Partial | SignalRGB layout + channel arrays (`crates/huedeck-runtime/src/signalrgb.rs`, `/v1/import/signalrgb`); plugins, effects and components read in place behind one setting. Not imported: effect parameters and presets (`HKCU …\effects\<id>`), the selected effect, global/layout brightness, per-device enable and brightness; imports are not marked "unsigned" |
| Own export/import of the whole setup | Done | `/v1/export`, `/v1/import`, `store.rs` test `export_import_round_trips_byte_for_byte` |
| Plugin sources manager (pre-listed community repos, licence acknowledgement, hash pinning) | Missing | `schemas/plugin-source.schema.json` exists; no code. Lower priority under the standalone direction |
| Tray shell, autostart | Partial | autostart: the MSI's sign-in option (`HKCU\...\Run` → `huedeckw.exe`, hidden daemon, logs in `%LOCALAPPDATA%\HueDeck\logs`); no tray |
| Effect playlists / auto-next timer | Done | `crates/huedeck-daemon/src/engine/playlists.rs`, `store.rs` table `playlists`, `/v1/playlists*`, `/v1/lighting/timer`, `ui/src/components/Playlists.svelte`; test `tests/playlists.rs` |
| Suspend/resume (`Shutdown(true)` and re-`Initialize`) | Missing | `LifecycleState::Suspending/Resuming` exist in `crates/huedeck-daemon/src/lifecycle.rs`, nothing listens for power events |
| On-exit behaviour, bake to device | In progress (`feature/bake`) | today: `Shutdown(false)` restore on exit |
| Portable build | Done | `tools/package/stage-windows.ps1`, `release.yml` |
| WiX MSI, winget | Done, unverified | `packaging/wix/` (per-user WiX v5 MSI, options page, `build.ps1`), `packaging/winget/` templates + `render.ps1`; built in `release.yml`, WiX check in `ci.yml`; never installed on a clean VM yet |
| Authenticode (SignPath) | Missing | `release.yml` "SIGNING" placeholder |
| Accept: fresh Win11 → MSI → lit devices with no terminal | Missing | needs the MSI |
| Accept: component JSON on JRAINBOW1 lights an ARGB strip | Done | `imported-plugins-m2.md` (Default Strip on JRainbow 1/2), `daemon-m4.md` |
| Accept: a tool hitting `GET /api/v1/lighting` on 16038 works | Done | `daemon-m4.md`, `server.rs` compat router |
| Accept: export/import round-trips | Done | store test above |

## M5 Privileged bus via broker: Partial

| Item | Status | Evidence / gap |
|---|---|---|
| `huedeck-iobroker` (LocalSystem service), op enum, mutex, allowlists, rate limit, dry-run, audit | Done | `crates/huedeck-iobroker/src/{server,policy,mutex,pipe,pawnio}.rs`, `crates/huedeck-protocol/src/broker.rs`, 52 + 15 tests |
| PawnIO SmbusPIIX4, pinned | Done | `crates/huedeck-iobroker/src/{piix4,pins}.rs` (PawnIO.Modules 0.2.11 SHA-256) |
| PawnIO SmbusI801 (Intel) | Done (no hardware run) | `crates/huedeck-iobroker/src/{i801,chipset,pins,boot}.rs`: pinned, tried after PIIX4, fake + fuzz tested; needs an Intel board |
| PawnIO LpcIO (SuperIO, NCT6687D) | Missing | no code |
| `huedeck-setup` one-shot elevated installer | Partial | `crates/huedeck-setup/src/`: `--pawnio-install` (verified download and silent install of PawnIO 2.2.0), pinned modules + broker, registers the service; the installer option and Settings → PawnIO / RAM lighting run it behind one UAC prompt. No WinUSB binding |
| `bus.*` mapping, capability matrix | Done | `crates/huedeck-plugin-host/src/smbus.rs`, `plugins/README.md` "SMBus plugins" |
| `huedeck-nvapi` GPU I2C | Done | `crates/huedeck-nvapi/`, 19 tests; `plugins/msi-gpu`, `docs/hardware/msi-gpu.md` |
| AMD GPU I2C (ADLX/ADL) | Missing | see `docs/research/drivers.md` |
| Sensors (MSR, NvAPI thermal, LibreHardwareMonitor JSON) | Missing | `huedeck-hal/src/sensors.rs` trait only; `engine.getSensorValue` reads pushed values (`inject.rs`), nothing pushes them |
| Fan curves + SuperIO | Missing | stretch goal in the plan |
| Accept: Crucial Ballistix via SmbusPIIX4, unmodified imported plugin | Done with a first-party plugin | `docs/hardware/crucial-ballistix.md`: both modules tested 2026-10-22 with HueDeck's `plugins/crucial-ballistix` (the standalone direction replaces the imported plugin) |
| Accept: RTX 3060 via NvAPI I2C | Done | commit `9311108`, `docs/hardware/msi-gpu.md` |
| Accept: 30-min soak with HWiNFO/OpenRGB running, no bus errors | Missing | not run |
| Accept: broker fuzz, 1M iterations | Missing | no fuzz target in the tree |
| Accept: no UAC after setup | Done in practice | the broker is a service; daemon runs unelevated (`crucial-ballistix.md` procedure). Not re-checked on a fresh machine |

## M6 Inputs, network, extras: Missing (network in progress)

| Item | Status | Evidence / gap |
|---|---|---|
| Audio FFT (`engine.audio`) | Missing | `crates/huedeck-inputs/src/lib.rs` defines `AudioSnapshot` only; renderer side ready (`inject.rs` fills typed arrays in place) |
| Screen zones (`engine.zone`) | Missing | `ZoneSnapshot` shape only; renderer side ready |
| Area/linear meters (`engine.vision`) | Missing | `huedeck-effects` parses `<meta meter>` (`model.rs`), nothing computes them |
| Network plugins (udp/tcp, DiscoveryService, mDNS, `service`/`controller`/`discovery`, XHR, settings, dashboard section) | Done, DTLS missing | first-party WLED, DDP, E1.31, Philips Hue (CLIP v1), Nanoleaf, Govee, LIFX, Yeelight; all untested on hardware |
| LCD faces (`@SignalRGB/lcd`, second browser per device) | Missing | dry-run stub only |
| Macros (Inputs/Actions), key/mouse hooks, `system.*`, `signalrgb.*`, `keys` | Missing | spec in `docs/host-api/macros-system.md`; no code |
| Action router (`huedeck://`, `signalrgb://`) | Missing | no URL handler |
| LampArray HAL backend (WinRT `Windows.Devices.Lights`) | Done, not run on hardware | `crates/huedeck-hal-windows/src/lamparray.rs` (DeviceWatcher, `IsAvailable`/`AvailabilityChanged`, identity check); built-in session `crates/huedeck-plugin-host/src/lamparray.rs`; ownership (a plugin wins: interface path, container id, VID:PID) `crates/huedeck-runtime/src/lamparray.rs`; identity package recipe `packaging/msix-identity/` (built, never registered). Design: `docs/windows-dynamic-lighting.md`. `plugins/hid-lamparray` still drives HID LampArrays first (tested on the DeathAdder's LampArray collection) |
| OpenRGB SDK server :6742 | Missing | `OPENRGB_SDK_PORT` constant in `crates/huedeck-api/src/lib.rs` only |
| Dynamic ControllableParameters UI | Partial | static `ControllableParameters` render in `Devices.svelte` and persist (`device_parameters` table); runtime `device.addProperty/removeProperty` changes stay inside the plugin (`host.js`) and never reach the API or UI; `DeviceMessages`/`addMessage` are not shown |
| Accept: audio-reactive + screen effects track input < 100 ms | Missing | |
| Accept: Govee/Yeelight/WLED-class plugin drives a LAN device | Mock and loopback tests only | needs a real device |
| Accept: OpenRGB SDK client sets a colour on the MSI board | Missing | |
| Accept: a DL-certified keyboard lights with no plugin | Partial | the generic `hid-lamparray` plugin is that path for HID LampArray devices, the WinRT backend for LampArrays no plugin owns (needs the identity package); not tested on a keyboard |

## M7 Linux backend + 1.0: Missing

| Item | Status | Evidence / gap |
|---|---|---|
| hal-linux hidraw + hid-desc | Done, untested on Linux | `crates/huedeck-hal-linux/src/{sysfs,tlc,hidraw,sys}.rs`: sysfs enumeration, TLC split, raw hidraw ioctls; `huedeck devices` and hardware runs wired through `huedeck-runtime`. Checked with `cargo clippy --target x86_64-unknown-linux-gnu` only |
| udev hotplug | Done, untested on Linux | `crates/huedeck-hal-linux/src/{hotplug,uevent}.rs`: netlink udev monitor, pure Rust |
| i2c-dev SMBus, hwmon sensors | Done, untested on Linux | `crates/huedeck-hal-linux/src/{i2c,hwmon}.rs`: adapters by sysfs name (PIIX4 ports 0/1, i801, NVIDIA port 1, AMDGPU OEM), Windows policy plus the broker write allowlist on DIMM buses, SPD from the kernel `eeprom` file or a host-internal EE1004 page select; wired in `huedeck-runtime` (`smbus.rs`, `sensors.rs`); udev i2c rules per adapter name. Checked with `cargo clippy --target x86_64-unknown-linux-gnu` and fake-sysfs/fake-device tests only |
| nusb, serialport, hwmon pwm (fan control) | Missing | WP-L3, WP-SENSORS-FANS |
| `tools/gen-udev` | Done | `huedeck udev generate` (`crates/huedeck-cli/src/udev.rs`); generated `packaging/udev/70-huedeck.rules` with a drift test; `packaging/sysusers.d/huedeck.conf` |
| CEF on Linux (X11/Wayland, headless CI) | Missing | `huedeck-renderer` builds with `cef` on Windows only (`ci.yml` `renderer-cef`) |
| PipeWire audio + screen capture | Missing | |
| `systemd --user` unit | Partial | `packaging/systemd/huedeckd.service` exists, untested |
| AppImage, Flatpak | Missing | placeholders in `packaging/README.md` |
| `docs/linux-porting.md` | Missing | design input in `docs/research/m7-linux-backend.md` |
| Accept: same plugins light the same hardware on Ubuntu LTS | Missing | |
| Accept: identical `Validate` tuples on both OSes (fixture) | Partial | `crates/huedeck-hal-linux/src/fixtures.rs`: one descriptor through the Windows path logic and the Linux split; descriptors are synthetic, a captured Linux `report_descriptor` per reference device is still to record |
| Accept: RAM via `/dev/i2c-*` without a helper | Partial | code done (WP-L4); needs a Linux run on the reference machine |
| Accept: conformance suite on both OSes | Missing | there is no conformance suite yet (see M2) |
| Accept: reproducible release via cargo-dist | Missing | `release.yml` is a hand-written placeholder (cargo-auditable + CycloneDX SBOM, unsigned) |

## Planned components outside the milestones

| Component | Status | Evidence / gap |
|---|---|---|
| Crates (17 planned) | 16 real, 1 stub | all exist; `huedeck-hal-linux` is a stub, `huedeck-inputs` is shapes only; `huedeck-runtime` was added (shared CLI/daemon plumbing). `crates/README.md` refreshed (processes, graph, status per crate) |
| First-party plugins | Done for the plan's list, except Sonix | `msi-mystic-light-7c91`, `razer` (DeathAdder V2 Pro), `nzxt-hue2` (Kraken X3), `steelseries-arctis-5`, `crucial-ballistix`, `hid-lamparray`, plus `msi-gpu`, `sony-gamepad` and the OpenRGB batch 1 families (`razer*`, `steelseries-*`, `msi-mystic-light*`, `evision-keyboard` V2). Batch 2: in progress (`feature/openrgb-batch-2`) |
| Starter effects (solid, rainbow-wave, audio-bars, screen-ambience, test-pattern) | Done | `effects/` has 13 CC0 effects |
| `components/` own component library | Missing | no `components/` directory; the daemon looks for one (`engine.rs` `components_dir`), and today only the SignalRGB and user component dirs fill the library |
| `schemas/` (manifest, ControllableParameters, component, layout, effect meta) | Done | `schemas/*.schema.json`, `tests/schemas/validate_corpus.py`, `ci.yml` job `schemas` |
| OpenAPI for the native API | Done | `docs/api/openapi.yaml` (OpenAPI 3.1); `crates/huedeck-api/tests/openapi.rs` checks it against the router and `NATIVE_ROUTES`. The compat API is not described |
| `sdk/huedeck.d.ts` | Done | `sdk/huedeck.d.ts`, members tagged `@signalrgb` / `@huedeck` / `@unimplemented` / `@stub`; `tsc -p sdk` in CI |
| `sdk/` Node simulator (mock HAL recorder) | Done | `sdk/simulator` (over `plugins/_testkit`): HID and SMBus lifecycles with a solid colour, prints the traffic; `huedeck bench --mock` runs the real host against a scripted device |
| `sdk/` plugin lint | Done | `sdk/lint` (manifest schema, exports, exports against the manifest, forbidden globals); `plugins/_testkit/manifests.test.mjs` runs it over the first-party plugins |
| `tests/conformance/` (corpus runner, HID fixtures, PSNR harness) | Partial | dry-run harness lives in the CLI; HID fixtures live per plugin; PSNR harness is `tools/effects-bench/psnr.py`. No `tests/conformance/` and no self-hosted workflow |
| `tests/integration/` | Partial | crate integration tests (`crates/*/tests/`); no top-level directory |
| `tools/gen-udev` | Done | `huedeck udev generate` |
| `tools/pawnio-hash-pin` | Missing (covered) | pins are compiled into `crates/huedeck-iobroker/src/pins.rs`; no tool to recompute them |
| `tools/shm-dump` | Missing | |
| `tools/effects-bench`, `tools/device-probe`, `tools/package` | Done | `tools/README.md` |
| `prelude/` | Done | `prelude/*.js`, `prelude/modules/*.js`, 7 test files |
| `docs/security.md`, `docs/linux-porting.md` | Partial | `docs/security.md` done (threat model, in place vs planned); `docs/linux-porting.md` missing |
| Packaging: `wix/`, `winget/` | Done, unverified | see packaging/README.md |
| Packaging: `systemd/`, `udev/` | Partial | files present, untested |
| Packaging: `appimage/`, `flatpak/` | Missing | |
| CI `rust` (win + ubuntu), `renderer-cef`, `deny`, `node`, `schemas` | Done | `ci.yml` |
| CI `conformance-selfhosted.yml` | Missing | referenced in `ci.yml`'s header only |
| CI `release.yml` via cargo-dist, signing, SBOM | Partial | SBOM + cargo-auditable done; cargo-dist and signing missing |
| Dependabot | Done | `.github/dependabot.yml`; three open PRs reviewed in `docs/research/dependabot-2026-10.md` |

## Remaining work packages

Each package is meant for one agent working alone. "Touches" lists where its changes
land, so the coordinator can avoid running two packages with the same hot files in
parallel. Sizes: **S** about a day, **M** two to four days, **L** a week or more, for
one agent with the mock HAL.

**Shared hot files.** Most packages need a few lines in one of these. Run at most two
packages that touch the same one in parallel, and merge the smaller one first:

- `crates/huedeck-plugin-host/js/host.js`
- `crates/huedeck-plugin-host/src/{session,device,catalog}.rs`
- `crates/huedeck-daemon/src/engine.rs`
- `crates/huedeck-api/src/server.rs`
- `ui/src/components/*.svelte`
- root `Cargo.toml` `[workspace.dependencies]`

### Priority 1: unblock device coverage on Windows

0. **WP-CRT: static C runtime. Size S. Do this first.**
   - Every shipped exe imports `VCRUNTIME140.dll`, which a clean Windows does not have and the portable build does not ship (see `docs/research/drivers.md`). Without it the daemon does not start, and neither does the broker service.
   - Add a repo `.cargo/config.toml` with `-C target-feature=+crt-static` for `x86_64-pc-windows-msvc`. Add a packaging check that fails on any PE import outside an allow-list.
   - Touches: `.cargo/config.toml`, `tools/package/`, `release.yml`. Coordinate with `feature/installer`.

1. **WP-USB: raw USB sessions (`rawusb`, `hybrid`, `bulk_transfer`, `control_transfer`, `setProtocol`). Size L.**
   - Touches: `huedeck-hal/src/usb.rs` (trait as needed) and `mock.rs`; new `huedeck-hal-windows/src/usb.rs` on `nusb` (WinUSB); `huedeck-plugin-host/src/{device,catalog,session}.rs`; `js/host.js` (remove the three names from `unimplemented`); `huedeck-runtime/src/hardware.rs` (enumeration); tests in `crates/huedeck-plugin-host/tests/`.
   - Prerequisite reading: `docs/research/drivers.md` (which devices get WinUSB without an INF).
   - Unblocks the OpenRGB rawusb families and SignalRGB's Lian Li, Aqua Computer and Thermalright plugins.
2. **WP-SERIAL: `@SignalRGB/serial` and `Type() == "serial"`. Size M.**
   - Touches: new `huedeck-hal-windows/src/serial.rs` (`serialport`); the plugin host native module (`src/device.rs` or a new `src/serial.rs`); `catalog.rs` `accept()`; `host.js` serial module; tests.
   - Conflicts with WP-USB on `catalog.rs`/`host.js`: run it after WP-USB, or give it only the module files.
3. **WP-I801: Intel chipset SMBus (PawnIO `SmbusI801`). Size M.** Done on `feature/intel-smbus` (mock/fake/fuzz only; hardware acceptance on an Intel board pending).
   - Touches: `huedeck-iobroker/src/{pins,boot,detect}.rs`, a new `i801.rs` beside `piix4.rs`, `huedeck-protocol/src/broker.rs` (bus kind, if needed), `huedeck-setup` (module list).
   - Mock and fake tests only; it needs an Intel board for hardware acceptance.
4. **WP-AMDGPU: AMD GPU I2C via ADL/ADLX. Size M.**
   - New crate `huedeck-adl` mirroring `huedeck-nvapi` (dynamic `atiadlxx.dll`, worker thread, `SmbusBus` impl, fake), wired in `huedeck-runtime/src/smbus.rs`.
   - See `docs/research/drivers.md`.
5. **WP-SUSPEND: system sleep and resume. Size S.**
   - Touches: `huedeck-hal-windows` (`PowerRegisterSuspendResumeNotification` or a hidden window), `huedeck-daemon/src/{lifecycle,engine}.rs`.
   - `Shutdown(true)` before sleep, re-`Initialize` on resume.
   - Run it after `feature/bake` merges: same exit path.
6. **WP-EVISION-V1: Sonix `320F:5064`, stored-settings-only mode. Size M.**
   - Blocked on the user's USB capture: see the procedure in `docs/research/evision-v1.md`.
   - Touches: `plugins/evision-keyboard/` only (new `evision-v1.js`), plus the host's "stored settings only" capability from `feature/bake`.

### Priority 2: M6 features

7. **WP-AUDIO: audio FFT into `engine.audio`. Size M.**
   - Touches: `huedeck-inputs` (cpal WASAPI loopback + realfft → 200 bins, level, density, width per `docs/research/m6-inputs-network.md` §1); `huedeck-daemon/src/engine.rs` (push to the renderer through the existing `engine` command).
   - Accept: Audio Bars follows music, with under 100 ms input-to-paint.
   - Status: implemented on `feature/audio-input` (`docs/audio.md`); latency still to be measured on the user's PC.
8. **WP-SCREEN: screen zones into `engine.zone`. Size M.**
   - Touches: `huedeck-inputs` (DXGI duplication, `windows-capture` or `xcap` per §2.4; 28x20 HSL + 160x100 RGBA at 10-60 fps; display picker); the daemon push; one settings field in `Settings.svelte`.
   - Same crate as WP-AUDIO: run them one after the other, or split the module files.
   - Status: implemented on `feature/screen-input` (`docs/screen.md`); DXGI duplication checked on the dev PC; ambience latency still to be measured. No dashboard picker (`[screen] display` in `config.toml`).
9. **WP-METERS: area/linear meters (`engine.vision`). Size S.**
   - Needs WP-SCREEN's frames.
   - Touches: `huedeck-inputs` (meter evaluation from the parsed `<meta meter>`), the daemon push.
   - OCR stays out of scope.
   - Status: implemented on `feature/screen-input` (area, linear, colormean; `docs/screen.md`).
10. **WP-SDK-SERVER: OpenRGB SDK server on :6742. Size M.**
    - New module `huedeck-api/src/openrgb.rs`, clean-room from `docs/research/m6-windows-integrations.md` §2.
    - Maps sessions to RGBControllers through a new daemon request (`engine.rs`: "set LEDs of device X, direct").
    - Opt-in setting; binds to 127.0.0.1.
    - Accept: the OpenRGB client sets a colour on the MSI board.
11. **WP-LAMPARRAY: WinRT LampArray HAL backend. Size L. Done (feature/lamparray), not run on hardware; see `docs/windows-dynamic-lighting.md`.**
    - Touches: new `huedeck-hal-windows/src/lamparray.rs` (`Windows.Devices.Lights`); a pseudo-session in the plugin host or the daemon that samples the layout like a plugin; the identity package (sparse MSIX) recipe in `packaging/` per §1.4.
    - Rule: a plugin wins over the backend on the same device.
    - Overlaps `plugins/hid-lamparray`: decide which owns which device first.
12. **WP-PARAMS: runtime ControllableParameters and device messages. Size S.**
    - Touches: `host.js` (`addProperty`/`removeProperty`/`addMessage` → native calls), `huedeck-plugin-host/src/session.rs` (an events channel), `huedeck-daemon/src/engine.rs` (device view), `huedeck-api/src/model.rs`, `ui/src/components/Devices.svelte`.
    - Also the `keycode` type and `filter` in `ControlInput.svelte`.
    - **Done** on `feature/runtime-params`: `huedeck-plugin-host/src/plugin_ui.rs` keeps a device's parameters and messages (seeded from `ControllableParameters()` / `DeviceMessages()`); the device-mode `device.addProperty` / `removeProperty` / `addMessage` / `removeMessage` / `notify` / `denotify` change them through natives (refusals logged with the exe's messages, re-adding an identical entry is no change). The session publishes them (`SessionHandle::plugin_ui`) and announces changes on its events channel (`SessionHandle::events`); the engine refreshes the snapshot on an event. `DeviceView.messages` is new; values set from the dashboard are coerced to the control type (`coerce_param_value`), as are the plugin's numeric and boolean strings (`default`, `min`/`max`/`step`/`order`, `live: "false"`). Dashboard: parameters sorted by `order` and grouped by `group`, `live` sliders send while dragging (throttled), `filter` validates textfields, `keycode` records a key combination, messages shown on the device card.
13. **WP-MACROS: macros and actions. Size L. Split it in two.**
    - (a) Action router: `huedeck://` URL scheme and `system.action` endpoints for app, view, effect, layout and device, in the daemon and API.
    - (b) Macro runtime: QuickJS runtime per macro script, Inputs/Actions, `signalrgb.*`, `keys`/`mouse` via SendInput, opt-in low-level hooks.
    - (b) touches a new crate `huedeck-macros` plus the daemon. It also connects the `keyboard`/`mouse` `sendEvent` feature globals in `host.js`.
14. **WP-LCD: LCD faces. Size L.**
    - Touches: `huedeck-renderer` (a second browser per LCD device, its own shm ring); `huedeck-plugin-host` native `lcd` module (`getFrame` in JPEG, PNG, RGB565 and the other formats); the daemon (face selection).
    - Needs WP-USB first: most LCD coolers are rawusb.
15. **WP-COMPAT-REST: complete the compat REST on 16038. Size S.**
    - Touches: `huedeck-api/src/server.rs` `compat_router` only.
    - Adds `lighting/shuffle`, `lighting/loadEffect`, `effects/{id}/image`, `playlists/{id}/play`, `app` fields, `partners`/`new` as documented empty answers, and contract tests for the envelope.
    - Coordinate with `feature/api-port`, which also touches `server.rs`.
    - **Done** on `feature/compat-rest-playlists` (with WP-PLAYLISTS).
16. **WP-IMPORT: import wizard completeness. Size S.**
    - Touches: `huedeck-runtime/src/signalrgb.rs`, `huedeck-daemon/src/store.rs` (import mapping), the `/v1/import/signalrgb` reply, `Settings.svelte`.
    - Imports effect parameters and presets, the selected effect, global and layout brightness, and per-device enable and brightness, from HKCU.
    - Shows a preview (what will be imported) and marks imported plugins and effects as third-party.
17. **WP-PLAYLISTS: effect playlists and the auto-next timer. Size S.**
    - Touches: `store.rs` (a table), `engine.rs`, the API, `Effects.svelte`.
    - Also fills compat `playlists`.
    - **Done** on `feature/compat-rest-playlists`: ordered entries with a preset, values and a duration each, shuffle, the auto-next timer (off, 300 s by default), next/previous within the playlist, resume after restart, dashboard panel.

### Priority 3: M5 remainder and hardening

18. **WP-SENSORS-FANS: sensors and fan control. Size L.**
    - Sensors: NvAPI thermals in `huedeck-nvapi`, CPU through the broker or LibreHardwareMonitor JSON; pushed as `engine.getSensorValue`.
    - The plugin fan API (`createFanControl`, …) with curves.
    - LpcIO/NCT6687D through PawnIO LpcIO in the broker.
    - Opt-in only. Touches `huedeck-iobroker`, `huedeck-nvapi`, `huedeck-inputs`, `host.js`, the daemon and a UI page.
19. **WP-FUZZ: broker fuzz target. Size S.**
    - `crates/huedeck-iobroker/fuzz/` (cargo-fuzz) or a proptest of `BrokerRequest` decoding and `server` handling with the fakes.
    - Accept: 1M iterations, no panic.
    - **Done** on `feature/broker-fuzz`: `crates/huedeck-iobroker/src/fuzz.rs` (proptest properties plus a deterministic driver, 5,000 inputs per `cargo test`), cargo-fuzz targets `frame` and `session` in `crates/huedeck-iobroker/fuzz/` (see its README). 1M-input acceptance run in release: 263 s, 9.97M requests, no panic, no invariant broken. Found and fixed a stack overflow on nested `Batch` frames (`one_level` in `huedeck-protocol`).
20. **WP-CONFORMANCE: conformance suite. Size M.**
    - `tests/conformance/` runner: dry-run over a user-supplied corpus with thresholds, replay of every recorded trace in `plugins/*/fixtures`, and the PSNR harness hook.
    - `.github/workflows/conformance-selfhosted.yml` for the reference machine.
21. **WP-DOCS-SDK: SDK and docs. Size M.**
    - `sdk/huedeck.d.ts` from `docs/host-api/` and `host.js`; `sdk/lint` (manifest + exports checks lifted from `_testkit`); the OpenAPI file for `/v1`.
    - `docs/security.md`; refresh `crates/README.md`.
    - Docs only, no conflicts.
22. **WP-COMPONENTS: own component library. Size S.**
    - `components/*.json` (CC0, from public product data: common fan and strip LED counts and shapes), validated by `schemas/component.schema.json`, shipped by `stage-windows.ps1`.
23. **WP-TRAY: tray and autostart. Size M.**
    - Per-user Run-key autostart (a setting) and a minimal tray icon (Tauri or `tray-icon`) that opens the dashboard and stops the daemon.
    - Coordinate with `feature/installer`.
24. **WP-SIGN: signing and the Chromium sandbox. Size M; needs a SignPath OSS account (user action).**
    - Touches: `release.yml` signing steps; turning the renderer sandbox on with the signed bootstrap (`huedeck-renderer` README).
25. **WP-SOURCES: plugin sources manager. Size M. Optional under the standalone direction.**
    - Git or HTTPS sources per `schemas/plugin-source.schema.json`, licence acknowledgement, hash pinning, update only on user action.
26. **WP-SOAK: hardware acceptance runs (user present). Size S each.**
    - Physical hot-unplug under 2 s (M2).
    - 30-min SMBus soak with HWiNFO and OpenRGB running (M5).
    - Fresh-machine MSI install (M4, after `feature/installer`).

### Priority 4: M7 Linux

These can start in parallel with Windows work: they touch `huedeck-hal-linux` and
`packaging/`, which nobody else edits.

27. **WP-L1: hidraw backend. Size M.**
    - `huedeck-hal-linux`: hidapi `linux-native` or direct hidraw ioctls, with `huedeck-hid-desc` splitting collections so `Validate` tuples match Windows.
    - A shared fixture test: same descriptor gives the same tuples on both OSes.
    - Enable `huedeck devices` on Linux (`huedeck-cli/src/devices.rs`).
28. **WP-L2: udev hotplug and `tools/gen-udev`. Size S.**
    - `huedeck-hal-linux/src/hotplug.rs` (udev monitor).
    - `tools/gen-udev` (a Rust bin or `huedeck udev` subcommand) emitting `70-huedeck.rules` from the manifests; `packaging/udev/`.
29. **WP-L3: nusb and serialport on Linux. Size S, after WP-USB and WP-SERIAL.**
    - `huedeck-hal-linux/src/{usb,serial}.rs` behind the traits WP-USB and WP-SERIAL define.
30. **WP-L4: `/dev/i2c-*` SMBus and hwmon. Size M.**
    - `huedeck-hal-linux/src/{i2c,hwmon}.rs` (`i2cdev`), adapter discovery by name (PIIX4, NVIDIA, AMDGPU DM aux) per `docs/research/m7-linux-backend.md`.
    - Wired in `huedeck-runtime/src/smbus.rs` under `cfg(unix)`.
    - **Done** on `feature/linux-i2c`, untested on Linux hardware. Pure ioctls through `libc` in `sys.rs` (no `i2cdev` crate: it lacks `I2C_FUNCS`). GPU I2C on Linux also comes from `/dev/i2c-*` (no NvAPI). AMDGPU DDC and DP AUX buses are left out. hwmon temperatures and fans are a `SensorBackend` (`huedeck_runtime::sensors::backends()`), not yet polled: WP-SENSORS-FANS adds the facade.
31. **WP-L5: CEF on Linux. Size L.**
    - `huedeck-renderer` builds and runs on Linux (OSR needs no X11 window; headless CI with `--ozone-platform=headless` or Xvfb); shm via `memfd`/`/dev/shm`; the control pipe as a unix socket (`huedeck-protocol`, `huedeck-render`).
    - Add a `renderer-cef` Ubuntu CI job.
32. **WP-L6: PipeWire audio and screen capture. Size M, after WP-AUDIO and WP-SCREEN.**
    - `huedeck-inputs` Linux backends: cpal ALSA/PipeWire monitor; portal ScreenCast + PipeWire for zones.
33. **WP-L7: Linux packaging and release. Size M.**
    - systemd user unit tested; AppImage (linuxdeploy) and Flatpak manifests in `packaging/`.
    - `cargo dist init` replacing the hand-written `release.yml` (keep cargo-auditable and SBOM).
    - Reproducibility check: two builds, same hashes.
    - `docs/linux-porting.md`.

### Suggested order of parallel batches

- **Next batch:** WP-CRT (first, small), WP-USB, WP-AUDIO, WP-SDK-SERVER, WP-L1, WP-DOCS-SDK, WP-FUZZ. No two of these share a hot file, except that WP-SDK-SERVER and WP-AUDIO both make small additions to `engine.rs`.
- **Then:** WP-SERIAL, WP-SCREEN, WP-PARAMS, WP-COMPAT-REST, WP-I801, WP-L2, WP-L4.
- **Then:** WP-LAMPARRAY, WP-MACROS (a), WP-IMPORT, WP-PLAYLISTS, WP-AMDGPU, WP-L5.
- **Last:** WP-LCD, WP-MACROS (b), WP-SENSORS-FANS, WP-L3, WP-L6, WP-L7, WP-SIGN, WP-TRAY.
