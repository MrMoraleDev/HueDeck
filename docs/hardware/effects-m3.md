<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Effects pipeline on the reference machine (M3)

Bench of 2026-10-18: Windows 11 Pro, RTX 3060 on a 144 Hz display, release build,
`huedeck-renderer` on CEF 154.0.34 (Chromium 154.0.8037.98). Effects from the SignalRGB
installation are read in place (never copied into this repository). Hardware runs were
made with SignalRGB closed (`taskkill /PID`, relaunched afterwards), after a dry run.

The scripts are in [tools/effects-bench/](../../tools/effects-bench/): `bench.py` for the
paint cadence and memory, `psnr.py` for the comparison with stock Chrome.

## Twenty lightscripts at 30 fps

Each effect painted 300 frames at 30 fps, once with software compositing
(`--software-compositing`) and once with GPU compositing (40 runs). Every run exited 0
with 300 of 300 frames, a measured rate of 29.96 to 30.22 fps and no console errors.

| Effect | Source | What it exercises | p99 paint interval, software / GPU (ms) | Dropped frames | Console errors |
|---|---|---|---|---|---|
| Audio Bars | HueDeck (CC0) | bars from `engine.audio` (silent input: baseline only) | 34.9 / 47.4 | 0 / 0 | 0 / 0 |
| Aurora | HueDeck (CC0) | gradients, `lighter` compositing, `filter: blur()` | 35.9 / 39.4 | 0 / 0 | 0 / 0 |
| Breathing | HueDeck (CC0) | colour controls, `on<prop>Changed` hooks | 35.1 / 46.7 | 0 / 0 | 0 / 0 |
| Conic Spin | HueDeck (CC0) | `createConicGradient` | 35.1 / 47.3 | 0 / 0 | 0 / 0 |
| Fire | HueDeck (CC0) | per-pixel ImageData | 35.3 / 47.5 | 0 / 0 | 0 / 0 |
| Marquee Text | HueDeck (CC0) | text (`fillText`, `measureText`) | 35.2 / 47.7 | 0 / 0 | 0 / 0 |
| Plasma | HueDeck (CC0) | per-pixel ImageData | 37.0 / 48.3 | 0 / 0 | 0 / 0 |
| Rainbow Wave | HueDeck (CC0) | 1-px `fillRect` columns | 35.3 / 47.8 | 0 / 0 | 0 / 0 |
| Screen Ambience | HueDeck (CC0) | per-pixel ImageData from `engine.zone.imagedata` | 35.5 / 47.1 | 0 / 0 | 0 / 0 |
| Shader Waves | HueDeck (CC0) | WebGL fragment shader | 35.0 / 47.1 | 0 / 0 | 0 / 0 |
| Solid Color | HueDeck (CC0) | solid fill | 35.4 / 46.7 | 0 / 0 | 0 / 0 |
| Starfield | HueDeck (CC0) | arcs, translucent trails | 35.2 / 47.9 | 0 / 0 | 0 / 0 |
| Test Pattern | HueDeck (CC0) | bars, ramp, text | 35.5 / 47.3 | 0 / 0 | 0 / 0 |
| Screen Ambience | SignalRGB 2.5.80 (bundled) | `engine.zone` input | 35.1 / 38.1 | 0 / 0 | 0 / 0 |
| Neon Shift | SignalRGB 2.5.80 (bundled) | canvas 2D | 35.0 / 48.7 | 0 / 0 | 0 / 0 |
| Rainbow | SignalRGB 2.5.80 (bundled) | controls read from the first frame | 34.6 / 47.3 | 0 / 0 | 0 / 0 |
| Side to Side | SignalRGB 2.5.80 (bundled) | `on<prop>Changed` hooks, `Math.random` | 35.5 / 48.0 | 0 / 1 | 0 / 0 |
| Solid Color | SignalRGB 2.5.80 (bundled) | drawing from `oncolorChanged` | 35.3 / 47.9 | 0 / 0 | 0 / 0 |
| Cobalt | SignalRGB catalog (cache) | canvas 2D | 35.1 / 47.5 | 0 / 0 | 0 / 0 |
| Black Ice | SignalRGB catalog (cache) | canvas 2D | 34.9 / 46.9 | 0 / 0 | 0 / 0 |

Only seven SignalRGB lightscripts are installed on this machine (five bundled, two from the
catalog cache), so the other thirteen are HueDeck's own CC0 starter effects in
[effects/](../../effects/), written to cover what the milestone asks for: per-pixel
ImageData (Plasma, Fire, Screen Ambience), WebGL (Shader Waves) and text (Marquee Text), plus
canvas filters, additive compositing, conic gradients and change hooks.

Frames 10, 100 and 290 of every run were non-black, except where black is right: both
Screen Ambience effects (no screen capture until M6), the Audio Bars baseline (0.9 % lit with
silent input), Starfield and Fire (dark backgrounds, 7 to 30 % lit) and SignalRGB's Solid
Color at frame 290 (the same in both modes; it was mid-fade).

### Software compositing is the default

| Mode | p99 paint interval | Runs with p99 < 40 ms | Dropped frames | Working set, median / max | Private bytes, median / max |
|---|---|---|---|---|---|
| Software compositing | 34.6 to 37.0 ms, median 35.2 | 20 of 20 | 0 | 280 / 411 MB | 152 / 289 MB |
| GPU compositing | 38.1 to 48.7 ms, median 47.4 | 2 of 20 | 1 | 297 / 310 MB | 160 / 198 MB |

Memory is the renderer's whole process tree (six processes), sampled every 1.5 s; the
working set counts shared pages once per process. The GPU numbers repeat what SPIKE A found
(see the [renderer README](../../crates/huedeck-renderer/README.md#measurements)): with GPU
compositing the frames arrive on the display's vsync grid, so intervals alternate between
late and early while the average stays at 30 fps. So `huedeck effect` and the client
(`CefRendererConfig::software_compositing`) composite in software unless
`--gpu-compositing` is given. WebGL still runs on the GPU in that mode (Shader Waves above).
The one effect that costs more in software is Aurora: a `filter: blur()` over the whole
canvas every frame peaked at 411 MB working set and 289 MB private bytes, against 310 and
198 MB with GPU compositing. That is what `--gpu-compositing` is for.

## Same pixels as stock Chrome

`psnr.py` renders each of the twenty effects in `huedeck-renderer` and in Chrome 155.0.8059.84
headless and compares the frames. To make one frame deterministic, the script wraps the effect
in a copy that:

- freezes time (`requestAnimationFrame` timestamp, `performance.now`, `Date`);
- seeds `Math.random`;
- defines the control defaults and a zeroed `window.engine`;
- runs SignalRGB's load sequence in Chrome (HueDeck's renderer injects its own);
- lets exactly one animation frame run, after `load` and the hooks.

Both browsers render that copy with the same compositing mode.

**Result: 40 of 40 comparisons (20 effects, software and GPU compositing) are bit-identical
(PSNR infinite, 0 differing pixels).** The milestone asks for at least 40 dB. Two
early mismatches were harness bugs, not renderer differences:

- Comparing software-composited HueDeck frames with GPU-composited Chrome screenshots gave
  39.8 to 58 dB on arcs, blurs and gradients.
- The harness's regex read a `<meta>` tag inside an HTML comment of SignalRGB's Side to Side,
  which HueDeck's parser (html5ever) correctly ignores.

## Four devices

`huedeck effect <effect> --signalrgb` with the four lighting devices of the reference machine
(MSI board with HueDeck's plugin, the others with SignalRGB's plugins read in place),
software compositing, 30 fps.

| Device | Plasma, 45 s: frames | canvas to wire p50 / p99 / max | Shader Waves, 25 s: frames | canvas to wire p50 / p99 / max | HID errors (Plasma / Shader Waves) |
|---|---|---|---|---|---|
| MSI MAG B550 Tomahawk `1462:7c91` | 1332 | 3.78 / 4.31 / 4.50 ms | 731 | 3.78 / 4.32 / 4.46 ms | 0 / 0 |
| Razer DeathAdder V2 Pro `1532:007c` | 1081 | 3.30 / 3.94 / 31.6 ms | 480 | 3.10 / 3.77 / 17.8 ms | 0 / 0 |
| NZXT Kraken X3 `1e71:2007` | 1332 | 1.70 / 2.24 / 3.94 ms | 731 | 1.70 / 2.25 / 5.55 ms | 0 / 0 |
| SteelSeries Arctis 5 `1038:12aa` | 1332 | 1.00 / 2.26 / 33.1 ms | 731 | 0.90 / 2.21 / 31.6 ms | 3 of 15997 writes / 0 |

- **Latency.** Canvas to wire is the time from the renderer's frame entering the frame store
  to the completion of the first HID write of the `Render()` that used it. Every device is
  far inside the milestone's p50 10 ms and p99 25 ms.
- **Razer.** It renders fewer frames because SignalRGB's plugin sets the mouse up lazily in its
  first `Render()` (about 8 s, see [imported-plugins-m2.md](imported-plugins-m2.md)).
- **Arctis errors.** The Arctis's three errors were `ERROR_GEN_FAILURE` on single output
  reports, the same transient failure seen in M2. The plugin went on, and the run exits 1
  because HID errors occurred.
- **Memory.** The renderer tree had a working set of 284 to 291 MB and private bytes of 154 to
  159 MB, within the milestone's 300 MB. `huedeck` itself had 25.5 MB working set and 17.8 MB
  private bytes.
- **Renderer status.** The renderer reported `1 start, 1334 frames painted, 0 held frames
  republished`.

## Renderer failures

Killing the renderer process during an effect run, the client held the last frame on the
devices and started a new renderer after 1 s. The longest gap between the last frame of the
old renderer and the first frame of the new one was **1522 ms**, under the milestone's 3 s
(`renderer: 2 starts, 486 frames painted, 6 held frames republished, longest recovery
1522 ms`). The client test `a_crashed_renderer_is_restarted_holding_the_last_frame` covers
the same path against a stand-in renderer on CI.

## Inputs and isolation

- **`engine` command.** The `engine` control command pushed audio, zone and sensor data into a
  probe effect. The typed arrays were filled in place (the page's saved reference saw the new
  values), `getSensorValue` returned the pushed sensor, and a `__proto__` key changed nothing.
  The renderer README's [serve mode section](../../crates/huedeck-renderer/README.md#serve-mode-and-control-commands)
  has the details. M6 connects real audio and screen capture to it.
- **URL filter.** A probe effect could load an image from its own directory and a `data:`
  image. It could not:
  - load an image from the parent directory;
  - `fetch` `http://142.250.180.14/` (a literal IP, which the DNS block alone lets through);
  - frame `file:///C:/Windows/win.ini`;
  - navigate to `file:///C:/Windows/`;
  - open a popup.

  The report listed the four refused URLs. None of the twenty effects above requests anything
  outside its directory.
- **Sandbox.** The Chromium sandbox is still off: CEF 154 needs a signed bootstrap executable
  for it (see the renderer README). The URL filter, the DNS block and the closed control
  channel are the isolation until releases are signed.

## Milestone M3 acceptance

| Criterion | Result |
|---|---|
| 20 lightscripts unmodified at 30 fps, incl. 3 per-pixel ImageData, 1 WebGL, 1 text | 20 at 29.96-30.22 fps, no errors; ImageData x3, WebGL x1, text x2. Seven are SignalRGB's, thirteen HueDeck's own. |
| PSNR >= 40 dB vs stock Chromium | bit-identical in 40/40 comparisons |
| Canvas to wire p50 <= 10 ms, p99 <= 25 ms with 4 devices | p50 0.9-3.8 ms, p99 2.2-4.3 ms |
| Renderer RSS <= 300 MB | 284-291 MB working set, 154-159 MB private (Aurora with software compositing: 411 MB) |
| Crash recovered < 3 s holding the last frame | 1522 ms |
