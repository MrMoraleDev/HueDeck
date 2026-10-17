<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
# Lightloom crates

Workspace members (`Cargo.toml` → `members = ["crates/*"]`). Shared versions are pinned once in the
root `[workspace.dependencies]`; every crate uses `{ workspace = true }` and lists only what it uses.

## Dependency graph

```text
                      ┌──────────────────┐
                      │   lightloom-hal    │  traits + mock (design centre; no OS code)
                      └───────┬──────────┘
            ┌─────────────────┼──────────────────────────┐
            ▼                 ▼                          ▼
 lightloom-hal-windows   lightloom-hal-linux           lightloom-engine ◄──── lightloom-render
 (hidapi, &MI/&Col)    (hidraw stub, M7)           (Rgb/Frame/FrameStore/   (RendererClient,
            │                                       Layout/sampler)          NullRenderer, FrameClock)
            ▼
   lightloom-hid-desc  (report-descriptor parser; also used by hal-linux in M7)

 lightloom-protocol ◄──── lightloom-iobroker (bin, M5)      lightloom-plugin-host   lightloom-effects
 (shm header, broker ◄── lightloom-renderer (bin, M3)      (meta model, resolver, (<meta> parser,
  op enum, postcard)                                      loader trait; rquickjs  effect-meta model)
                                                          from M1)
 lightloom-inputs (stub)   lightloom-api (route table)   lightloom-nvapi (stub)   lightloom-setup (bin stub)

 lightloom-daemon (bin lightloomd)  — will depend on: hal, hal-<os>, engine, render, plugin-host,
                                   effects, inputs, api, protocol
 lightloom-cli    (bin lightloom)   — will depend on: api client types, engine (colours), render (null)
```

Arrows point from dependency to dependent. At M0 the two binaries depend on nothing in-tree yet; the
daemon/CLI wiring arrives with M1 and the graph above shows the intended shape.

## Crate summary

| crate | kind | status | contents |
|---|---|---|---|
| `lightloom-hal` | lib | M0 | `HidBackend/HidHandle`, `UsbBackend/UsbHandle`, `SerialBackend/SerialHandle`, `SmbusBus` + `SmbusBusDescriptor{kind}`, `HotplugSource`, `SensorBackend`, `CaptureBackend`, `AudioBackend`, `LampArrayBackend`, `HalError`, `DeviceKey` (`vid:pid:serial`), `EndpointUid`; feature `mock`: `MockHid`, `MockSmbus`, `TransactionRecorder` |
| `lightloom-hal-windows` | lib | M0/M1 | `path::parse_hid_path` (`&MI_xx` hex → interface, `&ColNN` decimal → 0-based collection), `WindowsHidBackend` on hidapi `windows-native` |
| `lightloom-hal-linux` | lib | stub (M7) | `LinuxHidBackend` placeholder |
| `lightloom-hid-desc` | lib | M0 | HID report-descriptor items, top-level collections, report ids, max in/out/feature sizes (incl. report-id byte) |
| `lightloom-protocol` | lib | M0 | `shm::FrameHeader` (`"HDFR"`, 3 slots, seqlock generations), `broker::{BrokerOp, BrokerRequest, BrokerResponse, BrokerError}` + postcard helpers |
| `lightloom-engine` | lib | M0 | `Rgb`, `Frame` (BGRA), `FrameStore` (triple_buffer + `AtomicU64` seq + `Condvar`), `Layout`/`Placement`/`DeviceGeometry` (serde shape = `schemas/layout.schema.json`: `gridSize`, `flipX`/`flipY`, `lightingMode`, `colorCorrection`), `compute_offsets` sampler (top-left pivot per schema, centre pivot optional), brightness math |
| `lightloom-render` | lib | M0 | `RendererClient`, `FrameClock` (sleep + spin), `NullRenderer` (solid / rainbow / test pattern) |
| `lightloom-plugin-host` | lib | M0 | `PluginMeta`/`PluginType`/`ProductIdExport` (call-if-function rule documented), `ModuleResolver` (`@SignalRGB/*`, `@lightloom/*`, relative-inside-root), `Loader` trait, `PluginIndex` (last scanned wins). No rquickjs yet |
| `lightloom-effects` | lib | M0 | `parse_effect_meta(html) -> EffectMeta` (serde shape = `schemas/effect-meta.schema.json`: `properties[]`, `meters[]` with `region`/`match`/`hdrMatch`/`resolutions[]`, `uses`), `js_initializer` |
| `lightloom-inputs` | lib | stub (M6) | `AudioSnapshot`, `ZoneSnapshot` shapes, `InputSource` trait |
| `lightloom-api` | lib | stub (M4) | ports (16138 native, 16038/16034 compat, 6742 OpenRGB) and route tables |
| `lightloom-daemon` | bin `lightloomd` | M0 | clap + tracing, `--config`, `LifecycleState` |
| `lightloom-cli` | bin `lightloom` | M0 | `devices | light | rainbow | bench | compat dry-run`, each returning `NotImplemented` with its milestone |
| `lightloom-iobroker` | bin | stub (M5) | answers `Health`/`ListBuses` over the protocol types |
| `lightloom-nvapi` | lib | stub (M5) | constants, `NvStatus`, `NvapiError` |
| `lightloom-renderer` | bin | stub (M3) | prints shm geometry |
| `lightloom-setup` | bin | stub (M5) | prints banner |

## Conventions

* Every file starts with `SPDX-License-Identifier: Apache-2.0 OR MIT`.
* `#![forbid(unsafe_code)]` everywhere except OS backends, where each `unsafe` block carries a `// SAFETY:` comment.
* Keep each crate's dependency list to what it actually uses; add workspace pins in the root `Cargo.toml` first.
* `// TODO(compile-check)` marks spots written without a compiler at hand (dependency API details to confirm on the first `cargo build`).
* Serde names of `Layout`/`Placement` and `EffectMeta` are checked against the `schemas/*.schema.json` examples by unit tests (`serde_json` dev-dependency only); keep them in sync when either side changes.
* Never copy code from SignalRGB/WhirlwindFX files or OpenRGB; facts about the plugin contract come from `docs/host-api/*.md`.
