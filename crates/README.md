<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# HueDeck crates

Workspace members (`Cargo.toml` → `members = ["crates/*"]`). Shared versions are pinned once in the
root `[workspace.dependencies]`; every crate uses `{ workspace = true }` and lists only what it uses.

## Dependency graph

```text
                      ┌──────────────────┐
                      │   huedeck-hal    │  traits + mock (design centre; no OS code)
                      └───────┬──────────┘
            ┌─────────────────┼──────────────────────────┐
            ▼                 ▼                          ▼
 huedeck-hal-windows   huedeck-hal-linux           huedeck-engine ◄──── huedeck-render
 (hidapi, &MI/&Col)    (hidraw stub, M7)           (Rgb/Frame/FrameStore/   (RendererClient,
            │                                       Layout/sampler)          NullRenderer, FrameClock)
            ▼
   huedeck-hid-desc  (report-descriptor parser; also used by hal-linux in M7)

 huedeck-protocol ◄──── huedeck-iobroker (bin, M5)      huedeck-plugin-host   huedeck-effects
 (shm header, broker ◄── huedeck-renderer (bin, M3)      (meta model, resolver, (<meta> parser,
  op enum, postcard)                                      loader trait; rquickjs  effect-meta model)
                                                          from M1)
 huedeck-inputs (stub)   huedeck-api (route table)   huedeck-nvapi (GPU I2C)   huedeck-setup (bin, M5)

 huedeck-daemon (bin huedeckd)  — will depend on: hal, hal-<os>, engine, render, plugin-host,
                                   effects, inputs, api, protocol
 huedeck-cli    (bin huedeck)   — will depend on: api client types, engine (colours), render (null)
```

Arrows point from dependency to dependent. At M0 the two binaries depend on nothing in-tree yet; the
daemon/CLI wiring arrives with M1 and the graph above shows the intended shape.

## Crate summary

| crate | kind | status | contents |
|---|---|---|---|
| `huedeck-hal` | lib | M0 | `HidBackend/HidHandle`, `UsbBackend/UsbHandle`, `SerialBackend/SerialHandle`, `SmbusBus` + `SmbusBusDescriptor{kind}`, `HotplugSource`, `SensorBackend`, `CaptureBackend`, `AudioBackend`, `LampArrayBackend`, `HalError`, `DeviceKey` (`vid:pid:serial`), `EndpointUid`; feature `mock`: `MockHid`, `MockSmbus`, `TransactionRecorder` |
| `huedeck-hal-windows` | lib | M0/M1/M5 | `path::parse_hid_path` (`&MI_xx` hex → interface, `&ColNN` decimal → 0-based collection), `WindowsHidBackend` on hidapi `windows-native`; `broker::BrokerSmbus` (I/O broker client as `SmbusBus`), `pawnio::pawnio_presence` (uninstall entry only) |
| `huedeck-hal-linux` | lib | stub (M7) | `LinuxHidBackend` placeholder |
| `huedeck-hid-desc` | lib | M0 | HID report-descriptor items, top-level collections, report ids, max in/out/feature sizes (incl. report-id byte) |
| `huedeck-protocol` | lib | M0/M5 | `shm::FrameHeader` (`"HDFR"`, 3 slots, seqlock generations), `broker::{BrokerOp, BrokerRequest, BrokerResponse, BrokerError}` (v2: `BusLock`/`BusUnlock`, kind-aware SPD and block rules), frames, postcard helpers |
| `huedeck-engine` | lib | M0 | `Rgb`, `Frame` (BGRA), `FrameStore` (triple_buffer + `AtomicU64` seq + `Condvar`), `Layout`/`Placement`/`DeviceGeometry` (serde shape = `schemas/layout.schema.json`: `gridSize`, `flipX`/`flipY`, `lightingMode`, `colorCorrection`), `compute_offsets` sampler (top-left pivot per schema, centre pivot optional), brightness math |
| `huedeck-render` | lib | M0 | `RendererClient`, `FrameClock` (sleep + spin), `NullRenderer` (solid / rainbow / test pattern) |
| `huedeck-plugin-host` | lib | M0 | `PluginMeta`/`PluginType`/`ProductIdExport` (call-if-function rule documented), `ModuleResolver` (`@SignalRGB/*`, `@huedeck/*`, relative-inside-root), `Loader` trait, `PluginIndex` (last scanned wins). rquickjs runtime, device sessions, catalog; M6: `net` (LAN-only UDP/TCP sockets, HTTP client, one-shot mDNS), `service` (network plugin discovery services) and network device sessions |
| `huedeck-effects` | lib | M0 | `parse_effect_meta(html) -> EffectMeta` (serde shape = `schemas/effect-meta.schema.json`: `properties[]`, `meters[]` with `region`/`match`/`hdrMatch`/`resolutions[]`, `uses`), `js_initializer` |
| `huedeck-inputs` | lib | stub (M6) | `AudioSnapshot`, `ZoneSnapshot` shapes, `InputSource` trait |
| `huedeck-api` | lib | stub (M4) | ports (16138 native, 16038/16034 compat, 6742 OpenRGB) and route tables |
| `huedeck-daemon` | bin `huedeckd` | M0 | clap + tracing, `--config`, `LifecycleState` |
| `huedeck-cli` | bin `huedeck` | M0 | `devices | light | rainbow | bench | compat dry-run`, each returning `NotImplemented` with its milestone |
| `huedeck-iobroker` | lib + bin | M5 | LocalSystem service for the chipset SMBus (RAM): raw PawnIO IOCTLs (`pawnio`), SHA-256 pinned modules (`pins`), `SmbusPIIX4` transfers on ports 0x0B00/0x0B20 (`piix4`), `Global\Access_SMBUS.HTP.Method` (`mutex`), static policy and rate limit (`policy`), PawnIO detection (`detect`), the request handler with `BusLock` leases, `Batch` and `SpdRead` (`server`), the named pipe (`pipe`); fakes for tests |
| `huedeck-nvapi` | lib | M5 | dynamic `nvapi64.dll` binding (System32 only): `NV_I2C_INFO_V3` and the SMBus → NvAPI transfer mapping, `NvapiHub` (one `huedeck-nvapi` worker thread owns NvAPI), `NvapiBus: SmbusBus` per GPU (port 1, never DDC), `FakeNvI2c` for tests |
| `huedeck-renderer` | bin | stub (M3) | prints shm geometry |
| `huedeck-setup` | bin | M5 | `--smbus`: checks PawnIO (does not install it), copies the pinned modules and the broker under `%ProgramFiles%\HueDeck`, registers and starts `HueDeckIoBroker`; `--uninstall`, `--plan` |

## Conventions

* Every file starts with `SPDX-License-Identifier: GPL-3.0-or-later`.
* `#![forbid(unsafe_code)]` everywhere except OS backends, where each `unsafe` block carries a `// SAFETY:` comment.
* Keep each crate's dependency list to what it actually uses; add workspace pins in the root `Cargo.toml` first.
* `// TODO(compile-check)` marks spots written without a compiler at hand (dependency API details to confirm on the first `cargo build`).
* Serde names of `Layout`/`Placement` and `EffectMeta` are checked against the `schemas/*.schema.json` examples by unit tests (`serde_json` dev-dependency only); keep them in sync when either side changes.
* Never copy code from SignalRGB/WhirlwindFX files or OpenRGB; facts about the plugin contract come from `docs/host-api/*.md`.
