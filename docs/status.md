<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# Lightloom status against the plan (M0-M7)

Audit of 2026-10-25 against `develop` at `1899370`. The plan is the original M0-M7 plan
(milestones and acceptance criteria quoted from it in short form); the direction note of
2026-10-18 ("fully standalone, SignalRGB optional") overrides it where the two differ.

Legend:

- **Done**: in `develop`, with evidence (tests, bench notes, a hardware run).
- **Done, untested on hardware**: the code is in `develop` and mock/fake-tested, but it has not
  driven the real device, OS feature or machine yet.
- **Partial**: some of it is in `develop`; the gap is named.
- **Missing**: nothing in `develop` beyond a stub or a placeholder.

Every feature branch of the plan's work packages is merged; nothing is in flight. CI
(`ci.yml`: `rust` on Windows and Ubuntu, `renderer-cef`, `renderer-cef-linux`, `deny`, `node`,
`schemas`, `msi`) is green on `develop`. The release build (`release-build.yml`, called by
`release.yml` on a tag and by `ci.yml`'s weekly `release-dry-run`) and `flatpak.yml` have not
run yet (no tag, no scheduled run so far).

Test counts in `develop` (`#[test]` and `#[tokio::test]` per crate): plugin-host 224, iobroker 77,
daemon 63, hal-windows 58, inputs 52, hal 42, runtime 39, hal-linux 39, renderer 36, api 35,
engine 28, cli 27, nvapi 24, adl 24, setup 16, protocol 16, tray 14, effects 13, render 11,
hid-desc 8, winres 2 (about 800, plus the broker's proptest properties). Node tests:
`prelude/tests/*.test.mjs` (7 files), `plugins/*/tests/*.test.mjs`, `plugins/_testkit/*.test.mjs`.

## Summary

| Milestone | Status | The main gaps |
|---|---|---|
| M0 Bootstrap + spikes | **Done** | none |
| M1 First light | **Done** | the "within 100 ms" first light is met on the native backend only (56-58 ms) |
| M2 Compatible plugin host | **Partial** | rawusb/hybrid and serial sessions are mock-tested only (no WinUSB or serial device on the bench); the fan API is a no-op by decision (out of scope); no replayable `HidTrace` in `plugins/*/fixtures`; the Sonix 320F keyboard is not driven (EVision V1, needs a USB capture); the physical hot-unplug test has not been run |
| M3 Effects pipeline | **Done** | the Chromium sandbox is off until releases are signed |
| M4 Dashboard, persistence, API, packaging | **Partial** | Authenticode signing (SignPath account); the MSI never installed on a clean machine (fresh-machine acceptance); the plugin sources manager; save-to-device, exit modes, suspend/resume, tray and import wizard not yet run live |
| M5 Privileged bus | **Partial** | I801, AMD GPU (ADL) and the sensors not run on hardware; no LpcIO; fan control out of scope (decision 2026-10-25); no CPU temperature on Windows without LibreHardwareMonitor; no soak with HWiNFO/OpenRGB running; `lightloom-setup` does not bind WinUSB |
| M6 Inputs, network, extras | **Partial** | audio, screen zones, meters, OpenRGB SDK server, network plugins, LampArray backend and runtime parameters are in; macro runtime in (untested with real input on hardware); LCD faces are in (mock-tested; no LCD hardware yet, NZXT formats unsupported); latency acceptance and real clients/devices untested; DTLS and https in (loopback-tested; the Hue Entertainment path untested on a bridge) |
| M7 Linux + 1.0 | **Partial** | hidraw, udev hotplug, i2c-dev, hwmon, serial, raw USB (nusb), logind sleep/lock, notifications, the StatusNotifierItem tray, CEF and packaging are in and unit-tested on Ubuntu CI, but nothing has run on a Linux machine; audio and screen capture are in (audio and X11 checked in WSLg, the Wayland portal not yet run); the CEF renderer is in the tarball and AppImage but not the Flatpak; the release and Flatpak workflows have not run yet; cargo-dist deferred |

## M0 Bootstrap and risk retirement: Done

| Item | Status | Evidence |
|---|---|---|
| Workspace, pins, `rust-toolchain.toml`, `deny.toml` | Done | `Cargo.toml` `[workspace.dependencies]`, `deny.toml` |
| CI on Windows and Ubuntu with the mock HAL | Done | `.github/workflows/ci.yml` job `rust` (matrix `windows-latest`, `ubuntu-latest`: fmt, clippy `-D warnings`, test) |
| `cargo deny check` | Done | `ci.yml` job `deny` (`EmbarkStudios/cargo-deny-action@v2`) |
| HAL traits + mock | Done | `crates/lightloom-hal/src/{hid,usb,serial,smbus,hotplug,sensors,capture,audio,lamparray,power}.rs`, `mock.rs` (`MockHid`, `MockUsb`, `MockSmbus`, `TransactionRecorder`), 42 tests |
| ADRs | Done | `docs/adr/0001`-`0010` (0006 Apache/MIT superseded by 0010 GPL-3.0-or-later) |
| `docs/host-api/` seeded from the spec bundle | Done | `docs/host-api/{hid-usb,smbus,components-channels,network-services,network-lightloom,effects-engine,macros-system,save-shutdown}.md` |
| Spike A: CEF windowless at 30 fps into shm, p99 < 40 ms, software compositing | Done | `crates/lightloom-renderer/README.md` (measurements), commit `2d3dd8f`; re-measured in `docs/hardware/effects-m3.md` |
| Spike B: rquickjs loads corpus plugins as ES modules | Done | commit `b5efc18`: 100 % parse, Initialize 98.5-99.0 % per corpus; `crates/lightloom-plugin-host/tests/loader.rs` |

## M1 First light, headless: Done

| Item | Status | Evidence |
|---|---|---|
| hal-windows HID backend, per-TLC endpoint tuples, USB-parent grouping | Done | `crates/lightloom-hal-windows/src/{hid,native,path,group}.rs` (hidapi and native backends), 58 tests in the crate |
| First-party `msi-mystic-light-7c91` | Done | `plugins/msi-mystic-light-7c91/` (fixtures and tests), `docs/hardware/msi-mystic-light-185.md` |
| Null renderer + hi-res frame clock | Done | `crates/lightloom-render/src/{null,clock}.rs` |
| CLI `devices`, `light`, `rainbow`, `bench` | Done | `crates/lightloom-cli/src/main.rs` (also `effect`, `effects`, `plugins`, `compat`, `action`, `udev`) |
| Mock-HAL golden packet test | Done | `crates/lightloom-plugin-host/tests/session_msi.rs` |
| Static C runtime (no `VCRUNTIME140.dll` on a clean Windows) | Done | `.cargo/config.toml` (`+crt-static` for MSVC), `tools/package/check-imports.ps1` on the staged binaries |
| Accept: board lights from the CLI within 100 ms | Done (native backend) | `msi-mystic-light-185.md`: 56-58 ms native, 431 ms with the hidapi backend |
| Accept: 10-min soak, zero HID errors, p99 < 5 ms, RSS < 40 MB | Done | `msi-mystic-light-185.md`: 17 989 frames, 0 errors, p99 4.28 ms, peak RSS 10.6 MB |
| Accept: Ctrl-C restores the prior state | Done | `--restore-check` byte-identical for normal end, `--duration` and Ctrl-Break |
| Accept: tests pass on CI without hardware | Done | `ci.yml` `rust` job |

## M2 Compatible plugin host: Partial

| Item | Status | Evidence / gap |
|---|---|---|
| `lightloom compat dry-run <dir>` + API-gap report | Done | `crates/lightloom-cli/src/compat.rs`, `crates/lightloom-plugin-host/src/dryrun.rs`, commit `337cf1e` |
| `device.*` HID surface (write/read/feature/input reports, endpoints, info) | Done | `crates/lightloom-plugin-host/js/host.js` (device mode), `src/device.rs` |
| Channels, subdevices, components | Done | `host.js`, `tests/session_channels.rs` |
| `bus.*` (SMBus), Free/Fixed wrappers | Done (M5) | `src/smbus.rs`, `tests/session_smbus.rs` |
| `bulk_transfer`, `control_transfer`, `setProtocol` (rawusb, hybrid) | Done, untested on hardware | `lightloom_hal::usb` interface model + `MockUsb`; `crates/lightloom-hal-windows/src/usb.rs` (`WinUsbBackend` on `nusb`, WinUSB-bound interfaces only, no INF); USB devices joined to HID groups (`lightloom-runtime/src/hardware.rs`); `host.js` natives; `tests/session_rawusb.rs`, `crates/lightloom-daemon/tests/winusb.rs`; the device list's `needs-driver` state (`docs/hardware/winusb.md`). No WinUSB device on the reference PC |
| `@SignalRGB/serial` | Done, untested on hardware | `crates/lightloom-plugin-host/src/serial.rs` (own ports only, dry-run writes, read timeouts, reconnect after unplug), `host.js` `serialModule`, `crates/lightloom-hal-windows/src/serial.rs` (SetupAPI + `serialport`), `crates/lightloom-hal-linux/src/tty.rs`; `tests/session_serial.rs`. No serial device tested; no serial hotplug watch (ports are found on enumeration) |
| `@SignalRGB/udp`, `tcp`, `XMLHttpRequest`, `addFeature("udp")` | Done | `crates/lightloom-plugin-host/src/net/`, `tests/network.rs`, `tests/network_plugins.rs`; LAN-only (`docs/host-api/network-lightloom.md`) |
| `addFeature("dtls")`, `https` | Done, untested on hardware | `crates/lightloom-plugin-host/src/net/dtls.rs` (DTLS 1.2 PSK client, `TLS_PSK_WITH_AES_128_GCM_SHA256`, on `ring`; checked against `webrtc-dtls` 0.12.0 in both directions), `net/tls.rs` (rustls over the LAN-only client; certificates accepted by pin, expected common name or trust on first use stored in the plugin settings), `host.js` `dtlsFeatureMembers`, XHR `tlsPin`/`tlsName`/`tlsFingerprint`; `tests/network_secure.rs` (loopback DTLS-PSK and HTTPS servers), unit tests in `net/{dtls,tls,http}.rs`; `docs/host-api/network-lightloom.md`, `docs/security.md`. No real device has spoken DTLS or https to Lightloom yet |
| `addFeature("battery")` | Done, untested on hardware | `crates/lightloom-plugin-host/src/battery.rs`, `crates/lightloom-daemon/src/battery.rs` (low-battery toasts through `crates/lightloom-hal-windows/src/toast.rs`), `ui/src/components/BatteryBadge.svelte`; `tests/session_battery.rs`, `crates/lightloom-daemon/tests/battery.rs`. Used by `plugins/razer` and `plugins/sony-gamepad`; `docs/hardware/razer.md`: battery queries untested on hardware |
| `addFeature("keyboard"/"mouse")` (`sendEvent`, `sendHid`) | Done, untested on hardware | device sessions: `host.js` `inputFeature`, `crates/lightloom-plugin-host/src/input.rs` (`InputSink`), `tests/session_input.rs`; `sendEvent` reaches macro `onKey`/`onMouse`, `sendHid` injects a key or button only while macros are on (`crates/lightloom-macros/src/manager.rs`) |
| Fan API (`createFanControl`, `setRPM`, …) | Out of scope | decided 2026-10-25: Lightloom does not control fans; throws in device sessions (`host.js` `unimplemented("device", …)`); `fanControlDisabled()` answers `true` |
| API_GAP log-and-throw | Done | `prelude/HostApiMissing.js`, `host.js` `gap()`, `prelude/tests/HostApiMissing.test.mjs` |
| `lightloom-hid-desc` | Done | `crates/lightloom-hid-desc/src/lib.rs`, 8 tests |
| `set_endpoint` / `getHidEndpoints` | Done | `host.js`, `src/device.rs` |
| Hotplug via `CM_Register_Notification` | Done | `crates/lightloom-hal-windows/src/hotplug.rs` (HID and USB device interfaces), `crates/lightloom-runtime/src/hotplug.rs`, `tests/manager.rs` |
| Plugin index cache | Done | `crates/lightloom-plugin-host/src/catalog.rs` (cache keyed by path, size, mtime) |
| Watchdog, 64 MiB heap cap | Done | `src/runtime.rs`, `tests/session_robustness.rs`, commit `13514d4` |
| Conformance with HID record/replay | Partial | record/replay (`crates/lightloom-hal/src/trace.rs`, `lightloom compat replay`, `lightloom light <colour> --record`); corpus runner `tests/conformance/run.py` (+ `test_run.py`, run in `ci.yml`) and the manually dispatched `.github/workflows/conformance-selfhosted.yml`. None of the 65 JSON files in `plugins/*/fixtures` is a `HidTrace` (they are packet lists), so the replay stage skips everything until traces are recorded on the reference machine |
| Accept: 100 % of the corpus parses, >= 95 % Initialize | Done | commit `b5efc18`: 100 % parse; 98.5 % (CDN), 98.7 % (bundled), 99.0 % (addons) |
| Accept: unmodified plugins drive the DeathAdder, Arctis 5 | Done | `docs/hardware/imported-plugins-m2.md` |
| Accept: Sonix 320F keyboard (multi-collection Validate) | **Missing** | no SignalRGB plugin exists for `320F:5064`; `plugins/evision-keyboard` covers V2 only and excludes V1 for flash wear; a stored-settings-only V1 mode needs the user's USB capture (`docs/research/evision-v1.md`) |
| Accept: hot-unplug/replug recovers in < 2 s | Partial | mock test `tests/manager.rs`; the physical unplug on the reference machine is still to do (`imported-plugins-m2.md`, "Hotplug") |

## M3 Effects pipeline: Done

| Item | Status | Evidence / gap |
|---|---|---|
| `lightloom-renderer` (CEF OSR), shm ring, control pipe | Done | `crates/lightloom-renderer/src/{cef_host,ring,control,sink,ipc}.rs`, 36 tests |
| Supervision and restart holding the last frame | Done | `crates/lightloom-render/src/cef.rs`, `tests/cef_client.rs`; 1522 ms recovery (`effects-m3.md`) |
| `<meta>` parser, control model, load sequence | Done | `crates/lightloom-effects/src/parse.rs`, `crates/lightloom-renderer/src/inject.rs`, `tests/starter_effects.rs` |
| `engine.*` via JSON push, `onCanvasTapped`, `onCanvasApiEvent` | Done | `inject.rs`, `/v1/canvas/event`, `/v1/canvas/tap`, compat `:16034` |
| Layout + sampling, brightness chain | Done | `crates/lightloom-engine/src/{layout,sampler}.rs` |
| 15-fps downshift guard, `lightloom bench` | Done | `crates/lightloom-render/src/cef.rs` (`DOWNSHIFT_WINDOW`), `crates/lightloom-cli/src/run.rs` |
| Accept: 20 lightscripts at 30 fps | Done | `effects-m3.md` (7 SignalRGB, 13 Lightloom CC0) |
| Accept: PSNR >= 40 dB | Done | 40/40 bit-identical |
| Accept: canvas-to-wire p50 <= 10, p99 <= 25 ms, 4 devices | Done | p50 0.9-3.8, p99 2.2-4.3 ms |
| Accept: renderer RSS <= 300 MB | Done (one exception) | 284-291 MB; Aurora in software compositing 411 MB |
| Accept: crash recovered < 3 s | Done | 1522 ms |
| Chromium sandbox | Missing | `crates/lightloom-renderer/README.md` "Chromium sandbox": CEF 154 needs a signed bootstrap executable; waits for signing (WP-SIGN) |

## M4 Dashboard, persistence, local API, packaging: Partial

| Item | Status | Evidence / gap |
|---|---|---|
| SQLite + migrations | Done | `crates/lightloom-daemon/src/store.rs` (`rusqlite` + `rusqlite_migration`; tables kv, devices, device_parameters, layouts, placements, channel_components, effect_params, presets, playlists, custom_leds); export/import round-trip test |
| Native API (REST + WS, token, Host check) | Done | `crates/lightloom-api/src/server.rs` (`/v1/*`, `/v1/events`, `/v1/preview`); `docs/hardware/daemon-m4.md` |
| Configurable API port | Done | default `NATIVE_PORT` 53000 (`crates/lightloom-api/src/lib.rs`); `crates/lightloom-daemon/src/port.rs`: `--port`, the dashboard's Port setting, `config.toml` `[api] port`, never a random port when busy |
| Compat REST 16038 | Done | `compat_router()`: every route of SignalRGB's local API (app, new, partners, lighting, global_brightness, enabled, effects, effect, image, apply, presets, next, previous, shuffle, loadEffect, playlists, playlist, play, scenes/current_layout, scenes/layouts); envelope contract test for every route; `docs/integrations/signalrgb-api.md` |
| Action URLs (`lightloom://`) | Done | `crates/lightloom-api/src/actions.rs`, `crates/lightloom-daemon/src/engine/actions.rs`, `scheme.rs`: `POST /v1/actions`, `lightloom action <url>`, SignalRGB `system.action` paths (app, view, layout, effect, device, playlist, brightness, canvas), dashboard `action` events, opt-in per-user URL-scheme registration; `crates/lightloom-daemon/tests/actions.rs`; `docs/integrations/actions.md` |
| Compat canvas events 16034 | Done | `server.rs` `compat_canvas_event`; listeners wait while SignalRGB owns the ports |
| Dashboard: devices, layout editor (drag, scale, rotate, flip), effects + controls + presets, channels/components, network, sensors, settings | Done | `ui/src/components/{Devices,Layout,Effects,Preview,Network,Sensors,Settings,ControlInput}.svelte` |
| Per-LED custom colours and zones ("Customise LEDs", OpenRGB-style) | Done, untested on hardware | `crates/lightloom-engine/src/custom.rs` (`CustomLeds`, `led_spots`), static layer of `DirectLeds` (`direct.rs`) so plugins see the colours through `device.color` with brightness applied; `crates/lightloom-daemon/src/engine/custom_leds.rs` (debounced saves), table `custom_leds` (`store/leds.rs`, in the export); `GET /v1/devices/{key}/leds`, `PATCH /v1/devices/{key}/leds/{uid}`, `PUT`/`DELETE .../zones/{name}`; `ui/src/components/LedPainter.svelte`, `ui/src/lib/painter.ts`; `crates/lightloom-daemon/tests/custom_leds.rs`; `docs/custom-leds.md`. No lasso, no real keyboard tried |
| Dashboard: logs page | Done | `GET /v1/logs` (`crates/lightloom-api/src/logs.rs`: level, text and device/plugin filters, newest `limit` lines, API token and bearer credentials redacted); history from `lightloomd.log` + `lightloomd.old.log` when started by `lightloomw` (`LIGHTLOOM_LOG_FILE`), else an in-memory ring of 5000 lines with the plugins' `device.log` lines (`crates/lightloom-daemon/src/logs.rs`); `ui/src/components/Logs.svelte` (Settings → Log: filters, follow, Copy, Save as file) |
| Import wizard | Done, untested on a real SignalRGB install | `crates/lightloom-runtime/src/signalrgb.rs` (`SettingsSource`: registry read-only, `Fixture` in tests): layout, channel arrays, effect parameters and presets, selected effect, global brightness, per-device enable; `GET /v1/import/signalrgb` previews, `POST` merges (`Store::merge_effects`); `thirdParty` badges; `crates/lightloom-daemon/tests/import.rs`; `docs/signalrgb-import.md`. Read on the reference machine with `reg query` only; where SignalRGB keeps saved presets and `globalBrightness` is unconfirmed |
| Own export/import of the whole setup | Done | `/v1/export`, `/v1/import`, `store.rs` test `export_import_round_trips_byte_for_byte` |
| Profiles (OpenRGB-style named snapshots of the whole setup) | Done, untested live | `crates/lightloom-daemon/src/engine/profiles.rs`, `store.rs` (table `profiles`): effect and values, brightness and pause, layout, per device enable / parameters / channels / exit mode / hardware mode / custom LEDs (open per-device object: unknown keys kept); save, apply, rename, duplicate, delete, export/import as JSON, default profile at start; `/v1/profiles*`, `lightloom://profile/apply/<name>` (and `layout/apply` of a profile name), macro option `profilename`; dashboard Profiles tab and header switch, tray submenu; `crates/lightloom-daemon/tests/profiles.rs`; `docs/profiles.md` |
| Hardware modes (live built-in device effects) | Done, untested on hardware | `HardwareModes()` / `SetHardwareMode(request)` (`crates/lightloom-plugin-host/src/hwmode.rs`, `session.rs`: no `Render()` while a mode runs, denied SMBus registers stay refused); kept per device, `PUT /v1/devices/{key}/mode`, device card "Mode" select; `msi-gpu` (19 OpenRGB modes, never 0x3F), `msi-mystic-light-7c91` (static zones, save byte 0); manifest `hardwareModes` (schema, checked by the lint against `HardwareModes()` in its mock run, `sdk/lint/tests/lint.test.mjs`); `tests/session_hwmode.rs`, plugin node tests; `docs/host-api/hardware-modes.md` |
| Plugin sources manager (pre-listed community repos, licence acknowledgement, hash pinning) | Missing | `schemas/plugin-source.schema.json` exists; no code. Lower priority under the standalone direction |
| Tray shell, autostart | Done, untested live | `crates/lightloom-tray` (`Shell_NotifyIconW`: open dashboard, pause/resume, brightness, start at sign-in, quit; fake-API tests, 14); `lightloomw` starts it and owns the `HKCU\...\Run` value (`lightloomw --autostart on|off`), the MSI uses it. Linux: StatusNotifierItem with the same menu (`sni.rs`; no Start at sign-in, the systemd user unit does that), untested on a desktop |
| Effect playlists / auto-next timer | Done | `crates/lightloom-daemon/src/engine/playlists.rs`, `store.rs` table `playlists`, `/v1/playlists*`, `/v1/lighting/timer`, `ui/src/components/Playlists.svelte`; `crates/lightloom-daemon/tests/playlists.rs` |
| Suspend/resume (`Shutdown(true)` and re-`Initialize`) | Done on Windows, untested live | `crates/lightloom-hal-windows/src/power.rs` (hidden window: sleep/resume, lock/unlock, display off/on), `crates/lightloom-runtime/src/power.rs`, `crates/lightloom-daemon/src/lifecycle.rs`; `crates/lightloom-daemon/tests/suspend.rs`; `docs/power.md`. Real sleep on the user's PC not tried; Linux: `crates/lightloom-hal-linux/src/logind.rs` (`PrepareForSleep` with a delay inhibitor, session lock/unlock), fake-logind tests only |
| On-exit behaviour, bake to device | Done, untested on hardware | `crates/lightloom-plugin-host/src/save.rs` (`SaveToDevice` behind confirmation, 60 s limiter, preview as dry run; `ShutdownMode` restore/keep/off with per-device overrides), `/v1/save`, `/v1/save/preview`, `ui/src/components/SaveDialog.svelte`; `tests/session_save.rs`; `docs/host-api/save-shutdown.md` ("untested on hardware"); plugins `msi-gpu`, `msi-mystic-light*`, `razer` |
| Portable build | Done | `tools/package/stage-windows.ps1`, `release-build.yml` `windows` |
| WiX MSI, winget | Done, never installed on a clean machine | `packaging/wix/` (per-user WiX v5 MSI, en-US and it-IT, options page, PawnIO option through `lightloom-setup`, `build.ps1`), `packaging/winget/` templates + `render.ps1`; `ci.yml` job `msi` builds it from a stand-in stage; `release-build.yml` `msi` (never run) builds the real one and checks its name (`Lightloom-<x.y.z>-x64.msi`, what the updater looks for) |
| Authenticode (SignPath) | Missing | `release-build.yml` "SIGNING" placeholder; needs a SignPath OSS account |
| Accept: fresh Win11 → MSI → lit devices with no terminal | Missing | the MSI exists; the fresh-machine run (VM or clean install) is not done |
| Accept: component JSON on JRAINBOW1 lights an ARGB strip | Done | `imported-plugins-m2.md` (Default Strip on JRainbow 1/2), `daemon-m4.md` |
| Accept: a tool hitting `GET /api/v1/lighting` on 16038 works | Done | `daemon-m4.md`, `server.rs` compat router |
| Accept: export/import round-trips | Done | store test above |

## M5 Privileged bus via broker: Partial

| Item | Status | Evidence / gap |
|---|---|---|
| `lightloom-iobroker` (LocalSystem service), op enum, mutex, allowlists, rate limit, dry-run, audit | Done | `crates/lightloom-iobroker/src/{server,policy,mutex,pipe,pawnio}.rs`, `crates/lightloom-protocol/src/broker.rs`, 77 + 16 tests |
| PawnIO SmbusPIIX4, pinned | Done | `crates/lightloom-iobroker/src/{piix4,pins}.rs` (PawnIO.Modules 0.2.11 SHA-256) |
| PawnIO SmbusI801 (Intel) | Done, untested on hardware | `crates/lightloom-iobroker/src/{i801,chipset,pins,boot}.rs`: pinned, tried after PIIX4, fake + fuzz tested; needs an Intel board |
| PawnIO LpcIO (SuperIO, NCT6687D) | Missing | module name in `broker.rs` docs only; no code |
| `lightloom-setup` one-shot elevated installer | Partial | `crates/lightloom-setup/src/{pawnio_setup,trust,net}.rs`: `--pawnio-install` (verified download and silent install of PawnIO 2.2.0), pinned modules + broker, registers the service; the MSI option and Settings → PawnIO / RAM lighting run it behind one UAC prompt. No WinUSB binding (by design Lightloom ships no INF, `docs/hardware/winusb.md`) |
| `bus.*` mapping, capability matrix | Done | `crates/lightloom-plugin-host/src/smbus.rs`, `plugins/README.md` "SMBus plugins" |
| `lightloom-nvapi` GPU I2C | Done | `crates/lightloom-nvapi/`, 24 tests; `plugins/msi-gpu`, `docs/hardware/msi-gpu.md` |
| AMD GPU I2C (ADL) | Done, untested on hardware | `crates/lightloom-adl/` (legacy ADL, fake-backed tests, 24); `plugins/sapphire-nitro-glow`, `docs/hardware/sapphire-nitro-glow.md`; no AMD card on the reference PC |
| Sensors (NvAPI thermal, LibreHardwareMonitor JSON, system load, hwmon) | Done, read-only, untested on hardware | `docs/sensors.md`: registry `crates/lightloom-runtime/src/sensors.rs`, NvAPI queries `crates/lightloom-nvapi/src/sensors.rs`, poller `crates/lightloom-daemon/src/sensors.rs` (at most 1 Hz, on demand), `engine.sensors` / `getSensorValue`, `GET /v1/sensors`, `Sensors.svelte`. NvAPI queries and LHM's real server not run; CPU MSRs through PawnIO left as future work |
| Fan curves + SuperIO | Out of scope | decided 2026-10-25: Lightloom is lighting-only; the plugin fan API stays a no-op |
| Accept: Crucial Ballistix via SmbusPIIX4, unmodified imported plugin | Done with a first-party plugin | `docs/hardware/crucial-ballistix.md`: both modules tested 2026-10-22 with Lightloom's `plugins/crucial-ballistix` (the standalone direction replaces the imported plugin) |
| Accept: RTX 3060 via NvAPI I2C | Done | commit `9311108`, `docs/hardware/msi-gpu.md` |
| Accept: 30-min soak with HWiNFO/OpenRGB running, no bus errors | Missing | not run |
| Accept: broker fuzz, 1M iterations | Done | `crates/lightloom-iobroker/src/fuzz.rs` (proptest properties plus a deterministic driver in `cargo test`), cargo-fuzz targets `frame` and `session` in `crates/lightloom-iobroker/fuzz/`; release run of 1M inputs (9.97M requests), no panic; found and fixed a stack overflow on nested `Batch` frames |
| Accept: no UAC after setup | Done in practice | the broker is a service; daemon runs unelevated (`crucial-ballistix.md` procedure). Not re-checked on a fresh machine |

## M6 Inputs, network, extras: Partial

| Item | Status | Evidence / gap |
|---|---|---|
| Audio FFT (`engine.audio`) | Done (Windows; Linux checked in WSLg) | `crates/lightloom-inputs/src/audio/{wasapi,analyzer}.rs` (cpal WASAPI loopback, realfft, 200 bins, level, density, width), `audio/pulse.rs` (Linux: sink monitor over the PulseAudio protocol), `crates/lightloom-daemon/src/audio.rs`; `docs/audio.md`. Output device picker: `GET/PATCH /v1/inputs`, `ui/src/components/InputDevices.svelte` (saved in the database, wins over `config.toml`); `crates/lightloom-daemon/tests/inputs.rs`. Loopback of output devices only (never microphones), by design |
| Screen zones (`engine.zone`) | Done (Windows; Linux X11 checked in WSLg, Wayland not yet run) | `crates/lightloom-inputs/src/screen/{dxgi,sample}.rs` (DXGI duplication, checked on the dev PC), `screen/linux/` (ScreenCast portal + PipeWire, X11 MIT-SHM), `crates/lightloom-daemon/src/screen.rs`; `docs/screen.md`. Display picker: `GET/PATCH /v1/inputs`, Settings → Inputs |
| Area/linear meters (`engine.vision`) | Done | `crates/lightloom-inputs/src/screen/meters.rs` (area, linear, colormean from `<meta meter>`); OCR out of scope |
| Network plugins (udp/tcp, DTLS, https, DiscoveryService, mDNS, `service`/`controller`/`discovery`, XHR, settings, dashboard section) | Done, untested on hardware | first-party `wled`, `ddp`, `e131`, `philips-hue` (Entertainment API: CLIP v2 + HueStream v2 over DTLS, CLIP v1 REST fallback), `nanoleaf`, `govee`, `lifx`, `yeelight`; `Network.svelte`; `docs/hardware/{wled,…}.md` |
| LCD faces (`@SignalRGB/lcd`, second browser per device) | Done, untested on hardware | `crates/lightloom-lcd` (JPEG, PNG, BMP, RGBA, RGB, RGB565 encoders; crops, slices); `crates/lightloom-plugin-host/src/lcd.rs` (`initialize`, `getFrame`, `getSlicedFrame`, `getImageBuffer`, typed `LcdError`); `crates/lightloom-daemon/src/lcd.rs` (one `lightloom-renderer --face` per LCD device at its size and `lcdFramerate`, mock pattern without a renderer, sensors); `PUT /v1/devices/{key}/lcd` (face, frame rate, per-face control values kept per device and face, live to the page), `GET /v1/lcd/faces` (with controls), `GET /v1/devices/{key}/lcd/preview` (PNG), `LcdFacePicker.svelte` (controls) and `LcdPreview.svelte` (about 2 fps, only while on screen); starter faces `faces/clock.html`, `faces/sensor.html`; `docs/host-api/lcd-lightloom.md`; tests `crates/lightloom-daemon/src/lcd.rs`, `tests/lcd.rs`, `ui/src/lib/preview-loop.test.ts`. Gaps: `NZXT::RGB565` / `NZXT::Q565` layouts unknown (typed `UNSUPPORTED_FORMAT`), background layer, rotation, idle blanking |
| Macros (Inputs/Actions), key/mouse hooks, `system.*`, `signalrgb.*`, `keys` | Done, untested with real input | `crates/lightloom-macros` (QuickJS runtime and thread per script, `js/macro_host.js` globals, options, key names, `signalrgb.*` mapping, event bus, opt-in hooks, emergency stop), `crates/lightloom-hal-windows/src/input.rs` (`SendInput`, clipboard, `ShellExecuteW`, `GetLastInputInfo`, `WH_KEYBOARD_LL`/`WH_MOUSE_LL`), `crates/lightloom-daemon/src/engine/macros.rs`, `/v1/macros*`, `ui/src/components/Macros.svelte` (live from `macros` events, layout and profile pickers); events also `System Time` (each minute), `System Logoff` (`WTS_SESSION_LOGOFF` / `WM_ENDSESSION`), `Foreground App Changed` (polled `GetForegroundWindow`, image name and title, never logged) and `onSensorUpdate` (sensor poller); `docs/macros.md`. Tests use a recording desktop only (`crates/lightloom-macros/tests/runtime.rs`, `crates/lightloom-daemon/tests/macros.rs`). Missing: macro layers/profiles, Tier-1 native rules, the `Audio Capture` event, `elevateWindow`/`cycleWindow`/`getForegroundExplorerPath` (no Explorer integration), Linux input (and a Linux foreground window; the clock is UTC there) |
| Action router (`lightloom://`, `signalrgb://`) | Done | see M4 "Action URLs"; `signalrgb://` paths accepted by `actions.rs` |
| LampArray HAL backend (WinRT `Windows.Devices.Lights`) | Done, untested on hardware | `crates/lightloom-hal-windows/src/lamparray.rs`, built-in session `crates/lightloom-plugin-host/src/lamparray.rs`, ownership `crates/lightloom-runtime/src/lamparray.rs`; identity package recipe `packaging/msix-identity/` (never registered); `docs/windows-dynamic-lighting.md` "Not verified on hardware". `plugins/hid-lamparray` drives HID LampArrays first (tested on the DeathAdder's LampArray collection) |
| OpenRGB SDK server :6742 | Done, untested with a real client | `crates/lightloom-api/src/openrgb/{mod,wire}.rs` (opt-in, loopback only, Settings "OpenRGB SDK"), direct writes through `crates/lightloom-engine/src/direct.rs`; byte goldens and a TCP session against a fake daemon (`openrgb/tests.rs`); `docs/integrations/openrgb-sdk.md` "Not tested yet" |
| Dynamic ControllableParameters UI | Done | `crates/lightloom-plugin-host/src/plugin_ui.rs` (`addProperty`/`removeProperty`/`addMessage`/`removeMessage`/`notify`/`denotify` through natives), session events channel, `DeviceView.messages`, `coerce_param_value`; `Devices.svelte`/`ControlInput.svelte` (`order`, `group`, `live`, `filter`, `keycode`); `tests/session_runtime_props.rs`, `crates/lightloom-daemon/tests/runtime_params.rs` |
| Accept: audio-reactive + screen effects track input < 100 ms | Partial | code in place; latency not measured on the user's PC (`docs/audio.md` "Acceptance") |
| Accept: Govee/Yeelight/WLED-class plugin drives a LAN device | Partial | mock and loopback tests only; needs a real device |
| Accept: OpenRGB SDK client sets a colour on the MSI board | Partial | server done; no real client tried |
| Accept: a DL-certified keyboard lights with no plugin | Partial | the generic `hid-lamparray` plugin is that path for HID LampArray devices, the WinRT backend for LampArrays no plugin owns (needs the identity package); not tested on a keyboard |

## M7 Linux backend + 1.0: Partial

Everything in `lightloom-hal-linux` is unit-tested on Ubuntu in `ci.yml` (`rust` matrix,
fake sysfs and fake devices, 39 tests); none of it has run against hardware on a Linux machine.

| Item | Status | Evidence / gap |
|---|---|---|
| hal-linux hidraw + hid-desc | Done, untested on Linux hardware | `crates/lightloom-hal-linux/src/{sysfs,tlc,hidraw,sys}.rs`: sysfs enumeration, TLC split, raw hidraw ioctls; `lightloom devices` and hardware runs wired through `lightloom-runtime` |
| udev hotplug | Done, untested on Linux hardware | `crates/lightloom-hal-linux/src/{hotplug,uevent}.rs`: netlink udev monitor, pure Rust |
| i2c-dev SMBus, hwmon sensors | Done, untested on Linux hardware | `crates/lightloom-hal-linux/src/{i2c,hwmon}.rs`: adapters by sysfs name (PIIX4 ports 0/1, i801, NVIDIA port 1, AMDGPU OEM), Windows policy plus the broker write allowlist on DIMM buses, SPD from the kernel `eeprom` file or a host-internal EE1004 page select; wired in `lightloom-runtime` (`smbus.rs`, `sensors.rs`); udev i2c rules per adapter name. AMDGPU DDC and DP AUX buses left out |
| nusb (raw USB on Linux) | Done, untested on Linux hardware | WP-L3: `crates/lightloom-hal-linux/src/usb.rs` (`LinuxUsbBackend`): devices, interfaces, endpoints and kernel drivers from sysfs (fake-sysfs tests), usbfs I/O through `nusb` (bulk/interrupt and control transfers); interfaces without a kernel driver are claimed, `rawusb` plugins may also detach the kernel driver of a non-HID interface (`UsbBackend::for_rawusb`, re-attached on close), HID interfaces never; hybrid devices join their hidraw group by the same `USB\VID&PID\<serial or port>` id; `usb_device` uevents feed hotplug; udev `usb_device` rules for rawusb/hybrid VID:PIDs from `lightloom udev generate`. No raw USB device run on Linux |
| hwmon `pwm` (fan control) | Out of scope | decided 2026-10-25; hwmon stays read-only |
| serialport (Linux) | Done, untested on Linux hardware | `crates/lightloom-hal-linux/src/tty.rs`: tty enumeration from sysfs with the USB parent, `serialport` I/O |
| `tools/gen-udev` | Done | `lightloom udev generate` (`crates/lightloom-cli/src/udev.rs`); generated `packaging/udev/70-lightloom.rules` with a drift test; `packaging/sysusers.d/lightloom.conf`, `packaging/modules-load.d/lightloom.conf` |
| CEF on Linux (X11/Wayland, headless CI) | Done (CI only) | `crates/lightloom-renderer/src/ipc.rs` (tmpfs ring, unix-socket control), headless ozone by default, `$ORIGIN` rpath; `ci.yml` `renderer-cef-linux` (60-frame smoke run, `lightloom-render/tests/cef_real.rs`). Staged into the tarball and AppImage with its CEF runtime (`stage-linux.sh --require-renderer`; `release-build.yml` `linux` smoke-runs the staged copy, never run yet). Not run on a Linux desktop; not in the Flatpak |
| PipeWire audio + screen capture | Done, partly untested on Linux | WP-L6: `crates/lightloom-inputs/src/audio/pulse.rs` (PulseAudio protocol, pure Rust: PipeWire via `pipewire-pulse`, PulseAudio, Flatpak `--socket=pulseaudio`; only sink monitors, checked to be the monitor of that sink), `crates/lightloom-inputs/src/screen/linux/` (`portal.rs`: ashpd ScreenCast with a persisted restore token + a PipeWire stream; `x11.rs`: RandR + MIT-SHM; `drm.rs`: display list from sysfs); cargo features `pulseaudio`, `screencast`, `x11`; `docs/audio.md`, `docs/screen.md`. Audio and X11 run in WSLg (manual `--ignored` tests); the portal path has not run on a Wayland desktop |
| Linux sleep/resume, notifications, tray | Done, untested on a Linux desktop | WP-L8, zbus 6 (pure Rust, MIT) through `crates/lightloom-hal-linux/src/dbus.rs`: `logind.rs` (`LogindPower`: `PrepareForSleep` with a `delay` inhibitor lock, session `Lock`/`Unlock` and `LockedHint`), `notifications.rs` (`FreedesktopNotifier`, `org.freedesktop.Notifications`), wired in `lightloom-runtime` (`power.rs`, `notify.rs`); `crates/lightloom-tray/src/sni.rs` (StatusNotifierItem + `com.canonical.dbusmenu`, the Windows menu without Start at sign-in; no watcher: the old message). Tests over peer-to-peer D-Bus (fake logind, notification server and SNI host; Linux CI only). No display on/off from logind |
| `systemd --user` unit | Partial | `packaging/systemd/lightloomd.service`: user-instance-safe hardening, restart limits; passes `systemd-analyze --user verify`; not yet run as a real service on Linux |
| AppImage, Flatpak | Partial | `packaging/appimage` (linuxdeploy in two steps so the CEF runtime is not patched, AppRun) and `packaging/flatpak` (25.08 manifest, metainfo, pinned offline source generation); `release-build.yml` `linux` builds the tarball and AppImage with the renderer; `flatpak.yml` builds the Flatpak (flatpak-builder in the Flathub container, weekly and on dispatch). Neither workflow has run yet; no renderer in the Flatpak |
| `docs/linux-porting.md` | Done | user guide (udev, i2c, conflicts) and developer part (where the Linux code lives, what is missing) |
| Accept: same plugins light the same hardware on Ubuntu LTS | Missing | needs a Linux run on the reference machine |
| Accept: identical `Validate` tuples on both OSes (fixture) | Partial | `crates/lightloom-hal-linux/src/fixtures.rs`: one descriptor through the Windows path logic and the Linux split; descriptors are synthetic, a captured Linux `report_descriptor` per reference device is still to record |
| Accept: RAM via `/dev/i2c-*` without a helper | Partial | code done; needs a Linux run on the reference machine |
| Accept: conformance suite on both OSes | Partial | `tests/conformance/run.py` + `conformance-selfhosted.yml` (Windows reference machine, never dispatched); no Linux self-hosted runner |
| Accept: reproducible release via cargo-dist | Partial | `release-build.yml` `linux-reproducible` (two builds, same hashes, daemon + CLI; the release waits for it), never run; cargo-dist deferred with a migration plan in `docs/linux-porting.md` |

## Planned components outside the milestones

| Component | Status | Evidence / gap |
|---|---|---|
| Crates (17 planned) | Done (21) | all planned crates are real; added: `lightloom-runtime` (shared CLI/daemon plumbing), `lightloom-adl`, `lightloom-tray`, `lightloom-winres`. `crates/README.md` describes all 21 |
| First-party plugins | Done for the plan's list, except Sonix | 64 plugin directories: the plan's list (`msi-mystic-light-7c91`, `razer`, `nzxt-hue2`, `steelseries-arctis-5`, `crucial-ballistix`, `hid-lamparray`), `msi-gpu`, `sony-gamepad`, `sapphire-nitro-glow`, OpenRGB batch 1 (`razer*`, `steelseries-*`, `msi-mystic-light*`, `evision-keyboard` V2) and batch 2 (`asus-aura-*`, `coolermaster-*`, `corsair-*`, `hyperx-*`, `lenovo-legion-*`, `logitech-*`, `roccat-*`, `thermaltake-riing`, `wooting*`), the network plugins. Hardware-tested: MSI 7C91 board, DeathAdder V2 Pro, Kraken X3, Arctis 5, Ballistix, RTX 3060, DualSense; every batch 2 and network plugin is untested on hardware (empty `tested` lists) |
| Starter effects (solid, rainbow-wave, audio-bars, screen-ambience, test-pattern) | Done | `effects/` has 13 CC0 effects |
| `components/` own component library | Done | `components/*.json`: 50 CC0 descriptors generated by `tools/components/generate.mjs`, checked by `plugins/_testkit/components.test.mjs` and `crates/lightloom-daemon/tests/components.rs`; shipped by `stage-windows.ps1`. Ring geometry and wire order approximate (`components/README.md`) |
| `schemas/` (manifest, ControllableParameters, component, layout, effect meta) | Done | `schemas/*.schema.json`, `tests/schemas/validate_corpus.py`, `ci.yml` job `schemas` |
| OpenAPI for the native and compatible APIs | Done | `docs/api/openapi.yaml` (native, OpenAPI 3.1) and `docs/api/compat-openapi.yaml` (SignalRGB-compatible REST 16038 and canvas events 16034); `crates/lightloom-api/tests/{openapi,compat_openapi}.rs` check them against the routers, `NATIVE_ROUTES` and `COMPAT_ROUTES` |
| `sdk/lightloom.d.ts` | Done | `sdk/lightloom.d.ts`, members tagged `@signalrgb` / `@lightloom` / `@unimplemented` / `@stub`; `tsc -p sdk` in CI |
| `sdk/` Node simulator (mock HAL recorder) | Done | `sdk/simulator` (over `plugins/_testkit`); `lightloom bench --mock` runs the real host against a scripted device |
| `sdk/` plugin lint | Done | `sdk/lint`; `plugins/_testkit/manifests.test.mjs` runs it over the first-party plugins |
| `tests/conformance/` (corpus runner, HID fixtures, PSNR harness) | Partial | `tests/conformance/run.py` (+ `test_run.py`, in `ci.yml`), `tools/effects-bench/psnr.py` hook; no recorded `HidTrace` fixtures yet (see M2) |
| `tests/integration/` | Partial | crate integration tests (`crates/*/tests/`); no top-level directory |
| `tools/gen-udev` | Done | `lightloom udev generate` (`tools/gen-udev/README.md` points there) |
| `tools/pawnio-hash-pin` | Done | `tools/pawnio-hash-pin/pawnio-hash-pin.ps1` (recompute from the release or a folder, check against `pins.rs` and `fetch-pawnio-modules.ps1`, `-Emit` for a release bump; run against 0.2.11: all match); drift test `crates/lightloom-iobroker/tests/pawnio_pins.rs` |
| `tools/shm-dump` | Done | `lightloom shm-dump [RING] [--out] [--json]` (`crates/lightloom-cli/src/shm.rs`, `tools/shm-dump/README.md`) |
| `tools/effects-bench`, `tools/device-probe`, `tools/package` | Done | `tools/README.md`; `tools/package/{stage-windows.ps1,stage-linux.sh,check-imports.ps1,fetch-pawnio-modules.ps1}` |
| `prelude/` | Done | `prelude/*.js`, `prelude/modules/*.js`, 7 test files |
| `docs/security.md`, `docs/linux-porting.md` | Done | both present |
| Packaging: `wix/`, `winget/` | Done, never installed on a clean machine | `packaging/README.md`; `ci.yml` `msi` |
| Packaging: `systemd/`, `udev/`, `sysusers.d/`, `modules-load.d/` | Done, untested on Linux | files present, `systemd-analyze verify` only |
| Packaging: `appimage/`, `flatpak/` | Partial | built by `release-build.yml` and `flatpak.yml`, neither run yet |
| Packaging: `msix-identity/` | Partial | recipe and `build.ps1`; never registered |
| CI `rust` (win + ubuntu), `renderer-cef`, `renderer-cef-linux`, `deny`, `node`, `schemas`, `msi` | Done | `ci.yml`, green on `develop` |
| CI `conformance-selfhosted.yml` | Done, never dispatched | manual dispatch only, labels `self-hosted, windows, lightloom-reference`; needs the user's self-hosted runner |
| CI `release.yml` via cargo-dist, signing, SBOM | Partial | hand-written: `release.yml` (tag → draft release) calls the reusable `release-build.yml` (SBOM + pinned cargo-auditable, Windows zip + MSI, Linux tarball + AppImage with the CEF renderer, reproducibility gate, one `SHA256SUMS`); `ci.yml` `release-dry-run` runs the same build weekly and on dispatch without publishing. Never run; cargo-dist and signing missing |
| CI `flatpak.yml` | Done, never run | weekly and on dispatch: offline sources, flatpak-builder in the Flathub `freedesktop-25.08` container, bundle as an artifact |
| Issue forms | Done | `.github/ISSUE_TEMPLATE/{device-report,bug-report,feature-request,config}.yml` |
| Dependabot | Done | `.github/dependabot.yml`; `docs/research/dependabot-2026-10.md` |

## Remaining work packages

Each package is meant for one agent working alone. "Touches" lists where its changes
land, so the coordinator can avoid running two packages with the same hot files in
parallel. Sizes: **S** about a day, **M** two to four days, **L** a week or more, for
one agent with the mock HAL.

**Shared hot files.** Most packages need a few lines in one of these. Run at most two
packages that touch the same one in parallel, and merge the smaller one first:

- `crates/lightloom-plugin-host/js/host.js`
- `crates/lightloom-plugin-host/src/{session,device,catalog}.rs`
- `crates/lightloom-daemon/src/engine.rs`
- `crates/lightloom-api/src/server.rs`
- `ui/src/components/*.svelte`
- root `Cargo.toml` `[workspace.dependencies]`

**Merged into `develop`** (code done; hardware or live checks listed under WP-SOAK):
WP-CRT, WP-USB, WP-SERIAL, WP-I801, WP-AMDGPU, WP-SUSPEND (Windows), WP-AUDIO, WP-SCREEN,
WP-METERS, WP-SDK-SERVER, WP-LAMPARRAY, WP-PARAMS, WP-MACROS (a) action router, WP-COMPAT-REST,
WP-IMPORT, WP-PLAYLISTS, WP-SENSORS-FANS (sensors half), WP-FUZZ, WP-CONFORMANCE, WP-DOCS-SDK,
WP-COMPONENTS, WP-TRAY, WP-L1, WP-L2, WP-L4, WP-L5, WP-L7 (except cargo-dist), plus the
configurable port, battery and toasts, bake/exit modes, network plugins, OpenRGB batch 2,
the WiX installer and issue forms.

### Still to do: needs the user (hardware, accounts, captures)

1. **WP-SOAK: hardware and live acceptance runs (user present). Size S each.**
   - Physical hot-unplug under 2 s (M2).
   - 30-min SMBus soak with HWiNFO and OpenRGB running (M5).
   - Fresh-machine MSI install, no terminal, devices lit (M4).
   - Save to devices, the exit modes and the hardware modes on the MSI board and the RTX 3060; profiles on the real setup; real sleep/resume and lock; the tray; the SignalRGB import against the real registry.
   - Audio Bars and screen ambience latency under 100 ms (M6); NvAPI sensor queries; an OpenRGB SDK client (OpenRGB, openrgb-python) setting a colour on the MSI board.
   - Record `HidTrace` fixtures (`lightloom light <colour> --record`) for the tested devices, so the conformance replay stage has something to replay; dispatch `conformance-selfhosted.yml` once a runner is registered.
   - Needs hardware not on the reference PC: a WinUSB device, a serial device, an Intel board (I801), an AMD GPU, a LAN light (WLED/Govee/Hue/…), a Dynamic Lighting keyboard, any batch 2 device.
2. **WP-EVISION-V1: Sonix `320F:5064`, stored-settings-only mode. Size M.**
   - Blocked on the user's USB capture: see the procedure in `docs/research/evision-v1.md`.
   - Touches: `plugins/evision-keyboard/` only (new `evision-v1.js`), using the save path of `docs/host-api/save-shutdown.md`.
3. **WP-SIGN: signing and the Chromium sandbox. Size M; needs a SignPath OSS account (user action).**
   - Touches: `release.yml` signing steps; turning the renderer sandbox on with the signed bootstrap (`lightloom-renderer` README); minisign for the Linux tarball.
   - Then: register the `packaging/msix-identity/` package (needs a signed package) for the WinRT LampArray path.
4. **WP-LINUX-RUN: first run on a Linux machine. Size M; needs the user's Linux install.**
   - hidraw, udev rules, `/dev/i2c-*` RAM, hwmon, serial, the systemd user unit, the AppImage, the CEF renderer on a desktop; audio (sink monitor) and screen capture on GNOME and KDE Wayland (the portal dialog, the restore token, SHM buffers) and on X11 (`cargo test -p lightloom-inputs -- --ignored --nocapture pulse x11 portal`); capture a `report_descriptor` per reference device for `fixtures.rs`; a Linux self-hosted conformance runner.

### Still to do: code work

5. **WP-MACROS (b): macro runtime. Done on `feature/macros`** (see the macros row above); left: a hardware check of the hooks and `SendInput` on the user's PC, the missing events and window helpers listed there.
6. **WP-LCD remainder: LCD faces on hardware. Size M; needs an LCD device.**
   - Done: face renderer per device, the `lcd` module and encoders, face selection, starter faces (`docs/host-api/lcd-lightloom.md`).
   - Left: a real LCD device run; the NZXT formats (needs a capture of NZXT CAM and a Kraken Elite/Z3); per-face control values in the dashboard; background layer, rotation, idle blanking; a face preview.
7. **WP-SENSORS-FANS (fan half): dropped.** Decided 2026-10-25: Lightloom does not control fans (no fan API, curves, LpcIO/SuperIO or hwmon `pwm`). Read-only sensors stay.
8. **WP-DTLS: `addFeature("dtls")` and `https`. Done (`feature/dtls`).**
   - Left: a run against a real Hue bridge (pairing with `generateclientkey`, the CLIP v2 area, the PSK identity: application id or username, the stream); see `docs/hardware/philips-hue.md`.
9. **WP-SOURCES: plugin sources manager. Size M. Optional under the standalone direction.**
   - Git or HTTPS sources per `schemas/plugin-source.schema.json`, licence acknowledgement, hash pinning, update only on user action.
10. **WP-LOGS: dashboard log page.** Done (`feature/housekeeping`).
11. **WP-PICKERS: audio output and display pickers in Settings.** Done (`feature/housekeeping`).
12. **WP-L3: nusb on Linux.** Done (`feature/linux-sys`).
13. **WP-L6: PipeWire audio and screen capture.** Done (`feature/linux-inputs`); the Wayland portal path awaits WP-LINUX-RUN.
14. **WP-L8: Linux desktop integration.** Done (`feature/linux-sys`): logind power source, freedesktop notifications, StatusNotifierItem tray.
15. **WP-L7 remainder: release. Size M.**
    - Done (`feature/packaging-linux`): the CEF renderer in the Linux tarball/AppImage (`stage-linux.sh --require-renderer`), the release build as a reusable workflow with a weekly dry run, the Flatpak built in `flatpak.yml`.
    - Left: first dry run and fixes from it; the renderer in the Flatpak (CEF as an offline source, zypak); `cargo dist init` replacing the hand-written workflows (keep cargo-auditable and SBOM).
16. **Docs and tools.** Done (`feature/housekeeping`): `crates/README.md`, `docs/api/compat-openapi.yaml`, `tools/shm-dump`, `tools/pawnio-hash-pin`.

### Suggested order

- **Now (no user needed):** the WP-L7 remainder.
- **Then:** WP-LCD remainder once an LCD device is available.
- **With the user:** WP-SOAK, WP-EVISION-V1, WP-SIGN, WP-LINUX-RUN whenever the hardware, capture, account or machine is available.
