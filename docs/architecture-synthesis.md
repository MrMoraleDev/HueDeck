<!-- Architecture synthesis from a 3-proposal / 3-judge panel, 2026-10-17. The placeholder name 'OpenLux' (openluxd, luxctl) in this text means Lightloom (lightloomd, lightloom). -->
# OpenLux (placeholder name) — synthesized architecture

Base: **OpenLux (Architect A, Rust core)** — highest combined score across the three judges (218 vs GlowRGB 213 vs LumenRGB 199) and the only proposal that scored well on both least-privilege/Linux-readiness and real-time behaviour. Every fatal flaw the judges raised against it is resolved below by grafting from B and C:

| Judge flaw against A | Resolution (source) |
|---|---|
| From-scratch Canvas2D on tiny-skia + non-JIT QuickJS cannot run HTML lightscripts faithfully | **tiny-skia renderer dropped.** Effects run in real Chromium via **CEF offscreen rendering** (`cef` crate 154.5.0+154.0.34, Apache-2.0/MIT, verified 2026-10-17) in a separate `openlux-renderer` process — the Ultralight/Electron OSR model (C), without Electron as the daemon runtime. |
| Host API table not derived from corpus (`getColors('Separate')`, positional `set_endpoint`, `collection` semantics, exports-as-functions, missing `getHidEndpoints/productId/addProperty/…`) | Corrected table in §4 using the semantics the judges verified in C's proposal; corpus dry-run harness on day one of M2 (C); `API_GAP` trap that **logs then throws** (B, with judge 1's correction). |
| `collection` modelled as enumeration ordinal | Windows: parse `&colNN` from the HID device-interface path per top-level collection; Linux: own report-descriptor parser (`openlux-hid-desc`) splits hidraw nodes into TLCs in descriptor order so both OSes produce the same `{interface, usage, usage_page, collection}` tuple. Verified against the Razer DeathAdder V2 Pro (multi-TLC) in M2. |
| Broker privilege understated (PawnIO is admin-only, verified against OpenRGB SMBusAccess.md) | Broker is a **Windows service running as LocalSystem**, installed once (UAC) only when the user opts into SMBus/GPU-I2C/SuperIO. No per-session UAC (resolves B's flaw), no unprivileged PawnIO access (resolves C's flaw). Linux: no broker (i2c-dev + udev + hwmon). |
| hidapi feature name | `hidapi 2.6.7` with `windows-native` / `linux-native` (verified on docs.rs). |

## 1. Component diagram

```
+--------------------------------------------------------------------------------------------+
| Dashboard (served by daemon at 127.0.0.1:16138; opened in any browser or in the optional   |
| Tauri 2 tray shell `openlux-ui`)           | third-party tools (compat :16038 / :16034)   |
+------------------------+-------------------+-----------------------+------------------------+
                         | HTTP/WS loopback (native API token-authed; compat API shape-copied)
+------------------------v-------------------------------------------------------------------+
| openluxd  (per-user, UNPRIVILEGED daemon, tokio)                                           |
|  openlux-api (axum) -> openlux-engine                                                      |
|     registry | layout+sampling | FrameStore (triple buffer, seq) | scheduler | fan curves   |
|     persistence (SQLite) | presets/layouts | event bus | import wizard | plugin index cache  |
|        |                       |                           |                                |
|  openlux-plugin-host      openlux-inputs             openlux-render (client side)           |
|  1 rquickjs rt per plugin  audio (cpal+rustfft)       RendererClient trait:                 |
|  thread; device.*/bus.*    screen (xcap)              - null (solid/rainbow/test; M1, CI)  |
|  @SignalRGB/* shims (JS)   sensors (broker/hwmon/LHM) - cef (shm frames from renderer proc)|
|  compat-v1 + native-v2     - frame clock (hi-res waitable timer when no canvas)             |
|        |                                                                                   |
|  openlux-hal (traits + mock)                                                               |
|    hal-windows: hidapi(hid.dll) | nusb(WinUSB) | serialport | CM_Register_Notification      |
|                 CM_Get_Parent assoc | LampArray(WinRT) | broker client                      |
|    hal-linux:   hidapi(hidraw) | nusb | serialport | udev | i2cdev | hwmon                   |
+----------+-------------------------------------------+-------------------------------------+
           | shm (3x 320x200 BGRA + engine data) + seq  | named pipe (Win) / unix socket
           | over control pipe                          | postcard frames, closed op enum
+----------v-------------------------+   +--------------v------------------------------------+
| openlux-renderer (user, CEF OSR)   |   | openlux-iobroker (Windows SERVICE, LocalSystem;   |
|  windowless browser 320x200 @30fps |   |  optional, installed on opt-in; Linux: not needed) |
|  on_paint -> shm slot -> seq++     |   |  PawnIO client (\\.\PawnIO; SmbusPIIX4/SmbusI801/  |
|  engine.* injected (JSON push M3,  |   |  LpcIO/MSR modules, SHA-256 pinned; refuses        |
|  V8 extension over shm M6)         |   |  Unrestricted builds), NvAPI I2C (dynamic         |
|  URL filter: file:// effect dir +  |   |  nvapi64.dll, self-declared IDs), ADLX later,      |
|  openlux:// prelude only           |   |  named-mutex discipline, address allowlists        |
|  1 extra browser per LCD face      |   |  (<2k LOC, fuzzed parser, audit log)               |
+------------------------------------+   +---------------------------------------------------+
         CEF subprocesses (Chromium sandbox on)        PawnIO.sys (signed, GPL-2 + IOCTL exception)
```

No custom kernel driver, ever. HID/USB/serial users never install anything privileged.

## 2. Pinned crates / packages (crates.io, verified 2026-10-17 by judges unless marked)

| Role | Crate | Pin | License |
|---|---|---|---|
| async runtime | tokio | 1.53.3 | MIT |
| HTTP/WS | axum 0.8.9, tower-http, tokio-tungstenite | — | MIT |
| JS engine (plugins) | rquickjs (QuickJS-NG) | 0.14.0 | MIT |
| effect renderer | cef (tauri-apps/cef-rs) | 154.5.0+154.0.34 (verified today; CEF binaries fetched by `cef-dll-sys` build script or `export-cef-dir`, pinned by the `+154.0.34` metadata) | Apache-2.0 OR MIT (CEF itself BSD-3) |
| HID | hidapi | 2.6.7, features `windows-native` / `linux-native` | MIT |
| raw USB | nusb | 0.2.8 | MIT/Apache |
| serial | serialport | 4.10.1 | MPL-2.0 |
| Linux I2C | i2cdev | 0.6.2 | MIT/Apache |
| Windows APIs | windows | 0.62.2 | MIT/Apache |
| Windows service | windows-service | 0.8.x | MIT/Apache |
| frame buffer | triple_buffer 9.0.0 (verified today) + arc-swap 1.x | — | MPL-2.0 / MIT-Apache |
| screen capture | xcap | 0.9.8 | Apache-2.0 |
| audio | cpal 0.18.2 + rustfft 6.x | — | Apache-2.0 / MIT-Apache |
| system info | sysinfo | 0.39.6 | MIT |
| mDNS | mdns-sd | 0.21.5 | MIT/Apache |
| persistence | rusqlite 0.40.2 (`bundled`) + rusqlite_migration | — | MIT |
| UI shell (optional) | tauri | 2.12.4 | MIT/Apache |
| IPC encoding | postcard 1.x + serde 1 | — | MIT/Apache |
| images | image 0.25 | — | MIT/Apache |
| HTML meta parsing | scraper 0.2x (html5ever) | — | MIT/Apache |
| signing | ed25519-dalek 2.x | — | BSD-3 |
| HTTP client (XmlHttp) | reqwest 0.12 (blocking facade on plugin thread) | — | MIT/Apache |
| CLI | clap 4.x | — | MIT/Apache |
| logging | tracing + tracing-appender | — | MIT |
| FFI we write | openlux-nvapi (dynamic load of nvapi64.dll, self-declared QueryInterface IDs, no NVIDIA headers — B's approach), openlux-adlx later (ADLX MIT) | — | — |
| supply chain | cargo-deny, cargo-dist, cargo-auditable; WiX MSI; Authenticode via SignPath OSS | — | — |

Rejected: tiny-skia Canvas2D shim (fidelity/scope), deno_core/V8 for plugins (unneeded; `ScriptEngine` trait keeps the door open), QJSEngine (would force Qt), Electron as daemon runtime (RSS, 8-week churn, N-API prebuild treadmill), chromiumoxide/CDP (external browser + screenshot transport), Ultralight (proprietary), WinRing0/inpoutx64/any self-written .sys.

## 3. Process and thread model

**openluxd** (per-user, autostart via Startup/Run key; single instance via lock file):
- tokio multi-thread runtime: API, sockets, hotplug events (`CM_Register_Notification`, no window needed), discovery, persistence, renderer supervision.
- **One OS thread per loaded plugin** owning its `rquickjs::Runtime + Context`; the plugin's HID/USB/serial handle lives on that thread. `set_memory_limit(64 MB)`, `set_interrupt_handler` watchdog: `Render()` > 250 ms aborted; three strikes → plugin faulted, device shown in error, restart with backoff (A).
- **Frame loop** (C's SAB/Atomics design, in-process): scheduler publishes `FrameId` + atomic seq; each plugin thread waits on a condvar for `seq > last`, samples its **precomputed per-LED pixel-offset table** (recomputed only on layout change, never allocates), applies brightness/gamma/LightingMode, then calls `Render()`. Slow devices skip to the latest frame; no queue growth. `device.pause(ms)` = thread sleep; `setFrameRateTarget` = seq divider.
- **Frame clock**: when no renderer is active (Forced mode, headless), a high-resolution waitable timer thread (`CreateWaitableTimerExW` HIGH_RESOLUTION; Linux timerfd) bumps seq at the configured fps (C).
- Inputs threads: audio capture callback → 2048-pt FFT → 200 log bins at 60 Hz; screen capture at 10–30 Hz → 28x20 HSL + 160x100 RGBA; sensor poller 1 Hz.
- Lifecycle: `Booting → LoadingConfig → BrokerProbe → RendererProbe → ScanningHardware(hid, usb, serial, smbus, lamparray, sensors) → LoadingPlugins(index cache: only import() modules whose VID:PID is present — B) → LoadingEffects → Running`, plus `Suspending/Resuming` on `WM_POWERBROADCAST` (calls `Shutdown(true)`, re-`Initialize()` 3 s after resume). Shutdown order: stop seq → `Shutdown(false)` per plugin with 2 s deadline → close handles → stop renderer.

**openlux-renderer** (user privileges; our own exe used also as the CEF subprocess via `execute_process`): CEF with `windowless_rendering_enabled`, `windowless_frame_rate` = configured fps (30 default, up to 60); `RenderHandler::on_paint` copies the BGRA 320x200 buffer into the next shm slot (header: magic, version, w, h, seq, per-slot generation stamps before/after write for torn-read detection; 3 frame slots of 256 KB; engine-data region: audio 203 f32, zone 3x560 f32 + 160x100x4 u8, sensors table) and sends the new seq on the control pipe; the daemon reads 30 tiny messages/s and publishes. The daemon restarts a crashed renderer and holds the last frame meanwhile. `--software-compositing` fallback for VMs/RDP; if `on_paint` cadence drops below 20 fps for 60 consecutive frames the engine downshifts to 15 fps and raises a UI warning (B). One additional windowless browser per LCD face at the face's own resolution.

**openlux-iobroker** (Windows service, LocalSystem, optional): single-threaded request loop over a named pipe whose DACL is limited to the interactive user SID; per-boot token in a file ACL'd to the user; op enum ≈ 12 ops; every argument range-checked; **every SMBus transaction is wrapped in `Global\Access_SMBUS.HTP.Method`** (not only inside `TransactionLock`) so RAM and GPU plugins cannot interleave and HWiNFO/OpenRGB coexist; single-threadedness serializes all plugins by construction. Idle after startup costs nothing; no UAC per session.

**openlux-ui** (optional Tauri 2 tray shell): hosts the same dashboard the daemon serves; no privileged logic. The dashboard is usable in a plain browser so the UI shell is never on the critical path.

## 4. Plugin host: SignalRGB-compatible profile (`compat-v1`), corrected

**Strategic answer:** implement a compatible JavaScript host as a *profile* of our own richer plugin format. The ~560 plugin files and the HTML lightscripts run from the **user's own copies** (import wizard points at the user's SignalRGB install/`Documents\WhirlwindFX`, copies into the user data dir, marks them `imported/unsigned`); **no WhirlwindFX file is ever in our repo or installer**. Our repo ships MIT first-party plugins written in the same shape, a frozen MIT **Host API v1 (2.5.80-era) spec** derived from how plugins *use* the API (readable source, clean-room, no decompiled code), and a conformance suite. Community repos (SRGBmods etc.) are consumed as plugin sources only when their own license allows; the source manager records `license_text_acknowledged_at`. "SignalRGB" appears only nominatively ("compatible with"); the `@SignalRGB/*` specifiers are accepted verbatim for interop and aliased as `@openlux/*` for native plugins (user decision 5).

**Loading:** real ES modules via rquickjs `Resolver/Loader` (bare `@SignalRGB/*` → Rust/JS shim modules; relative `./X.js` resolved inside the plugin's directory; `export default class` for `DiscoveryService`). After evaluation the namespace is read. **Metadata exports are call-if-function**: `VendorId, ProductId (int | int[] | object → keys), Name, Publisher, Documentation, Size, DefaultPosition, DefaultScale, LedNames, LedPositions, Type ('Hid' default|'rawusb'|'hybrid'|'serial'|'SMBUS'|'network'), DeviceType, ControllableParameters, ConflictingProcesses, SubdeviceController, DefaultComponentBrand, SupportsFanControl, UsesCorsairMutex, ImageUrl/Image, DeviceMessages (verify)`. **Lifecycle exports are invoked by the host**: `Validate(endpoint)`, `Initialize()`, `Render()`, `Shutdown(SystemSuspending)`, `Scan(bus)`, `DiscoveryService`, `on<Prop>Changed()`. ControllableParameters (validated against our JSON schema) become bare globals in the plugin context; changes set the global and call `on<Prop>Changed`.

**Endpoint model:** per physical device (grouped by USB parent via `CM_Get_Parent` / udev), a list of `{interface, usage, usage_page, collection, length}`; Windows `collection` = `&colNN` index from the HID interface path (0/1-based to be confirmed in M2 against corpus values); Linux computed by `openlux-hid-desc` from `HIDIOCGRDESC`. `Validate` is called per endpoint; first truthy wins; `set_endpoint` switches among the same list.

| Host call (compat-v1) | Native implementation |
|---|---|
| `device.write(buf, len)` | HID: `write` on active endpoint (byte 0 = report id), reusable per-length buffer, pad/truncate to `len`; rawusb: interrupt/bulk OUT on selected ep; serial: port write |
| `device.send_report(buf,len)` / `get_report(buf,len)` | `send_feature_report` / `get_feature_report` (buf[0] = report id) |
| `device.read(buf,len,timeout=0)` / `getLastReadSize()` / `clearReadBuffer()` | `read_timeout`; last size recorded (0 on timeout); drain with 0 ms reads |
| `device.input_report(id,len)` | `HidD_GetInputReport` via `windows` crate on Windows (hidapi native backend lacks it); Linux `HIDIOCGINPUT` |
| `device.getHidEndpoints()` | returns the endpoint list above |
| `device.set_endpoint(interface, usage, usage_page, collection?)` | **positional**; re-opens the matching path; `collection` optional wildcard |
| `device.productId()` / `vendorId()` / `getDeviceInfo()` | matched PID/VID, path, serial |
| `device.bulk_transfer(ep,buf,len,timeout)` / `control_transfer(bmRequestType,bRequest,wValue,wIndex,data,len,timeout)` | `nusb` interface claimed at load (`rawusb`/`hybrid`), blocking wrapper with timeout; WinUSB bound by the one-shot setup tool |
| `device.flush()` / `pause(ms)` | serial flush / HID no-op; `thread::sleep` bounded 2000 ms (logged > 200 ms) |
| `device.log(msg, {toFile})` / `notify(title, body, type)` / `console.*` | `tracing` span per plugin, per-plugin log file, event bus → UI toast / tray notification |
| `device.color(x,y)` → `[r,g,b]` | fresh 3-array from the per-device sampled grid (Canvas) or forced color (Forced); global brightness, per-device gamma/white point applied |
| `device.subdeviceColor(id,x,y)` | same on the subdevice's placement |
| `createSubdevice/removeSubdevice/setSubdeviceName/Size/Leds(LedNames,LedPositions)/Image` | registry child entries with own placement + sample table |
| `addChannel(name, ledLimit, ledCountDefault?)` / `SetLedLimit` / `channel(name).getColors(mode)` | channel = ordered user-attached components (same JSON schema as SignalRGB components); `'Inline'` → `[r,g,b,r,g,b,…]`; **`'Separate'` → `[[r…],[g…],[b…]]`** (three parallel arrays, verified in corpus) |
| `channel(name).LedCount()/ledCount` / `getChannelPulseColor(n)` / `createColorArray(c,n,mode)` | host-side; pulse = sine on host clock |
| `setName/setSize/setControllableLeds/setImageFromUrl` | registry update + event |
| `addProperty(obj)` / `getProperty(name)` / `removeProperty(name)` | dynamic ControllableParameters (same schema), persisted per device |
| `addFeature('keyboard'|'mouse'|'battery'|'dtls'|'base64')` | capability flags: keyboard/mouse → input-report subscription for macros; battery → `setBatteryLevel` badge; dtls → DTLS client in net module; base64 → shim |
| `createFanControl(name)` / `setRPM` / `getFanLevel` / `getNormalizedFanlevel` / `fanControlDisabled()` / `createTemperatureSensor` / `SetTemperature` | fan/sensor registry; curves evaluated in Rust; plugin is the actuator; values exposed as `engine.getSensorValue` |
| `setFrameRateTarget(fps)` | seq divider for this plugin thread |
| `bus.WriteByte/ReadByte/WriteWord/ReadWord/WriteBlock/ReadBlock/ReadBlockBytes/WriteQuick/WriteByteWithoutRegister/ReadByteWithoutRegister` | `SmbusBus` trait → Windows broker RPC → PawnIO module fn; Linux `i2cdev` ioctls; address 0x03–0x77; block ≤ 32; SPD EEPROM range 0x50–0x57 and write-protect 0x30–0x37 denied by default |
| `bus.GetAddress()/IsSystemBus()/IsNvidiaBus()/IsAMDGpuBus()/Vendor()/SubVendor()/Product()/SubDevice()` | bus descriptor (PCI ids via SetupDi / sysfs) |
| `bus.TransactionLock()/TransactionUnlock()` | process-local mutex + named mutex held across the block; broker already holds the mutex per op |
| `Scan(bus)` | called once per bus; addresses that answer become that plugin's **write allowlist** (B) |
| `@SignalRGB/udp`, `/tcp` | tokio sockets with sync facade on the plugin thread; default binds 5353/4001/4002/1982/5555/12345 opened only while a network plugin is active |
| `@SignalRGB/serial` | `serialport` handle |
| `@SignalRGB/lcd` | frame sink from the renderer's LCD-face browser (RGB565/JPEG as requested) |
| `@SignalRGB/permissions`, `/systeminfo`, `/appInfo`, `/performance`, `/base64`, `/DeviceDiscovery`, `Errors.js` | small Rust-backed or pure-JS modules; `appInfo` reports our version + `compat: 'signalrgb-2.5'`; `permissions` = persisted per-plugin consent |
| `controller` / `discovery` / `service` globals (`Type:'network'`) | one runtime per DiscoveryService (`Initialize/Update/Discovered/Removed/Shutdown`); `service.addController/announceController/removeController/hasController/log` spawn/destroy device threads whose `controller` global is the announced object |
| `XmlHttp.Get/Post(url, cb, headers?)` | `reqwest` blocking facade; http(s) only; per-plugin host allowlist; RFC1918/loopback denied unless declared |
| `BinaryUtils` | pure-JS prelude implementing the surface the corpus uses (LE/BE packing, CRC, hex) |
| `ConflictingProcesses` | shown in UI with a "close" button; **never killed silently** (B) |
| unknown member on `device`/`bus`/`channel` | Proxy trap logs `API_GAP:<plugin>:<member>` once, then **throws** a typed `HostApiMissing` error (config `compat.unknown_member = throw|noop`) |

The JS side of the shims (`prelude/*.js`: BinaryUtils, createColorArray, Errors.js, base64, module facades) is plain JavaScript in-repo so plugin authors can fix host gaps without Rust.

**Native-v2 profile** (superset): `manifest.json` (id, version, SPDX license, devices[], permissions[]), async I/O variants, `device.onInput(cb)`, `openlux.d.ts`, Node-based simulator with mock HAL recording transactions for golden tests, capability manifest by Type (HID plugins only get their matched handles; network plugins only sockets; SMBUS plugins only the scanned bus).

## 5. HAL per OS

Traits in `openlux-hal`: `HidBackend/HidHandle`, `UsbBackend/UsbHandle`, `SerialBackend`, `SmbusBackend`, `GpuI2c`, `SensorBackend`, `HotplugSource`, `CaptureBackend`, `AudioBackend`, `LampArrayBackend`; selected by `cfg(target_os)`; `mock` backend used in CI and the simulator from M0.

**Windows (now):** hidapi `windows-native`; parent association via `CM_Get_Parent`/`SetupDiGetDeviceProperty(DEVPKEY_Device_Parent)`; hotplug via `CM_Register_Notification` (HID + USB device interface GUIDs) debounced 500 ms; rawusb via `nusb` (WinUSB bound by `openlux-setup` for user-approved VID:PIDs); serial via `serialport`; SMBus/SuperIO/MSR via broker → PawnIO (`SmbusPIIX4` for this AMD B550 at 0xB00/0xB20, `SmbusI801`, `LpcIO`; module hashes pinned; "Unrestricted" builds refused); GPU I2C via NvAPI `I2CReadEx/WriteEx` (dynamic) and ADLX later; sensors via broker MSR/NvAPI thermal plus optional LibreHardwareMonitor HTTP JSON source (C); screen via WGC (`xcap`), audio via WASAPI loopback (`cpal`); **Dynamic Lighting consumed** as a generic `LampArray` HAL backend via WinRT (B) — DL-certified keyboards light without a plugin; export to Windows is impossible from user mode and is not attempted.

**Linux (M7, abstractions from M0):** hidapi `linux-native` (hidraw) + `openlux-hid-desc` for TLC parity; `tools/gen-udev` emits `70-openlux.rules` (`TAG+="uaccess"` per VID:PID from loaded manifests, `KERNEL=="i2c-[0-9]*", GROUP="i2c"`); `nusb` driverless; `serialport` + dialout; SMBus via `i2cdev` (NVIDIA/AMD GPU buses appear as `/dev/i2c-*`); SuperIO through hwmon (`nct6775`, `it87`) — `createFanControl` writes `pwmN`; hotplug via udev monitor; capture via PipeWire portal / X11; audio via PipeWire/Pulse monitor; CEF Linux with `--ozone-platform=headless` for CI; `systemd --user` unit; AppImage + Flatpak.

## 6. Effect ("lightscript") pipeline

1. `openlux-effects` scans effect dirs, parses `<meta>` tags with html5ever at index time (title/description/publisher; `property/label/type/min/max/step/default/values`; `zone`/`audio`/`vision` declarations) → control model persisted per effect; presets = JSON of control values.
2. Renderer loads `file:///…/effect.html` in the windowless browser with a URL request filter (only the effect's directory and `openlux://prelude`); injects the `engine` global (`audio.{freq[200],level,width,density}`, `zone.{hue,saturation,lightness}[560]` + `imagedata` 160x100 RGBA, `vision.<meter>` (area/linear meters via pixel stats in M6; OCR out of scope), `getSensorValue(name)`), control globals, `on<Prop>Changed`, `onCanvasTapped(x,y)` (UI preview), `onCanvasApiEvent({sender,event})` (:16034). M3 pushes engine data via `ExecuteJavaScript` JSON at the input rates; M6 replaces that with a CEF V8 extension in the render process that reads the shm region directly (zero-copy for the 64 KB zone image).
3. `on_paint` at 30 Hz → shm slot → seq → daemon FrameStore → plugin threads sample (2x2 box or bilinear, configurable) → `Render()` → HID writes on each plugin thread.
4. Fidelity harness: renders 60 frames of each of 20+ community effects and compares against a reference capture from the same effect in a stock Chromium (`PSNR ≥ 40 dB` expected since both are Chromium); frame profiler histograms (`luxctl bench`) from M1.

Estimated budget (320x200): Chromium JS + raster 0.5–5 ms (per-pixel effects 5–15 ms, in parallel), `on_paint` copy ~0.1 ms, shm publish < 0.05 ms, wake 5–50 µs, sampling ~0.3 µs/LED, `Render()` 0.05–1 ms (QuickJS; per-LED loops of a few hundred entries), HID write 0.3–2 ms per report. Target end-to-end p50 ≤ 10 ms, p99 ≤ 25 ms; RSS ≈ 40 MB daemon + 150–250 MB renderer (CEF).

**Layout/sampling model:** canvas 320x200, origin top-left, identical to SignalRGB so imported layouts transfer. Placement `{x, y, scale, rotationDeg (0/90/180/270 + free), invertX, invertY, enabled, lightingMode: Canvas|Forced, forcedColor}` per device/subdevice/channel; `LedPositions` in `Size` grid units transformed once into an `Int32` offset table; components (ARGB headers) use `LedCoordinates`/`LedMapping` concatenated in channel order; multiple named layouts switchable via `layout/apply/<name>`.

## 7. Persistence

`%LOCALAPPDATA%\OpenLux` / `$XDG_CONFIG_HOME/openlux`: `config.toml`; `state.sqlite` (WAL; tables `devices, device_settings, placements, channel_components (per-channel JSON arrays mirroring SignalRGB's HKCU shape), layouts, effects, effect_params, presets, macros, plugin_sources(kind, url|path, pinned_sha256, license_text_acknowledged_at), kv`; migrations via `rusqlite_migration`); `plugins/{firstparty|imported|community}/`, `effects/…`, `components/*.json` (SignalRGB-compatible schema), `cache/plugin-index.json` (file hash → exports), `logs/`. Export/import = JSON files (`.luxlayout`, `.luxpreset`). No registry writes; the import wizard reads HKCU once with consent.

## 8. Local API

- Native: `127.0.0.1:16138` REST + WS event stream, mutations require bearer token from a file in the config dir (UI reads it); OpenAPI spec in `schemas/`.
- Compat (default on when ports are free; **refuse to bind and tell the user if SignalRGB is running** — C): `:16038` `GET/PATCH /api/v1/lighting`, `GET /api/v1/lighting/effects`, `POST /api/v1/lighting/effects/{id}/apply`, `GET/POST /scenes`, plus `/api/v1/devices`, `/api/v1/health`; `:16034` `POST /canvas/event?sender=&event=`. GET open on loopback as upstream; mutating compat calls require the token unless `compat.allow_unauthenticated_write=true` (resolves B's flaw).
- M6: OpenRGB SDK protocol **server** on `:6742` (wire protocol only, no linking) so OpenRGB-aware tools can drive our devices; optional OpenRGB SDK **client** bridge for devices only OpenRGB supports (B), marking claimed VID:PIDs so HID is not double-opened.

## 9. Security model

- No custom kernel driver; PawnIO only, optional, pinned module hashes, Unrestricted builds refused.
- Privilege split: daemon, renderer, UI as the user; broker is a < 2k LOC LocalSystem service with a closed op enum, argument range checks, Scan()-derived address allowlists, SPD write denial, 2000 ops/s rate limit, dry-run mode, per-plugin kill switch, audit log, fuzzed parser.
- Plugins are untrusted-ish code: QuickJS has no fs/net primitives unless injected; memory cap 64 MB; interrupt watchdog; network only through host modules with per-plugin consent (`@SignalRGB/permissions`) and host allowlist; HID plugins only receive their matched handles.
- Effects: Chromium sandbox on; URL filter denies everything outside the effect dir; no clipboard; no daemon access except `engine`.
- Sources: hash-pinned, updated only on user action with a diff summary; project ed25519-signed index for first-party packs; imported WhirlwindFX files marked unsigned.
- Supply chain: `cargo-deny` (licenses: MIT/Apache/BSD/MPL-2.0 allow, GPL deny), `cargo-auditable`, CycloneDX SBOM, reproducible `cargo-dist` builds, Authenticode via SignPath OSS, WiX MSI, winget manifest. No telemetry; diagnostics export is manual.
- Clean-room policy in `CONTRIBUTING.md`: protocol facts may come from the readable plugin corpus, public docs and own captures; no decompiled code, no OpenRGB (GPL-2) code paste; Pawn modules we author live in a separate GPL-2 repo (PawnIO's exception does not cover the Pawn side).

## 10. Honest real-time statement

Soft real-time, on par with SignalRGB: frame drops are skips, never backlog; p50 ≈ 8–10 ms, p99 ≈ 25 ms canvas-to-wire; occasional 30–50 ms spikes from Chromium style recalcs. Not suitable for sub-millisecond sync; the docs will say so and `luxctl bench` prints the histograms.

## Decision log

- D1 Base proposal = OpenLux (Architect A, Rust core). Highest combined judge score (218/300) and best on least-privilege, Linux readiness, jitter and licensing; its two fatal flaws (renderer, host-API accuracy) are fixable by grafting, whereas B's (QtWebEngine-under-RenderControl, GPL-3, UAC-per-session) and C's (no privileged broker, node:vm isolation, Electron churn) are structural.
- D2 Effect renderer = real Chromium via CEF offscreen rendering (cef crate 154.5.0+154.0.34, Apache-2.0/MIT, verified on crates.io 2026-10-17) in a separate openlux-renderer process; tiny-skia Canvas2D shim and chromiumoxide/CDP fallback removed. Resolves judges 2 and 3's fatal flaw against A (fidelity, QuickJS per-pixel speed, external browser dependency) using C's Electron-OSR insight without Electron as the daemon. Week-1 spike in M0 retires the remaining risk (cef-rs OSR API surface).
- D3 Plugin engine stays rquickjs 0.14 (QuickJS-NG) with one runtime per plugin OS thread, memory cap and interrupt watchdog (A). B's QJSEngine would force Qt into a Rust core; corpus uses no BigInt/await/#private so QuickJS-NG is a syntactic superset. A ScriptEngine trait keeps a V8 backend possible if the M2 corpus harness shows semantic drift above 5% of plugins.
- D4 Host API table rebuilt from corpus-verified semantics (C, judge 1): getColors('Separate') returns [[r..],[g..],[b..]]; set_endpoint is positional (interface, usage, usage_page, collection); metadata exports are call-if-function; added getHidEndpoints, productId, addProperty/getProperty/removeProperty, fanControlDisabled/getNormalizedFanlevel, bus.*WithoutRegister/ReadBlockBytes/GetAddress/IsAMDGpuBus, UsesCorsairMutex; DeviceMessages flagged 'verify'.
- D5 Validate(endpoint).collection is the Windows per-top-level-collection index parsed from the HID interface path (&colNN), and on Linux is reproduced by an in-house report-descriptor parser (openlux-hid-desc) that splits hidraw nodes into TLCs in descriptor order. Resolves judge 1's fatal flaw on collection semantics and A's Linux-parity gap. 0- vs 1-based numbering is verified against the corpus in M2 on the Razer DeathAdder V2 Pro.
- D6 Unknown host member policy = log API_GAP once, then throw a typed error (judge 1's correction of C's no-op Proxy); corpus dry-run harness (C) runs on day one of M2 to produce the gap backlog; B's API_GAP telemetry feeds docs/host-api compat matrix.
- D7 Privileged broker = Windows service running as LocalSystem, installed once (UAC) only when the user opts into SMBus/GPU-I2C/SuperIO; PawnIO is verified admin-only (OpenRGB SMBusAccess.md), so neither A's 'LocalService-style' nor C's unprivileged addon nor B's per-session UAC runas works. Linux needs no broker.
- D8 Every SMBus transaction in the broker holds Global\Access_SMBUS.HTP.Method and the broker is single-threaded, so plugins are serialized by construction (resolves C's bus-corruption flaw); Scan()-derived per-plugin address allowlists, SPD EEPROM (0x50-0x57) and write-protect (0x30-0x37) write denial, rate limit, dry-run mode, kill switch (B).
- D9 Frame transport = C's SharedArrayBuffer/Atomics design translated to Rust: shm with 3 frame slots + seq between renderer and daemon, in-process triple buffer + condvar wake per plugin thread, precomputed Int32 pixel-offset table per LED, slow devices skip to the latest frame, high-resolution waitable timer as frame clock when no canvas is active.
- D10 Compatibility strategy = compatible runtime profile (compat-v1) of our own native-v2 format; WhirlwindFX plugin/effect files are never shipped, only imported from the user's own copies and marked unsigned; frozen MIT Host API v1 spec + conformance suite with HID record/replay (B); nominative use of the SignalRGB name only; @SignalRGB/* specifiers accepted for interop and aliased as @openlux/*.
- D11 Project license recommended Apache-2.0 OR MIT for all code (A), MIT for the spec/first-party plugins/effects, CC0 for starter effects; GPL-3 (B) rejected because it walls the core off from OpenRGB (GPL-2-only) and permissive consumers, and B's premise that QtWebEngine has no GPL-2 option is false anyway. Pawn modules we author go in a separate GPL-2 repo. cargo-deny denies GPL in the main tree.
- D12 UI = dashboard served by the daemon and usable in any browser; Tauri 2 tray shell is optional and off the critical path. Avoids making a second engine (WebView2/webkitgtk) load-bearing while CEF is already shipped for effects.
- D13 Consume Windows Dynamic Lighting LampArray devices as a generic HAL backend (B); exporting LampArray from user mode is impossible and not attempted.
- D14 ConflictingProcesses are reported with a close button, never killed silently; no LocalSystem app service, no custom driver with image-name checks, no Firebase marketplace, no Squirrel launcher, no registry persistence (B's 'what we do not copy' list adopted).
- D15 Compat API: refuse to bind 16038/16034 when SignalRGB owns them instead of fighting (C); mutating compat calls require the token by default (fixes B's default-off flaw); native API on 16138 token-authed.
- D16 Hotplug via CM_Register_Notification and USB parent association via CM_Get_Parent (A, judge 1's 'best hotplug design'); Bluetooth HID without a USB parent handled by treating each HID interface path as its own physical device group.
- D17 Sensors: no MSR/port I/O outside the broker; optional LibreHardwareMonitor HTTP JSON sensor source (C) so users who already run LHM get temps with zero kernel risk from us.
- D18 Milestone order puts the two highest technical risks first: M0 contains the CEF OSR spike and the rquickjs corpus-load spike; M1 is headless (no renderer, no browser) so first light does not wait on either.

## Milestones (panel version; the approved plan supersedes)

- M0 Bootstrap and risk retirement (weeks 1-2): cargo workspace with pinned [workspace.dependencies], deny.toml, rust-toolchain, CI on windows-latest + ubuntu-latest with the mock HAL; openlux-hal traits + mock backend; ADR-0001..0006 (Rust core, CEF OSR, rquickjs, LocalSystem broker, compat profile, license); SPIKE A: cef crate windowless browser renders a sample lightscript at 30 fps and delivers BGRA frames through on_paint into shm (acceptance: 300 consecutive frames, p99 paint interval < 40 ms, works with --software-compositing); SPIKE B: rquickjs loads 10 corpus plugins from the user's local directory as real ES modules with stubbed device/bus (acceptance: all 10 parse, exports resolve including call-if-function, relative imports and export-default class). Acceptance: both spikes green, CI green on both OSes, `cargo deny check` passes.
- M1 First light, headless (weeks 2-4): hal-windows hidapi backend with per-TLC endpoint enumeration and USB-parent grouping; openlux-plugin-host running first-party `msi-mystic-light-7c91.js` through the compat loader; null renderer (solid/rainbow/test pattern) + high-res frame clock; `luxctl devices|light|rainbow|bench`; mock-HAL golden test recording the 185-byte feature report. Acceptance: see first_milestone_detail (MSI MAG B550 Tomahawk lights from the CLI, 10-minute soak with p99 frame time < 5 ms and zero HID errors, original device state restored on exit).
- M2 Compatible plugin host (weeks 5-9): corpus dry-run harness (`luxctl compat dry-run <dir>`) producing the API-gap report; full device.*/bus.*/channel/subdevice/module surface from the corrected table; API_GAP log-and-throw trap; openlux-hid-desc report-descriptor parser; set_endpoint/getHidEndpoints; hotplug via CM_Register_Notification; plugin index cache; conformance suite with HID record/replay fixtures; plugin watchdog. Acceptance: 100% of the user's corpus parses, >= 95% complete Initialize() under the harness; unmodified imported plugins drive the Razer DeathAdder V2 Pro (feature reports + CRC + sleep/wake), SteelSeries Arctis 5 and the Sonix 320F keyboard (multi-interface/multi-collection Validate + getHidEndpoints) on this machine; a hot-unplug/replug of the mouse recovers within 2 s without restarting the daemon.
- M3 Effects pipeline (weeks 10-14): openlux-renderer process (CEF OSR), shm frame ring + control pipe, renderer supervision/restart, <meta> parser and control model, engine.* globals with JSON push, onCanvasTapped/onCanvasApiEvent, layout + sampling with precomputed offsets, global/per-device brightness and LightingMode, 15-fps downshift guard, `luxctl bench` end-to-end histograms. Acceptance: 20 community lightscripts (including at least 3 per-pixel ImageData effects, 1 WebGL effect and 1 text-rendering effect) run unmodified at 30 fps; fidelity harness PSNR >= 40 dB vs stock Chromium reference on all 20; canvas-to-wire p50 <= 10 ms, p99 <= 25 ms with 4 devices; renderer RSS <= 300 MB; renderer crash is recovered within 3 s with the last frame held.
- M4 Dashboard, persistence, local API, packaging (weeks 15-19): SQLite persistence and migrations; axum native API on 16138 (REST + WS, token) and compat listeners 16038/16034 with the shape-copied responses; dashboard (devices, layout editor with drag/scale/rotate/invert, effects + controls + presets, components/channels editor for the MSI JRAINBOW headers, logs, settings); import wizard (plugins, effects, components, HKCU channel arrays, layouts) marking imports unsigned; optional Tauri tray shell with autostart; WiX MSI + winget manifest, Authenticode signing. Acceptance: a fresh Windows 11 install goes from MSI to lit devices and a saved layout without touching a terminal; a community component JSON attached to JRAINBOW1 lights an ARGB strip; an existing third-party integration hitting GET /api/v1/lighting on 16038 works unchanged; config/layout export and re-import round-trip byte-for-byte.
- M5 Privileged bus via broker (weeks 20-24): openlux-iobroker Windows service (LocalSystem) with PawnIO client, pinned SmbusPIIX4/SmbusI801/LpcIO module hashes, Unrestricted-build refusal, op enum, named-mutex discipline, allowlists, dry-run; openlux-setup one-shot elevated installer (PawnIO, service, WinUSB binding); bus.* mapping; openlux-nvapi dynamic I2C; sensor sources (broker MSR/NvAPI thermal, optional LibreHardwareMonitor JSON); fan curves + SuperIO (stretch). Acceptance: Crucial Ballistix RAM lights through SmbusPIIX4 on this B550 with the unmodified imported plugin; RTX 3060 lights through NvAPI I2C; running HWiNFO/OpenRGB concurrently produces no bus errors over a 30-minute soak; the broker's fuzz target runs 1M iterations without a crash; no UAC prompt after the one-time setup.
- M6 Inputs, network, extras (weeks 25-29): audio FFT (engine.audio), screen ambience via xcap (engine.zone) delivered through the CEF V8 extension over shm, vision area/linear meters, network plugins (@SignalRGB/udp|tcp, DeviceDiscovery, mDNS, controller/discovery/service globals, DTLS feature), LCD faces (second browser per device), macros (Inputs/Actions scripts, system.action effect/layout apply), LampArray HAL backend, OpenRGB SDK server on 6742, Dynamic ControllableParameters UI. Acceptance: an audio-reactive and a screen-ambience effect track their inputs with < 100 ms latency; an unmodified network plugin discovers and drives a LAN device (Govee/Yeelight/WLED class) with the UDP binds opened only while active; an OpenRGB SDK client sets a color on the MSI board; a DL-certified keyboard lights with no plugin.
- M7 Linux backend and release 1.0 (weeks 30-35): hal-linux (hidraw via hidapi linux-native + openlux-hid-desc, nusb, serialport, udev hotplug, i2cdev, hwmon fan/temps), tools/gen-udev, CEF Linux renderer (X11/Wayland, headless for CI), PipeWire capture/audio, systemd --user unit, AppImage + Flatpak, docs/linux-porting.md. Acceptance: the same first-party and imported MSI/Razer plugin files light the same hardware on Ubuntu LTS with zero plugin changes; Validate() receives identical endpoint tuples on both OSes for the Razer mouse (conformance fixture); RAM lights via /dev/i2c-* without any privileged helper; CI runs the full conformance suite on both OSes; 1.0 release via cargo-dist with reproducible hashes.

## Repo layout (panel version)

```
openlux/                      # placeholder name; see open decisions
  Cargo.toml                  # workspace; [workspace.dependencies] = the pin table in §2
  rust-toolchain.toml         # stable, pinned minor
  deny.toml                   # allow MIT/Apache-2.0/BSD/MPL-2.0/ISC/Zlib; deny GPL/LGPL/AGPL in-tree
  LICENSE-APACHE  LICENSE-MIT  README.md  SECURITY.md  CONTRIBUTING.md (clean-room rule)  CODE_OF_CONDUCT.md
  crates/
    openlux-hal/              # traits (Hid, Usb, Serial, Smbus, GpuI2c, Sensors, Hotplug, Capture, Audio, LampArray) + mock backend + transaction recorder
    openlux-hal-windows/      # hidapi windows-native, nusb/WinUSB, serialport, CM_Register_Notification, CM_Get_Parent grouping, &colNN endpoint parsing, HidD_GetInputReport, WinRT LampArray, broker client
    openlux-hal-linux/        # hidapi linux-native (hidraw), nusb, serialport, udev monitor, i2cdev, hwmon sysfs
    openlux-hid-desc/         # HID report-descriptor parser -> top-level collections / report ids (TLC parity across OSes)
    openlux-protocol/         # broker wire types (serde + postcard), op enum, shm frame layout (shared with renderer)
    openlux-iobroker/         # Windows service (LocalSystem): PawnIO client, module hash pins, allowlists, mutex discipline, audit log, fuzz target
    openlux-nvapi/            # dynamic nvapi64.dll loader, self-declared QueryInterface IDs (I2C, GPU enum, thermal)
    openlux-plugin-host/      # rquickjs runtime per thread, module resolver/loader, export resolution (call-if-function), device/bus/channel host objects, API_GAP trap, watchdog, compat-v1 + native-v2 profiles
    openlux-effects/          # lightscript <meta> parser, effect index, presets, engine-data producer
    openlux-render/           # RendererClient trait; null renderer; cef client (shm reader, process supervisor); frame clock
    openlux-renderer/         # openlux-renderer binary: CEF windowless browser, on_paint -> shm, URL filter, engine.* injection, V8 extension (M6), LCD faces
    openlux-inputs/           # audio FFT (cpal + rustfft), screen zones (xcap), sensor facade (broker / hwmon / LibreHardwareMonitor JSON)
    openlux-engine/           # registry, layouts, sampling (precomputed offsets), FrameStore, scheduler, channels/components, fan curves, persistence (rusqlite), events, import wizard, macros
    openlux-api/              # axum: native API :16138 (REST+WS, token), compat :16038/:16034, OpenRGB SDK server :6742 (M6), OpenAPI
    openlux-daemon/           # openluxd binary: lifecycle state machine, power events, single instance, dashboard static serving
    openlux-cli/              # luxctl: devices, light, rainbow, bench, compat dry-run, record/replay, import, udev
    openlux-setup/            # one-shot elevated installer: PawnIO, broker service, WinUSB binding for approved VID:PIDs
  prelude/                    # plain-JS host shims: BinaryUtils.js, Errors.js, base64.js, createColorArray.js, @SignalRGB/* facades, engine.js (renderer side)
  ui/                         # dashboard (Svelte + TS, Vite); built assets embedded in openlux-daemon; optional tauri/ shell (tray, autostart)
  plugins/                    # first-party MIT plugins in compat shape: msi-mystic-light-7c91/, razer-deathadder-v2-pro/, nzxt-kraken/, steelseries-arctis5/, crucial-ballistix-smbus/, sonix-320f/, generic-lamparray/ (each with manifest.json + LICENSE)
  effects/                    # first-party lightscripts (CC0): solid, rainbow-wave, audio-bars, screen-ambience, test-pattern
  components/                 # own component JSON library (fans, strips) in the SignalRGB-compatible schema
  schemas/                    # JSON Schemas: manifest, ControllableParameters, component, layout, effect meta; OpenAPI
  sdk/                        # openlux.d.ts, Node simulator (mock HAL recorder), plugin lint
  docs/
    host-api/                 # MIT: Host API v1 (2.5.80-era) spec: exports, device.*, bus.*, channel, modules, globals, engine.* ; compat matrix generated from API_GAP reports
    adr/                      # ADR-0001 rust-core, 0002 cef-osr-renderer, 0003 rquickjs, 0004 localsystem-broker, 0005 compat-profile, 0006 license
    architecture.md  security.md  linux-porting.md  hardware/ (per-device protocol notes, facts only)
  tests/
    conformance/              # corpus runner (points at an external user-supplied dir), HID record/replay fixtures, fidelity harness (effect PSNR)
    integration/              # mock-HAL end-to-end
  tools/                      # gen-udev, pawnio-hash-pin, protocol capture notes template, shm-dump
  packaging/                  # wix/ (MSI), winget/, systemd/, udev/, appimage/, flatpak/
  pawn-modules/ -> separate GPL-2 repository (openlux-pawn-modules) for Pawn code we author for PawnIO
  .github/workflows/          # ci.yml (win + linux, mock HAL, deny, fmt, clippy), conformance-selfhosted.yml, release.yml (cargo-dist, signing, SBOM)
```

## First milestone detail

M1 — light the MSI MAG B550 Tomahawk (Mystic Light, HID 1462:7C91) on this machine, headless.

Why this device: physically present; single 185-byte HID feature report per update; no handshake, no WinUSB, no elevation, no browser; wrong report id simply does nothing; the user's local corpus contains a SignalRGB-format plugin for it (used only as a reference for the packet shape and as the M2 'unmodified import' check), and OpenRGB's MSIMysticLight185 documentation can be used to cross-check our understanding without copying code (GPL-2).

Deliverables:
1. Workspace + crates from M0 (openlux-hal with mock, openlux-hal-windows hidapi backend, openlux-plugin-host, openlux-render null, openlux-engine minimal, openlux-cli).
2. hal-windows: `hid_enumerate` grouped by USB parent (CM_Get_Parent); endpoint tuple {interface, usage, usage_page, collection (&colNN), length}; open/write/read_timeout/send_feature_report/get_feature_report.
3. First-party plugin `plugins/msi-mystic-light-7c91/plugin.js` written in the compat shape (MIT): `export function VendorId(){return 0x1462}`, `ProductId(){return 0x7C91}`, `Type 'Hid'`, `Validate(endpoint)` selecting the vendor-defined usage page, `Size`, `LedNames`/`LedPositions` for onboard zones (JRGB1/JRGB2 single-color headers and the onboard logo zones), JRAINBOW1/JRAINBOW2 as channels (disabled by default in M1), `Initialize()` reads the current 0x52 feature report with get_report and keeps it as the restore image, `Render()` builds the 185-byte packet (report id 0x52, per-zone mode/color bytes, direct-mode flag) only touching the bytes we understand, `Shutdown()` writes the restore image back.
4. Null renderer producing solid / rainbow / test-pattern frames into the in-process FrameStore at 30 fps from the high-resolution waitable timer; engine samples the plugin's LedPositions through a default placement; plugin thread wakes on seq, samples, calls Render().
5. `luxctl devices` (lists endpoints with the tuple), `luxctl light 1462:7C91 --color ff0000`, `luxctl rainbow --fps 30 --minutes 10 --stats`, `luxctl bench` (frame-time and write-latency p50/p95/p99 histograms, JSON output).
6. Mock-HAL golden test: the plugin running through the same loader and mock backend records the exact 185-byte packet for a known color and compares to a checked-in fixture; a second test replays `get_report` output recorded from the real board.
7. docs/hardware/msi-mystic-light-185.md with the packet layout as we verified it (facts only), docs/adr/0001-0006.

Safety: never write before a successful get_report; keep unknown bytes from the read image; JRAINBOW headers untouched in M1; a `--dry-run` flag prints the packet instead of sending; Shutdown restores the original image; SignalRGB/Mystic Light must be closed first (detected via ConflictingProcesses and reported, not killed).

Acceptance criteria (all measured on this machine):
- `luxctl devices` shows 1462:7C91 with at least one vendor-usage-page endpoint and the plugin as matched.
- `luxctl light … --color ff0000` turns the board's onboard LEDs red; `--color 00ff00` green; visible within 100 ms.
- `luxctl rainbow --minutes 10`: zero HID write errors, frame-time p99 < 5 ms (no renderer involved), send_feature_report latency p99 < 5 ms, daemon RSS < 40 MB, no memory growth over the soak.
- Ctrl-C restores the pre-run lighting state (verified by re-reading the feature report and diffing).
- Mock-HAL golden tests pass in CI on windows-latest and ubuntu-latest (no hardware).
- The plugin is loaded by exactly the compat loader that M2 will point at the user's imported SignalRGB plugin directory (no M1-only code path).

## Decisions resolved by the user on 2026-10-17

- Stack: Rust + rquickjs + CEF (confirmed).
- Compat runtime + community repos pre-listed as sources.
- License Apache-2.0 OR MIT.
- Name Lightloom.
