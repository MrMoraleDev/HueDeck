<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# ADR-0002: Effects render in real Chromium via CEF offscreen rendering

- Status: Accepted (2026-10-17); risk retired by SPIKE A in M0
- Deciders: project owner, architecture panel (synthesis D2, D9)
- Related: [ADR-0001](0001-rust-core.md), [docs/host-api/effects-engine.md](../host-api/effects-engine.md), [docs/architecture-synthesis.md](../architecture-synthesis.md) sections 3 and 6

## Context

Effects ("lightscripts") are ordinary HTML pages: a `<canvas>` of 320x200 pixels, a
`requestAnimationFrame` loop, `<meta>` tags that declare user controls, and free use
of the web platform (per-pixel `ImageData`, WebGL, text rendering, CSS). The engine
samples the canvas per device roughly 30 times per second. Fidelity matters: users
import effects they already own and expect them to look identical.

The reference app renders them with Ultralight (proprietary WebKit build, CPU
raster) and falls back to QtWebEngine. The first Rust proposal planned a Canvas2D
re-implementation on `tiny-skia` plus QuickJS for the effect scripts; two judges
rated that a fatal flaw (incomplete Canvas2D semantics, no WebGL, per-pixel loops too
slow without a JIT, every browser quirk re-implemented by hand).

## Decision

Effects run in a real Chromium instance embedded through the Chromium Embedded
Framework, in a **separate process** `lightloom-renderer`:

- Crate `cef` 154.5.0+154.0.34 (Apache-2.0 OR MIT bindings; CEF itself BSD-3). The
  same executable serves as the CEF subprocess via `execute_process`.
- `windowless_rendering_enabled`, `windowless_frame_rate` = configured fps (30
  default, up to 60). `RenderHandler::on_paint` copies the BGRA 320x200 buffer into
  the next slot of a shared-memory ring (3 slots of 256 KB, header with magic,
  version, size, sequence number, per-slot generation stamps for torn-read
  detection) and sends the new sequence number over a control pipe (named pipe on
  Windows, Unix socket on Linux, `postcard` frames).
- The daemon publishes the frame to its in-process triple buffer and wakes plugin
  threads; a crashed renderer is restarted while the last frame is held.
- Chromium sandbox stays on. A URL request filter allows only the effect's own
  directory and the `lightloom://` prelude. `engine.*` data (audio bins, screen zones,
  sensors) is pushed as JSON in M3 and read from the shm region by a V8 extension in
  M6.
- `--software-compositing` fallback for VMs/RDP; if `on_paint` cadence stays below
  20 fps for 60 frames the engine downshifts to 15 fps and raises a warning.
  (Amended in M3: software compositing became the default, because at 320x200 it paints
  at a steadier cadence than GPU compositing, whose readback lands on the display's vsync
  grid; GPU compositing is opt-in. Measurements in docs/hardware/effects-m3.md. The
  sandbox is still off until the bootstrap executable can be signed; the URL filter and
  the DNS block are in place.)
- One extra windowless browser per LCD face, at the face's own resolution.
- A `RendererClient` trait in `lightloom-render` keeps a `null` renderer (solid,
  rainbow, test pattern) for M1 and CI; a hi-res waitable timer provides the frame
  clock when no canvas is active.

## Consequences

Positive:

- Effect fidelity is Chromium-vs-Chromium: the M3 harness expects PSNR >= 40 dB
  against a stock browser capture; WebGL, fonts, CSS animations all just work.
- Crash isolation: a misbehaving effect cannot take the daemon or device state down.
- Same renderer serves LCD faces and the dashboard preview.

Negative / accepted costs:

- RSS of 150-250 MB for the renderer process and a CEF binary distribution of
  roughly 100 MB per platform; packaging must ship it (MSI, AppImage, Flatpak).
- Build complexity: `cef-dll-sys` fetches binaries (or `export-cef-dir`), pinned by
  the `+154.0.34` metadata; CI needs the download cached.
- Two IPC mechanisms (shm + control pipe) to keep correct; mitigated by a closed
  message set and `tools/shm-dump`.
- Linux needs `--ozone-platform=headless` in CI and X11/Wayland handling in M7.

## Alternatives considered

- **tiny-skia Canvas2D shim + QuickJS** (original proposal A): rejected for fidelity
  and scope; replaced by this ADR.
- **chromiumoxide / CDP against an external browser**: depends on a user-installed
  browser, screenshot transport is slow and lossy, no sandbox control.
- **Ultralight**: proprietary license; CPU raster only; not redistributable under our
  terms.
- **QtWebEngine**: would force Qt into a Rust core (see ADR-0001) and LGPL linking
  constraints.
- **Electron as the daemon runtime**: renderer story is equivalent, but it makes
  Electron the whole process model (see ADR-0001 alternatives).
- **WebView2 / webkitgtk through Tauri**: platform-specific, offscreen rendering is
  not uniformly available, and it would tie effect rendering to the optional UI shell.
- **Servo**: not mature enough for unmodified community content.
