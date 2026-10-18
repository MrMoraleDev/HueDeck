<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ADR-0001: Rust for the core (daemon, CLI, renderer host, broker)

- Status: Accepted (2026-10-17)
- Deciders: project owner, architecture panel (3 proposals, 3 judges; synthesis D1)
- Related: [ADR-0002](0002-cef-offscreen-renderer.md), [ADR-0003](0003-rquickjs-plugin-engine.md), [ADR-0004](0004-localsystem-iobroker-pawnio.md), [docs/architecture-synthesis.md](../architecture-synthesis.md) D1

## Context

HueDeck must run as an unprivileged per-user daemon that:

- talks to HID, raw USB (WinUSB/libusb-class), serial, SMBus/I2C and GPU I2C hardware
  with soft real-time behaviour (frame drops are skips, never backlog; canvas-to-wire
  p50 <= 10 ms, p99 <= 25 ms);
- hosts hundreds of untrusted-ish JavaScript device plugins, one engine per device on
  its own thread, with a memory cap and a watchdog;
- supervises a separate Chromium (CEF) renderer process and a separate privileged
  I/O broker process over narrow IPC;
- stays small (target daemon RSS around 40 MB), has no GC pauses in the frame loop,
  and ports to Linux without rewriting the engine.

The reference application (SignalRGB 2.5.80) is a Qt 6 / C++ monolith with a Rust
helper service; the architecture panel scored three proposals (Rust core, C++/Qt
core, Electron core) and the Rust proposal scored highest overall (218/300) and best
on least privilege, Linux readiness, jitter and licensing. Its two fatal flaws (the
from-scratch Canvas2D renderer and an inaccurate host-API table) were fixable by
grafting from the other proposals; the structural flaws of the others were not.

## Decision

The whole core is written in Rust as a single Cargo workspace:

- `huedeckd` (daemon: engine, plugin host, API, persistence), `huedeck` (CLI),
  `huedeck-renderer` (CEF host process), `huedeck-iobroker` (optional Windows
  service), `huedeck-setup` (one-shot elevated installer) are all Rust binaries.
- Async work (API, sockets, hotplug, discovery, renderer supervision) runs on `tokio`;
  plugin execution and device I/O are plain OS threads (one per plugin), not tasks.
- Hardware access goes through traits in `huedeck-hal` with per-OS backend crates and
  a mock backend used by CI and the simulator from M0.
- Dependencies are pinned once in `[workspace.dependencies]` and policed by
  `cargo-deny` (permissive licenses only, no vulnerable I/O drivers, crates.io only).
- A `ScriptEngine` trait and a `RendererClient` trait keep the JS engine and the
  effect renderer replaceable without touching the engine.

## Consequences

Positive:

- Memory safety across the HID/USB/FFI boundary and in the parser-heavy broker.
- One toolchain and one build for Windows and Linux; cross-platform crates exist for
  every HAL need (`hidapi`, `nusb`, `serialport`, `i2cdev`, `rusqlite`, `cef`,
  `rquickjs`).
- Predictable latency: no runtime GC, threads and condvars map directly to the
  per-plugin frame loop.
- `cargo-deny`, `cargo-auditable` and reproducible `cargo-dist` builds give a
  supply-chain story for free.

Negative / accepted costs:

- FFI we must write ourselves: CEF bindings use the `cef` crate, but NvAPI (dynamic
  `nvapi64.dll`), PawnIO IOCTLs and a few Win32 calls (`HidD_GetInputReport`,
  `CM_Register_Notification`) are hand-bound in `huedeck-nvapi` / `huedeck-hal-windows`.
- Longer compile times than a scripting-language core; CEF adds a large download.
- Fewer plugin authors are fluent in Rust; mitigated by keeping the JS prelude shims
  (`prelude/*.js`) in plain JavaScript so host gaps can be fixed without Rust.

## Alternatives considered

- **C++ / Qt** (what the reference app uses): mature, but drags Qt licensing and a
  very large surface into a project whose value is the plugin ecosystem; QJSEngine
  and QtWebEngine would then be forced choices.
- **Electron / Node.js as the daemon** (proposal C): excellent renderer story, but
  150+ MB RSS before the first plugin, N-API prebuild churn for HID/USB bindings, weak
  `node:vm` isolation for plugins, and a privileged broker could not be written in
  the same codebase cleanly.
- **Go**: cgo for every HID/USB/CEF call and GC pauses in a 30 Hz loop.
- **C# / .NET**: Windows-first ecosystem, runtime size, and a second toolchain for the
  renderer and broker.
