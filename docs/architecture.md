<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# HueDeck architecture (concise)

This is the current, short description of how HueDeck is built. The long form,
including the proposal/judge history, the pinned-crate table with licenses, the
corrected host-API mapping and the decision log D1-D18, is
[architecture-synthesis.md](architecture-synthesis.md) (it uses the placeholder
name "OpenLux" for HueDeck). Individual decisions are recorded in [adr/](adr/).
The host API itself is specified in [host-api/](host-api/).

## 1. Processes

| Process | Privilege | Role |
|---|---|---|
| `huedeckd` | user, unprivileged | Daemon: device registry, plugin host, layouts and sampling, FrameStore, inputs, persistence (SQLite), local API, dashboard serving, renderer and broker supervision. Single instance per user (lock file), autostart entry optional. |
| `huedeck` | user | CLI client of the daemon's API (`devices`, `light`, `rainbow`, `bench`, `compat dry-run`, `record`/`replay`, `import`, `udev`). |
| `huedeck-renderer` | user | CEF windowless Chromium that renders effect HTML at 320x200 and LCD faces; writes frames into shared memory. Also serves as the CEF subprocess executable. Crash is recovered by the daemon within seconds while the last frame is held. |
| `huedeck-iobroker` | **LocalSystem** Windows service, **optional** | Only route to SMBus / GPU-I2C / SuperIO / MSR on Windows, through PawnIO. Installed once by `huedeck-setup` on opt-in. Absent on Linux. |
| `huedeck-setup` | elevated, one-shot | Installs PawnIO, the broker service and WinUSB bindings for user-approved VID:PIDs. The only UAC prompt HueDeck ever shows. |
| `huedeck-ui` | user, optional | Tauri 2 tray shell around the same dashboard the daemon serves on `127.0.0.1:16138`; never on the critical path. |

No custom kernel driver, ever ([ADR-0007](adr/0007-no-custom-kernel-driver.md)).

## 2. Crates

```
huedeck-hal          traits: Hid, Usb, Serial, Smbus, GpuI2c, Sensors, Hotplug, Capture, Audio, LampArray + mock backend + transaction recorder
huedeck-hal-windows  hidapi (hid.dll), nusb/WinUSB, serialport, CM_Register_Notification hotplug, CM_Get_Parent grouping, &colNN endpoint parsing, HidD_GetInputReport, WinRT LampArray, broker client
huedeck-hal-linux    hidapi (hidraw), nusb, serialport, udev monitor, i2cdev, hwmon
huedeck-hid-desc     HID report-descriptor parser -> top-level collections / report ids (same endpoint tuples on both OSes)
huedeck-protocol     broker wire types (serde + postcard), closed op enum, shm frame layout shared with the renderer
huedeck-iobroker     the Windows service: PawnIO client, module hash pins, allowlists, mutex discipline, audit log, fuzz target
huedeck-nvapi        dynamic nvapi64.dll loader (I2C, GPU enumeration, thermal) with self-declared interface ids
huedeck-plugin-host  rquickjs runtime per thread, ES-module resolver/loader, call-if-function exports, device/bus/channel/service host objects, API_GAP trap, watchdog, compat-v1 + native-v2 profiles
huedeck-effects      lightscript <meta> parser (html5ever), effect index, presets, engine-data producer
huedeck-render       RendererClient trait; null renderer; CEF client (shm reader, process supervisor); frame clock
huedeck-renderer     the CEF process: windowless browser, on_paint -> shm, URL filter, engine.* injection, V8 extension (M6), LCD faces
huedeck-inputs       audio FFT (cpal + rustfft -> 200 bins), screen zones (xcap -> 28x20 HSL + 160x100 RGBA), sensor facade
huedeck-engine       registry, layouts, sampling (precomputed per-LED offsets), FrameStore, scheduler, channels/components, fan curves, persistence, event bus, import wizard, macros
huedeck-api          axum: native API :16138 (REST + WS, token), compat :16038/:16034, OpenRGB SDK server :6742 (M6), OpenAPI
huedeck-daemon       huedeckd: lifecycle state machine, power events, single instance, dashboard static files
huedeck-cli          huedeck
huedeck-setup        one-shot elevated installer
```

Plain-JavaScript shims live in `prelude/` (`BinaryUtils`, `Errors.js`, `base64`,
`createColorArray`, the `@SignalRGB/*` and `@huedeck/*` module facades, and the
renderer-side `engine.js`) so plugin authors can fix host gaps without Rust.

## 3. Threads inside huedeckd

- **tokio multi-thread runtime**: HTTP/WS API, sockets for network plugins,
  hotplug notifications, mDNS discovery, persistence writes, renderer and broker
  supervision, event bus.
- **One OS thread per loaded plugin**, owning its `rquickjs::Runtime` + `Context`
  and its device handle (HID/USB/serial). Memory limit 64 MB; interrupt watchdog
  aborts a `Render()` whose script time exceeds 250 ms (host `pause()` and blocking
  reads excluded; `Initialize()` / `Shutdown()` get a seconds-scale budget because
  corpus plugins pause for 1-20 s during setup); three strikes fault the plugin
  and restart it with backoff. Network services get one thread per `DiscoveryService`, and each
  announced controller gets its own device thread.
- **Frame scheduler**: publishes `FrameId` + an atomic sequence number; plugin
  threads wait on a condvar for `seq > last`. `device.pause(ms)` is a thread sleep;
  `setFrameRateTarget(fps)` is a sequence divider.
- **Frame clock**: when no renderer is active (Forced colour, headless, M1) a
  high-resolution waitable timer thread (`CreateWaitableTimerExW` with
  `HIGH_RESOLUTION`; `timerfd` on Linux) bumps the sequence at the configured fps.
- **Input threads**: audio capture callback -> 2048-point FFT -> 200 log bins at
  60 Hz; screen capture 10-30 Hz -> 28x20 HSL + 160x100 RGBA; sensor poller 1 Hz.

Lifecycle: `Booting -> LoadingConfig -> BrokerProbe -> RendererProbe ->
ScanningHardware (hid, usb, serial, smbus, lamparray, sensors) -> LoadingPlugins
(index cache: only modules whose VID:PID is present are evaluated) ->
LoadingEffects -> Running`, with `Suspending / Resuming` on power events
(`Shutdown(true)` per plugin, re-`Initialize()` about 3 s after resume). Shutdown
order: stop the sequence, `Shutdown(false)` per plugin with a 2 s deadline, close
handles, stop the renderer. Device state captured at `Initialize()` is restored at
exit where the plugin supports it.

## 4. Frame pipeline

```
effect.html in CEF ---on_paint (30 fps)---> shm ring: 3 x 320x200 BGRA slots, header {magic, version, w, h, seq, per-slot generation stamps}
                                           + engine-data region {audio 203 f32, zone 3x560 f32 + 160x100x4 u8, sensors}
      | control pipe: new seq (postcard)
      v
huedeckd FrameStore (triple buffer + seq) --condvar--> plugin thread N:
      sample LEDs through precomputed Int32 offset table (recomputed only on layout change, no allocation)
      -> apply global x layout x device brightness, gamma, LightingMode (Canvas | Forced)
      -> call plugin Render()  -> device.color(x,y) / channel(name).getColors(...) answer from the sampled grid
      -> HID / USB / serial / broker write on the same thread
```

- Canvas is 320x200, origin top-left, identical to the upstream layout model so
  imported layouts transfer. Each endpoint (device, subdevice, channel, component
  instance) has a placement `{x, y, scale, rotation, flipX, flipY, enabled,
  brightness, lightingMode, forcedColor}`.
- Slow devices skip to the latest frame; nothing queues. Target canvas-to-wire
  p50 <= 10 ms, p99 <= 25 ms with four devices; `huedeck bench` prints histograms.
- If `on_paint` cadence stays below 20 fps for 60 frames the engine downshifts to
  15 fps and warns. The renderer composites in software by default (steadier
  paint cadence at 320x200, works in VMs and RDP); GPU compositing is opt-in.
- Effect load sequence: page scripts run -> engine injects `var <prop> = <value>`
  for every `<meta property>` -> every `on<prop>Changed()` called once ->
  `onEngineReady()` -> `requestAnimationFrame` loop. `engine.audio.*`,
  `engine.zone.*`, `engine.vision.*` (area/linear meters only; OCR out of scope),
  `engine.getSensorValue()` are pushed as JSON in M3 and read from shm by a V8
  extension in M6.
- Soft real-time only: occasional 30-50 ms spikes from Chromium style recalcs are
  expected; not suitable for sub-millisecond synchronisation.

## 5. Plugin host and compatibility

- Plugins are real ES modules loaded through a custom resolver: bare
  `@SignalRGB/*` (accepted verbatim for interoperability) and `@huedeck/*` map to
  shim modules; `./X.js` resolves inside the plugin directory; `export default
  class` is supported for discovery services. Metadata exports are
  call-if-function; lifecycle exports (`Validate`, `Initialize`, `Render`,
  `Shutdown`, `Scan`, `on<Prop>Changed`) are invoked by the host.
- Endpoint model: per physical device (grouped by USB parent) a list of
  `{interface, usage, usage_page, collection, length}`; `collection` is the
  Windows `&colNN` index and is reproduced on Linux by `huedeck-hid-desc` so
  `Validate()` sees identical tuples on both OSes. `Validate` runs per endpoint,
  first truthy wins; `set_endpoint` switches among the same list.
- `ControllableParameters` become bare globals in the plugin context; changes set
  the global and call `on<Prop>Changed`.
- Unknown host members: log `API_GAP:<plugin>:<member>` once, then throw a typed
  error. `huedeck compat dry-run <dir>` runs a whole corpus and reports gaps.
- Capability by `Type`: HID plugins receive only their matched handles; network
  plugins only sockets (default UDP binds opened only while active); SMBus plugins
  only the scanned bus plus a `Scan()`-derived write allowlist.
- First-party plugins (MIT, `plugins/`) are written in the compat shape and loaded
  by the same code path as imported ones.

Details: [ADR-0003](adr/0003-rquickjs-plugin-engine.md),
[ADR-0005](adr/0005-signalrgb-compat-profile.md), [host-api/](host-api/).

## 6. HAL

Traits in `huedeck-hal`, selected by `cfg(target_os)`, with a `mock` backend used
by CI, the Node simulator and golden-packet tests from M0.

| Need | Windows | Linux (M7) |
|---|---|---|
| HID | hidapi `windows-native`; parent association via `CM_Get_Parent`; hotplug via `CM_Register_Notification` (debounced 500 ms); `HidD_GetInputReport` for `input_report` | hidapi `linux-native` (hidraw) + `huedeck-hid-desc`; udev monitor; `HIDIOCGINPUT` |
| raw USB | `nusb` over WinUSB (bound by `huedeck-setup` for approved VID:PIDs) | `nusb`, driverless |
| serial | `serialport` | `serialport` + dialout group |
| SMBus / SuperIO / MSR | broker -> PawnIO (`SmbusPIIX4`, `SmbusI801`, `LpcIO`, MSR), hashes pinned | `i2cdev` on `/dev/i2c-*`; hwmon for fans and temps: out-of-tree `nct6687` for the NCT6687D on the reference board (mainline `nct6683` exposes PWM writes only on Mitac boards), `nct6775` / `it87` elsewhere |
| GPU I2C | `huedeck-nvapi` (dynamic `nvapi64.dll`) inside the broker; ADLX later | GPU buses appear as `/dev/i2c-*` |
| sensors | broker MSR/NvAPI thermal; optional LibreHardwareMonitor HTTP JSON | hwmon |
| screen / audio | `xcap` (WGC) / `cpal` (WASAPI loopback) | PipeWire portal or X11 / PipeWire or Pulse monitor |
| Dynamic Lighting | WinRT `LampArray` consumed as a generic backend (certified keyboards light without a plugin) | n/a |

udev rules are generated by `tools/gen-udev` from loaded plugin VID:PID lists
(`TAG+="uaccess"`, `GROUP="i2c"` on i2c nodes).

## 7. Persistence and local API

- `state.sqlite` (rusqlite, WAL, `rusqlite_migration`) in the user data directory,
  tables mirroring the upstream key shapes so the import wizard is a 1:1 mapping;
  `config.toml` for static settings; JSON export/import. No registry writes;
  the import wizard reads HKCU once with consent ([ADR-0008](adr/0008-persistence-sqlite.md)).
- Native API `127.0.0.1:16138`: REST + WS event stream; mutations need a bearer
  token from a file ACL'd to the user; OpenAPI spec in `schemas/`.
- Compat API `:16038` (`/api/v1/lighting`, `/effects`, `/scenes`, ...) and `:16034`
  (`POST /canvas/event`) with shape-copied responses; refuses to bind when another
  application owns the ports; mutating calls need the token by default.
- M6: OpenRGB SDK **protocol** server on `:6742` (wire protocol only, no code
  linking).

## 8. Security

- Privilege split: daemon, renderer, CLI and UI as the user; the broker is the only
  privileged component, optional, < 2k LOC, closed op enum, range checks,
  single-threaded, every SMBus transaction under `Global\Access_SMBUS.HTP.Method`,
  Scan()-derived allowlists, SPD write denial, rate limit, dry-run, kill switch,
  audit log, fuzzed parser ([ADR-0004](adr/0004-localsystem-iobroker-pawnio.md)).
- Plugins: no fs/net primitives unless injected; memory cap; watchdog; network only
  through host modules with per-plugin consent and a host allowlist.
- Effects: Chromium sandbox on; URL filter restricted to the effect directory; no
  clipboard; only `engine` is exposed.
- Sources: hash-pinned, user-triggered updates with a diff summary, per-repo
  license acknowledgement, ed25519-signed first-party index, imports marked
  `unsigned`.
- Supply chain: `cargo-deny` (permissive licenses only, GPL denied in-tree,
  `winring0` banned), `cargo-auditable`, CycloneDX SBOM, reproducible
  `cargo-dist` builds, Authenticode via SignPath OSS (planned). No telemetry.
- Conflicting RGB software is reported with a "close" button, never killed.

Reporting: [SECURITY.md](../SECURITY.md). Clean-room rules:
[CONTRIBUTING.md](../CONTRIBUTING.md).

## 9. Explicitly not copied from the reference application

Custom kernel driver with image-signature lock, LocalSystem process-killer
service, cloud accounts / Pro gating / payments / analytics / crash upload,
Squirrel launcher, registry persistence, marketplace, OCR meters (phase 2 at most),
MCP server (optional later).
