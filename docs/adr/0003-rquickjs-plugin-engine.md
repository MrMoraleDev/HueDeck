<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ADR-0003: Device plugins run on rquickjs (QuickJS-NG), one runtime per plugin thread

- Status: Accepted (2026-10-17); risk retired by SPIKE B in M0; revisit if the M2
  corpus harness shows semantic drift above 5 % of plugins
- Deciders: project owner, architecture panel (synthesis D3, D6)
- Related: [ADR-0005](0005-signalrgb-compat-profile.md), [docs/host-api/hid-usb.md](../host-api/hid-usb.md), [docs/architecture-synthesis.md](../architecture-synthesis.md) section 4

## Context

Device support is roughly 560 JavaScript ES-module plugins (one per device family,
about 1366 VID:PID pairs) plus network-service and SMBus plugins. They:

- are ES modules with bare imports (`@SignalRGB/udp`, `@SignalRGB/base64`, ...) and
  relative imports, `export default class` for discovery services, and metadata
  exports that may be values or functions ("call-if-function");
- expect a **synchronous** host API: `device.write()` returns bytes written,
  `device.read(..., timeout)` blocks, `bus.ReadByte()` returns a value, `pause(ms)`
  sleeps;
- use an ES2017-ish subset: a corpus survey found no `BigInt`, no top-level or
  function `await`, no `#private` fields;
- run in the reference app in one QJSEngine per device on its own thread, with no
  DOM globals (`Image is not defined` rejects a plugin).

The plugin loop is called about 30 times per second with per-LED loops of a few
hundred entries; the host must bound memory and abort runaway `Render()` calls.

## Decision

- Engine: `rquickjs` 0.14 (QuickJS-NG) with features `loader`, `array-buffer`,
  `futures`, `parallel`.
- **One `Runtime` + `Context` per plugin, owned by one OS thread** that also owns the
  plugin's HID/USB/serial handle. Network services get one runtime per
  `DiscoveryService`; each announced controller gets its own device thread.
- Real ES-module loading through a custom `Resolver`/`Loader`: `@SignalRGB/*` and the
  `@lightloom/*` alias map to Rust-backed or plain-JS shim modules; `./X.js` resolves
  inside the plugin directory; the module namespace is read after evaluation and
  metadata exports are invoked if they are functions.
- `set_memory_limit(64 MB)` per runtime; `set_interrupt_handler` watchdog aborts a
  `Render()` call whose script time exceeds 250 ms (time spent inside host
  `pause()` and blocking `read()` is not counted); three strikes fault the plugin,
  show the device in error and restart with backoff. `Initialize()` / `Shutdown()`
  get a seconds-scale budget instead: corpus plugins legitimately call
  `pause(1000)` up to `pause(20000)` during setup.
- Host objects (`device`, `bus`, `channel(...)`, `service`, `controller`,
  `discovery`) are Rust classes; unknown members hit a Proxy trap that logs
  `API_GAP:<plugin>:<member>` once and then throws a typed `HostApiMissing` error
  (configurable to no-op).
- Synchronous facades: sockets, HTTP (`XmlHttp`, `XMLHttpRequest` with sync `open`)
  and `pause` block the plugin thread only; `tokio` work happens elsewhere.
- A `ScriptEngine` trait wraps the engine so a V8 backend (`deno_core`) can be added
  without touching the engine if needed.

## Consequences

Positive:

- Tiny footprint (a runtime is a few MB), fast startup, trivially embeddable, no JIT
  means no warm-up jitter in the frame loop.
- Interrupt handler and memory limit give a real watchdog, which `node:vm` cannot.
- Thread-per-plugin matches the reference behaviour that plugin authors implicitly
  rely on (blocking reads, `pause`, `TransactionLock`).

Negative / accepted costs:

- No JIT: per-LED loops are roughly 0.05-1 ms per `Render()`, acceptable at 30 Hz
  but heavier plugins (large keyboards with per-key packets) must be measured by
  `lightloom bench`.
- QuickJS-NG is a different engine from Qt's V4; semantic corners (Date, regex,
  `Array.prototype.sort` stability, number formatting) may differ. Mitigation: the
  M2 corpus dry-run harness and the conformance suite with HID record/replay.
- The whole `@SignalRGB/*` module surface and `BinaryUtils`-style helpers must be
  written as shims in `prelude/`.

## Alternatives considered

- **V8 via `deno_core`**: full JIT and the broadest semantics, but async-first
  embedding, 10x binary size, slow builds, and one isolate per thread is heavy for
  hundreds of plugins. Kept as a possible backend behind `ScriptEngine`.
- **QJSEngine** (Qt V4, what the reference uses): forces Qt into the core.
- **Boa** (Rust JS engine): promising, but too immature for unmodified community
  code and slower than QuickJS.
- **Node.js / Electron with `node:vm`**: no memory cap, no reliable interrupt, and a
  single process for all plugins.
- **Duktape**: ES5-era; the corpus needs ES modules, classes and spread.
