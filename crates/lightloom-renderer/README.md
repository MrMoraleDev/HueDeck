<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# lightloom-renderer

Offscreen Chromium (CEF, via the [`cef`](https://crates.io/crates/cef) crate 154.5.0+154.0.34,
Apache-2.0 OR MIT; CEF itself is BSD-3-Clause) rendering a SignalRGB-style lightscript at
**320x200**. Every `OnPaint` BGRA buffer goes into a memory-mapped frame ring with the
`lightloom_protocol::shm` layout. After `--frames` frames the binary prints paint-interval
statistics as JSON. With `--serve` (M3) it runs until stdin closes instead, taking control
commands from it; this is how `lightloom effect` and the daemon use it through
`lightloom_render::CefRenderer`. See [Serve mode](#serve-mode-and-control-commands).

**SPIKE A status, 2026-10-18 (Windows 11, RTX 3060 on a 144 Hz display, SignalRGB running alongside),
re-measured by review:** SPIKE A passes with `--software-compositing` and with `--disable-gpu`
(12/12 runs each: 300/300 frames, p99 34.6-35.5 ms, 0 dropped). With the default GPU
compositing all 300 frames still arrive at 30 fps with 0 dropped, but p99 < 40 ms holds in
only 2 of 12 runs (p99 37.9 ms or 45.8-48.5 ms). Details in [Measurements](#measurements).

```text
lightloom-renderer --effect <path.html> --out <ring.shm> [--frames 300] [--fps 30]
                 [--png <snap.png>] [--snapshot-frames 10,100] [--software-compositing]
                 [--disable-gpu] [--chromium-flag name[=value]]... [--stats <report.json>]
                 [--cef-log <file>] [--cef-cache <dir>] [--timeout-factor 3] [--raw-intervals]
                 [--allow-network] [--no-inject] [--verbose] [--serve] [--prop name=value]...
```

Exit codes:

- `0`: all `--frames` frames were published within `timeout-factor × frames/fps` seconds of
  browser creation, and the page loaded.
- `1`: too few frames, a timeout or a load error. A non-animating page paints only once, so it
  also ends here.
- `2`: a setup error (bad path, the shm file, a `--cef-cache` dir we refuse to use, or
  `CefInitialize`), or a command-line error (`--frames 1`, `--timeout-factor` outside 1..=1000).

The JSON report is always printed to stdout. It holds frames, elapsed time, effective fps, the
paint interval p50/p95/p99/max/mean/min, `dropped` (intervals longer than 2 × the period), the
publish cost, and timings (CEF init, browser created, load end, first paint). It also holds,
per snapshot frame, a SHA-256 hash of the BGRA bytes, the non-black fraction, mean luma and the
PNG path. The rest is the shm geometry, load status, the mode (`software_compositing`,
`disable_gpu`, `sandbox`, `network_blocked`, the Chromium switches), what happened to the cache
dir (`cef_cache_prepared`, `cef_cache_removed`), `skipped_controls` and the first 40 console
lines.

## Serve mode and control commands

```text
lightloom-renderer --serve --effect <path.html> --out <ring.shm> [--fps 30] [--prop name=value]...
                 [--software-compositing] [--control stdin|unix:PATH]
```

`--serve` drops the frame limit and the deadline: the renderer paints into the ring until it is
told to stop. Paint statistics cover the last 1800 paints (about a minute at 30 fps); the JSON
report is printed when it exits. `--prop` sets a control's starting value; the value is parsed
as JSON, else taken as a string, and coerced to the control's type (`ControlMeta::coerce`: a
number control takes `5` or `"5"`, a boolean `true`, `"true"` or `1`, a colour `#rrggbb`).

The control channel carries one JSON object per line (`src/control.rs`). It is stdin by default
(and what lightloomd uses on Windows); `--control unix:PATH` makes the renderer connect to a unix
domain socket instead (what lightloomd uses on Linux, see "Linux" below). Lines are read with a
bound of 512 KB each (`ipc::read_bounded_line`), so a peer that never sends a newline cannot
make the renderer allocate without limit.

| command | effect on the page |
|---|---|
| `{"cmd":"set","values":{"speed":5,"color":"#ff0000"}}` | each known control is coerced, assigned to its global, then its `on<name>Changed()` runs; unknown or non-injectable names are ignored with a warning on stderr |
| `{"cmd":"event","sender":"discord","event":"message"}` | `onCanvasApiEvent({sender, event})`, as SignalRGB's `POST /canvas/event` |
| `{"cmd":"tap","x":120,"y":40}` | `onCanvasTapped(x, y)` in canvas pixels |
| `{"cmd":"fps","value":15}` | `SetWindowlessFrameRate` (the client's 15 fps downshift) |
| `{"cmd":"engine","data":{"audio":{"level":-20,"freq":[...]}}}` | merged into `window.engine` (below) |
| `{"cmd":"quit"}` | close the browser and exit; the end of stdin does the same, so a renderer never outlives its parent |

Strings are embedded as JSON string literals, never spliced into code, and every hook call is
wrapped in `try`/`catch` that reports to the console. Lines longer than 512 KiB are refused.

**Engine data.** Only `audio`, `zone`, `vision` and `sensors` are accepted. Objects merge
recursively; an array fills the page's existing typed array in place (`engine.audio.freq`,
`engine.zone.hue|saturation|lightness|imagedata`), truncated to its length, so effects that kept
a reference to the array see the new values; any other value replaces. Only own keys are copied
and `__proto__` is skipped. `engine.getSensorValue(name)` returns `engine.sensors[name]` or
`null`. Verified live (CEF 154): after one push a probe effect logged `level=-20 freq0=5
freq199=9 sameArray=true hue0=123 sensor=55C`, `({}).polluted` stayed `undefined`, and a
`getSensorValue` key in the data was ignored. The whole 160x100 zone image as a JSON array is
about 250 KB per push; M6 moves bulk input data to shared memory.

**Load sequence.** The injected script runs in `OnContextCreated`, before any page script:

1. `window.engine` with zeroed inputs.
2. `var <control> = <value>;` for every injectable control, so page code can read controls
   from its first line and its first animation frame.
3. A `load` listener that assigns every control again (a page that declares `var speed = 1`
   itself would otherwise win; SignalRGB evaluates the values after the page scripts), calls
   every `on<control>Changed()` once, then `onEngineReady()`.

`lightloom_render::CefRenderer` drives this process: it polls the ring every 2 ms, republishes
the last frame when the page paints nothing for two frame periods, restarts a renderer that
exits (1 s doubling to 30 s, holding the last frame), downshifts to 15 fps below 20 fps, and
replays the latest control values on a restart's command line.

## Building

```sh
. crates/lightloom-renderer/tools/cef-env.sh                  # CMake + Ninja on PATH (Git Bash)
cargo build --release -p lightloom-renderer --features cef     # `cef` is opt-in
target/release/lightloom-renderer --effect crates/lightloom-renderer/testdata/spike.html \
    --out "$TEMP/hd.shm" --png "$TEMP/hd.png" --software-compositing
```

The `cef` feature is **off by default**, so `cargo build|test|clippy --workspace` and CI's main
jobs never download or build Chromium. Without it, `cargo test -p lightloom-renderer` still builds
and tests all the plumbing: ring, sink, stats, snapshots, injection script, CLI and URLs. A
binary built that way only prints "built without the `cef` feature" and exits 2. CI builds and
lints the real CEF host in its own Windows job (`renderer-cef` in `.github/workflows/ci.yml`)
with a cached `CEF_PATH`.

### How cef-dll-sys gets CEF (read from `cef-dll-sys-154.5.0+154.0.34/build.rs`, `download-cef-3.0.0`)

- **Source.** The CEF build index at `https://cef-builds.spotifycdn.com` (override with
  `CEF_DOWNLOAD_URL`). It takes the **minimal** distribution of the version pinned by the crate
  version: `cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_windows64_minimal.tar.bz2`,
  172,752,666 bytes. Unpacked it is 424 MB, 49 MB of which is `locales/` (220 files). The SHA-1
  is checked and recorded in `archive.json`.
- **Where it lands.** That depends on `CEF_PATH`:
  - `CEF_PATH` unset: everything goes into the build script's `OUT_DIR`
    (`target/<profile>/build/cef-dll-sys-<hash>/out/cef_windows_x86_64`). Each build-script
    hash gets its own download. Workspace-wide commands, `-p` commands, test builds and release
    builds can each have a different hash, and so can every dependency change by any crate in
    the workspace. Each costs about 720 MB under `target/`.
  - `CEF_PATH=<dir>`: the build script uses `<dir>/154.0.34/cef_windows_x86_64`. If `<dir>`
    itself holds a matching `archive.json`, it uses `<dir>`. Otherwise it downloads into
    `<dir>/154.0.34/`. The shared cache we use is `%LOCALAPPDATA%\lightloom\cef`, which is
    outside the repo and the Nextcloud tree, so nothing needs to go in `.gitignore`. The repo's
    `.gitignore` already ignores `/.cef/` in case someone keeps it in-tree.
  - **Pitfall.** The build script declares `rerun-if-env-changed=CEF_PATH`. Shells that
    disagree about `CEF_PATH` keep re-running it, which needs CMake again, and without
    `CEF_PATH` it downloads again. Either never set it, or set it **persistently for every
    shell** (a user environment variable). `tools/cef-env.sh` therefore does not set it unless
    you pass `LIGHTLOOM_CEF_PATH=...`.
- **Build.** The build script then builds `libcef_dll_wrapper` (C++, about 214 objects)
  with the **CMake + Ninja** generator. It uses `RelWithDebInfo` and `/MT`, and compiles with
  `CEF_API_VERSION=15400`. Neither tool is on PATH by default. Visual Studio Build Tools ships
  both under `Common7/IDE/CommonExtensions/Microsoft/CMake/{CMake/bin,Ninja}`, and
  `tools/cef-env.sh` adds them. This takes about 1 min per build-script hash.
  - **Long paths.** CMake's try-compile fails with `ninja: error: loading 'build.ninja':
    GetLastError() = 3` when the target dir path is too long. It failed with a target dir under
    `%TEMP%\claude\...\scratchpad`, and works with the repo's `target/`. Use a short
    `CARGO_TARGET_DIR` if that happens.
- **Linking.** The final binary links `libcef_dll_wrapper.lib`, `libcef.lib` (an import library,
  so the exe imports `libcef.dll`) and 14 Windows SDK libraries.

### Runtime files next to the exe

cef-dll-sys's build script copies the **whole top level** of the CEF directory plus `locales/`
into `target/<profile>/`. That is why `cargo run` and `target/<profile>/lightloom-renderer.exe`
just work. Test executables live in `target/<profile>/deps/`, which has no CEF files. So the
library never references `cef` and the bin target has `test = false`. Otherwise
`cargo test -p lightloom-renderer` would fail to start with `STATUS_DLL_NOT_FOUND`.

What a packaged renderer needs next to the exe (sizes from 154.0.34):

| file | bytes | why |
|---|---:|---|
| `libcef.dll` | 289,818,624 | CEF + Chromium |
| `chrome_elf.dll` | 2,829,312 | crash / ELF hooks, loaded by libcef |
| `icudtl.dat` | 10,819,840 | ICU data |
| `v8_context_snapshot.bin` | 757,576 | V8 startup snapshot |
| `resources.pak`, `chrome_100_percent.pak`, `chrome_200_percent.pak` | 23.4 MB + 0.6 MB + 0.9 MB | resources |
| `locales/*.pak` | 49 MB (en-US.pak alone is 589,612) | UI strings; `en-US.pak` is enough with `locale=en-US` |
| `d3dcompiler_47.dll`, `dxcompiler.dll`, `dxil.dll` | 4.8 + 25.8 + 1.5 MB | ANGLE / D3D shader compilers (GPU process) |
| `vk_swiftshader.dll`, `vk_swiftshader_icd.json`, `vulkan-1.dll` | 5.4 + 0.0 + 1.0 MB | software GL/Vulkan fallback (`--disable-gpu`, WebGL without a GPU) |
| `bootstrap.exe` / `bootstrapc.exe` | 4.3 MB each | only for the sandboxed layout (see below) |

`build.rs` embeds `lightloom-renderer.exe.manifest`. The manifest declares Windows 7 through 11
as supported OSes (supportedOS GUIDs) and loads Common Controls v6, the same shape as CEF's
own sample manifests. The embedding was verified: the Win10 GUID string is present in the exe.

## Linux (WP-L5)

The renderer and the daemon-side client (`lightloom_render::CefRenderer`) share all code with
Windows; the per-platform parts are in `src/ipc.rs`, `cli.rs` and `build.rs`. Status: the whole
crate graph, including the `cef` host, is compile-checked for `x86_64-unknown-linux-gnu`
(`cargo clippy --target x86_64-unknown-linux-gnu -p lightloom-renderer --features cef,cef/dox`:
`cef/dox` skips cef-dll-sys's download, the bindings are pre-generated per target). Building,
linking and running against `libcef.so` happens only in CI (`renderer-cef-linux`).

- **Building.** `cargo build -p lightloom-renderer --features cef`. On Linux cef-dll-sys builds no
  C++ wrapper (no CMake, Ninja or C++ compiler): it resolves the CEF distribution like on
  Windows (`CEF_PATH/154.0.34/cef_linux_x86_64`, the linux64 *minimal* archive, 326.5 MB bz2),
  links `libcef.so` and copies the runtime files next to the binary: `libcef.so`, `icudtl.dat`,
  `v8_context_snapshot.bin`, `chrome-sandbox`, `libvk_swiftshader.so`, `libvulkan.so.1`,
  `vk_swiftshader_icd.json`, the `.pak` files and `locales/`. `build.rs` adds an `$ORIGIN`
  rpath, so the binary (and every Chromium subprocess, which is the same binary) finds
  `libcef.so` without `LD_LIBRARY_PATH`. CI checks it with `readelf -d` and `ldd`.
- **No display.** Windowless (OSR) browsers paint into `OnPaint` buffers and need no window.
  On Linux the default switches add `--ozone-platform=headless` (software output, no X11 or
  Wayland connection). CEF's maintainer called Ozone headless unsupported (2020), but CEF OSR has
  been run with it since M133 (CEF issue #3902). Escape hatches: `--chromium-flag=ozone-platform=x11`
  (with Xvfb or a desktop session), or `LIGHTLOOM_OZONE_PLATFORM=x11|wayland` in the environment
  for a renderer lightloomd starts. CI tries headless first and retries once under `xvfb-run`
  with X11, with a warning, so the log shows which one worked.
- **Shared memory.** The ring stays a mapped file with the `lightloom_protocol::shm` layout, but on
  Linux it lives on a tmpfs: `$XDG_RUNTIME_DIR` (per user, 0700), else `/dev/shm`, else the work
  dir. A file in `/dev/shm` is exactly what `shm_open` creates (glibc maps `shm_open("/x")` to
  `/dev/shm/x`), while both processes can still open it by path. `memfd_create` was not used:
  an anonymous memfd would have to be passed across `exec` (`pre_exec` + `dup2`, `unsafe` in a
  `forbid(unsafe_code)` crate) or opened through `/proc/<pid>/fd` (subject to Yama). Each run
  gets a private directory (`ipc::PrivateDir`: 0700, unique name, never adopted if it exists)
  holding `frames.shm` (0600) and the control socket, removed when the run ends.
- **Control channel.** lightloomd binds `control.sock` in that directory, starts the renderer with
  `--control unix:PATH` and stdin at `/dev/null`, and accepts the connection while it waits for
  the ring. The renderer connects right after creating the ring, before CEF initialises; lines
  sent earlier wait in the socket buffer, and lines the daemon sends before the accept are
  buffered on its side (up to 4 MB) and flushed on accept. Writes time out after 2 s. The
  framing is the same JSON lines as stdin, and end of stream still means "the daemon is gone".
  Why not stdin: the accepted connection proves the renderer got through its start-up, launch
  wrappers that do not forward stdin (AppImage runtimes, `flatpak-spawn`, zypak) do not matter,
  and stdin stays free. If the socket cannot be bound (a `sun_path` over 107 bytes), the client
  falls back to stdin.
- **Sandbox.** Off (`no_sandbox = 1`), as on Windows. The setuid `chrome-sandbox` helper needs
  `root:root 4755` next to the binary, and Ubuntu 24.04+ restricts unprivileged user namespaces
  to binaries with an AppArmor profile granting `userns` (an AppImage's random mount path cannot
  be matched). The Linux packages (WP-L7: `tools/package/stage-linux.sh`, tarball and AppImage)
  therefore ship without `chrome-sandbox` and run unsandboxed; effects are local files loaded
  with DNS blocked.
- **System libraries.** Chromium's runtime list (NSS, GBM, ALSA, CUPS, ATK/AT-SPI, Pango/Cairo,
  xkbcommon and the X11/XCB client libraries, even with Ozone headless, because `libcef.so`
  links them). The CI job installs the Ubuntu 24.04 names (`t64` variants).
- **Not verified yet** (needs CI or a Linux machine): that headless paints at the requested
  rate; WebGL lightscripts (Ozone headless is software only; they would need
  `--use-angle=swiftshader-webgl --enable-unsafe-swiftshader`); a sandboxed layout; Wayland.

## Process and message-loop model

- **One exe for every process.** `main` checks for `--type=` first. If it is there, this is a
  Chromium subprocess (renderer, GPU, utility): `cef::execute_process` runs it and we exit with
  its code, never touching our CLI parser. Otherwise this is the browser process.
  `execute_process` returns -1 there, and then `cef::initialize` runs with:
  - `windowless_rendering_enabled=1` and `no_sandbox=1` (see below).
  - `command_line_args_disabled=1`, so our `--frames 300` is not read as Chromium switches or a
    startup URL. Our switches are added in `OnBeforeCommandLineProcessing`.
  - A `root_cache_path` per ring: `--cef-cache`, default `<out>.cef-cache`. Two live renderers
    sharing one would let Chromium's process singleton hand the second's command line to the
    first. Every run starts from an empty profile (`src/cache.rs`):
    - The dir is only ever wiped when it carries the marker file `.lightloom-renderer-cache`,
      which this tool writes when it creates (or adopts an empty) dir. A non-empty dir without
      the marker, or a file, is refused with exit 2 and left untouched. (Before review, the
      path was passed straight to `remove_dir_all`, so `--cef-cache <any dir>` deleted it.)
    - The browser process keeps the profile's LevelDB locks (`Session Storage`,
      `shared_proto_db`, `Site Characteristics Database`) until it exits, also after
      `CefShutdown`; 20 retries over 2 s did not release them. So the end-of-run removal
      deletes everything else and keeps the marker; the next run with the same `--out` wipes
      the rest. The report says `cef_cache_prepared` and `cef_cache_removed`.
    - Keep the path short. Under a ~150-character dir, Chromium logs `Failed to open
      persistent cache files in directory ...\GPUPersistentCache\DawnGraphiteCache\<hash>,
      error 2 (filename too long)` to stderr; the run is unaffected, only the GPU cache is not
      persisted.
  - `log_file=<out>.cef.log` and `log_severity=WARNING`.
- **Message loop.** We use **`cef::run_message_loop()` on the main thread**: CEF's own Win32
  loop. The browser process has nothing else to do, and it gives the most regular `OnPaint`
  cadence.
  - The run ends in one of three ways. `OnPaint` reaches N frames and calls
    `CloseBrowser(force)`. A main-frame load error closes the browser. Or the deadline task
    fires; it is posted with `post_delayed_task(TID_UI)`. Then `OnBeforeClose` calls
    `quit_message_loop()`, and `cef::shutdown()` follows.
  - The alternatives. `do_message_loop_work()` on our own timer is the pattern CEF discourages:
    each call does a bounded amount of work, so at 30 fps it either busy-polls or adds latency.
    `external_message_pump=1` with `OnScheduleMessagePumpWork` is the proper way to embed into a
    foreign loop. `multi_threaded_message_loop=1` (Windows-only) runs CEF's UI thread separately.
  - M3 kept `run_message_loop` on the main thread. The control pipe runs on its own thread
    (`lightloom-control`) and hands commands to CEF with `post_task(TID_UI, ...)`.
- **Pacing.** We set `BrowserSettings.windowless_frame_rate = --fps`. CEF 154's header gives
  only a minimum (1), and it is not capped at 60: spike.html measured 60.4 / 60.1 fps at
  `--fps 60` and 120.4 / 120.0 fps at `--fps 120` (GPU / software compositing, 240 frames, 0
  dropped). Chromium's own begin-frame timer drives rendering. `OnPaint` runs on the UI thread
  **only when something was damaged**. A static page paints once, so the daemon must hold the
  last frame; the alternative is `BrowserHost::invalidate(PET_VIEW)` on a timer.
- **Size.** `GetViewRect` returns 320x200, and `GetScreenInfo` returns a device scale factor of
  1, so buffers are exactly 320x200 on HiDPI desktops. A size mismatch is counted and not
  published.
- **Injection.** Before any page script runs, we inject a zeroed `window.engine`:
  - `engine.audio` is `{freq: Int8Array(200), level: -100, rawlevel: 0, width: 0, density: 0}`.
  - `engine.zone` is `{hue: Int16Array(560), saturation: Int8Array(560), lightness: Int8Array(560), imagedata: Uint8ClampedArray(64000)}`.
  - `engine.vision` and `engine.sensors` are `{}`, and `getSensorValue` returns `null`.

  Two more things go in with it. Each `<meta property>` control becomes a `var` global set to
  its default; Rainbow reads `speed` and `vertical` from its first animation frame. A `load`
  listener then calls each `on<prop>Changed()` once and `onEngineReady()`.

  Mechanics: the browser process builds the script (`inject::injection_script`, which uses
  `lightloom_effects::parse_effect_meta`) and passes it in `extra_info`. The render process stores
  it in `OnBrowserCreated`. In `OnContextCreated` (main frame only), `CefV8Context::Eval`
  evaluates it. It is verified: `spike.html` logs
  `engine=object freq=200 zone.hue=560 speed=3 barColor=#ffffff`, which appears in the report's
  `console`, and draws `engine=object ready=true` into the frame. M3 completed the load
  sequence; see [Serve mode](#serve-mode-and-control-commands).
  - Controls whose name cannot be a page global are skipped and listed in the report's
    `skipped_controls`: non-identifiers, ECMAScript reserved words (a single `var default = 1;`
    is a SyntaxError that would drop the whole script, `window.engine` included), and globals
    such as `engine`, `window`, `document` or `location` (`var location = 5` navigates away).
  - If the evaluation still throws, the render process logs it with `console.error`, so it
    reaches the report's `console` / `console_errors` (render-process stderr is not visible).
- **Switches** (browser process): `disable-background-timer-throttling`,
  `disable-renderer-backgrounding`, `disable-backgrounding-occluded-windows`, `mute-audio`,
  `disable-extensions`, `disable-component-update`, `no-first-run`,
  `no-default-browser-check`, `disable-default-apps` and, unless `--allow-network`,
  `host-resolver-rules=MAP * ~NOTFOUND`. `--software-compositing` adds
  `disable-gpu-compositing`. `--disable-gpu` adds `disable-gpu` and `disable-gpu-compositing`.
  Anything else goes through `--chromium-flag`.
- **Network.** Without the resolver rule, a 10 s run of the local spike.html opened 4 TCP
  connections to Google hosts (`34.104.35.123:80`, `142.251.x.x:443`; component updater /
  optimization guide / network time) plus 4 UDP sockets, all from the
  `network.mojom.NetworkService` utility process. `disable-background-networking` and
  `disable-features=OptimizationGuideModelDownloading,OptimizationHints,NetworkTimeServiceQuerying,...`
  did not stop them. With the rule (now the default) a run shows 0 TCP/UDP endpoints in the
  whole process tree. Effects are local files, and docs/architecture.md wants "URL filter
  restricted to the effect directory" and "no telemetry". Literal-IP URLs pass the resolver
  rule, so M3 added the URL filter (`url::UrlFilter`, applied by a `CefRequestHandler` in the
  browser process to every navigation and resource request). It allows files in the effect's
  directory and below, `data:`, `blob:`, `about:blank` and `about:srcdoc`; `http(s)` and
  `ws(s)` only with `--allow-network`; it cancels everything else and every popup. The report
  counts refused requests (`blocked_requests`) and lists the first 20 (`blocked_urls`).
  Verified with a probe effect: a parent-directory image, `fetch("http://142.250.180.14/")`,
  an iframe on `file:///C:/Windows/win.ini`, a navigation to `file:///C:/Windows/` and
  `window.open` were all refused; a same-directory image and a `data:` image loaded.

## shm ring (`src/ring.rs`)

The ring is a file mapped read-write with `memmap2`, laid out exactly as in
`lightloom_protocol::shm`: a 64-byte header, `seq`, three 320x200 BGRA slots and per-slot seqlock
generations. The protocol is unchanged; nothing was added to lightloom-protocol.

- **Writer.** It stamps `gen[slot]` odd, then `fence(Release)`, then stores the pixels as
  relaxed `AtomicU32`. It then stores `gen` with the next even value (Release), then
  `seq + 1` (Release). Header words are stored and loaded as little-endian, as the protocol
  says (a no-op on x86_64/aarch64).
- **Reader** (`RingReader`, which goes to `lightloom-render`'s future `CefRenderer`). It loads
  `seq` (Acquire), checks that `gen` is even **and equals `2 * ((seq - 1) / 3 + 1)`**, copies
  the slot, issues `fence(Acquire)`, and checks `gen` again. It retries up to 64 times. The
  generation check (added in review) catches a writer that lapped the reader between its `seq`
  and `gen` loads; before, the reader returned the newer frame's pixels labelled with the old
  `seq`. `ring::tests::reader_labels_match_pixels_when_the_writer_laps` covers it with a
  crafted header (it fails when the check is removed).
- **Atomics.** Every shared word is accessed atomically, so the in-process tests do not race.
- **Publish cost (release).** p99 0.03 ms with software compositing and 0.11-0.21 ms with GPU
  compositing. A debug build takes about 1.1 ms because of the per-pixel atomics.
- **Cross-process check** (review, own reader `xreader.py`, final binary): a Python process
  polled the ring file while the renderer wrote it. It saw seq 1..300 in order with 0 gaps, 0
  torn reads, 0 generation/label mismatches, 300 non-black and 300 distinct frames, and final
  generations `(200, 200, 200)`.
- **Production.** Windows could use a named section (`CreateFileMapping(INVALID_HANDLE_VALUE,
  name)`) instead of a file; `memmap2` cannot create named sections, which is why it is a file.
  On Linux the file lives on a tmpfs, which makes it the same object `shm_open` returns (see
  "Linux").

## Tests (no CEF, no GPU, no hardware)

- `src/*` unit tests (16): percentiles, interval statistics, snapshot metrics and PNG names,
  JS identifier, reserved-word skipping and injection script, `file://` URLs, CLI (defaults,
  DNS-block switch, `--timeout-factor` validation, saturating deadline), ring generations /
  little-endian header / lapped-reader labels, and the cache-dir rules (create, wipe, adopt an
  empty dir, refuse a foreign dir or a file without touching it).
- `tests/fake_paint.rs` (4):
  - `header_layout_on_disk` checks the header byte layout in the file.
  - `fake_on_paint_source_fills_ring_and_stats`: a fake `OnPaint` source feeds 120 frames at
    30 fps ±2 ms with one 90 ms hitch. After every frame, a reader on a second read-only
    mapping sees `seq == tick` and identical bytes. The test then checks the generations
    `[80, 80, 80]`, p50 ≈ 33.3 ms, p99 < 40 ms, `dropped == 1`, and the raw intervals. It also
    checks that frames 10 and 100 are non-black with different hashes, and that the PNGs decode
    with the right BGRA→RGB conversion.
  - `size_mismatch_is_rejected_not_published` checks that a 640x400 (DPI-scaled) frame and a
    short buffer are counted but not published.
  - `concurrent_reader_never_sees_torn_frames`: a writer thread publishes 3000 uniform frames
    while a reader thread on another mapping asserts that it never reads a mixed frame and that
    the returned `seq` matches the frame's fill byte. (The lap it guards against is rare under
    this stress test; the deterministic check is the ring unit test above.)

## Chromium sandbox

The plain `lightloom-renderer.exe` runs **unsandboxed**. CEF 138+ on Windows supports the
sandbox only through `bootstrap.exe` / `bootstrapc.exe`, and the 154.0.34 binaries are
unsigned. The bootstrap executable is renamed to `<app>.exe` and loads `<app>.dll`, which must
export `RunWinMain` or `RunConsoleMain(argc, argv, sandbox_info, version_info)`
(`include/cef_sandbox_win.h`). The `sandbox_info` it receives is then passed to
`execute_process` and `initialize`, with `no_sandbox=0`.

**Verified in a scratch crate outside the repo** (not committed). The steps:

1. Build a `cdylib` that includes this crate's unchanged `src/cef_host.rs` via `#[path]` and
   exports `RunConsoleMain`. Its dependencies are `cef` with `default-features = false,
   features = ["sandbox"]`, which builds the wrapper with `USE_SANDBOX=ON`.
2. Build it with a short `CARGO_TARGET_DIR`. Copy `bootstrapc.exe` to `hd_sandbox_exp.exe` in
   the output directory. Run it with **absolute** paths: relative `--effect` paths failed with
   "path not found" in this mode.

What it showed:

- The run passed with `"sandbox": true`. With GPU compositing, spike.html gave 300 frames,
  p50 33.3, p95 34.8, p99 35.2 and max 37.1 ms, with 0 dropped. Frames 10 and 100 had the same
  hashes as the unsandboxed run.
- `Win32_Process` command lines during the run: the sandboxed build's gpu-process, utility and
  renderer children have **no** `--no-sandbox`. The plain exe's children all carry
  `--no-sandbox`.
- **Token integrity levels** (review, `OpenProcessToken` + `TokenIntegrityLevel` +
  `IsTokenRestricted` on every process of a live run of the scratch build, which re-ran at
  p99 36.2 ms with `"sandbox": true`): both renderers **Untrusted, restricted token**;
  gpu-process **Low, restricted**; `storage.mojom.StorageService` **Untrusted, restricted**;
  `network.mojom.NetworkService` and `chrome.mojom.ProcessorMetrics` Medium (not sandboxed by
  this CEF build); browser Medium. The plain in-repo exe: all six processes Medium, unrestricted.

**To ship it (M3):**

1. Move `cef_host` into its own `crates/lightloom-renderer-host` crate with
   `crate-type = ["cdylib"]`. It cannot live in this lib: the lib would then import libcef and
   its tests could not run.
2. In packaging, install `bootstrapc.exe` as `lightloom-renderer.exe` next to
   `lightloom-renderer.dll`.
3. Sign both with the same certificate. Once bootstrap is signed, CEF requires the DLL to carry
   the same signature.

## Measurements

Final binary after review (DNS blocked by default), release build, 300 frames at 30 fps,
`--snapshot-frames 10,100 --raw-intervals`, run sequentially. Three passes of the matrix; each
cell lists pass 1 / pass 2 / pass 3. Every one of the 18 runs: exit 0, `ok`, 300/300 frames,
0 dropped, frames 10 and 100 non-black with different SHA-256.

| effect | mode | p50 | p95 | p99 | max | fps | first paint |
|---|---|---|---|---|---|---|---|
| testdata/spike.html | GPU compositing (default) | 33.1 / 33.3 / 33.3 | 44.1 / 35.7 / 38.0 | **48.5 / 37.9 / 46.4** | 49.9 / 38.3 / 47.5 | 30.15 / 30.12 / 30.14 | 278 / 265 / 270 |
| testdata/spike.html | `--software-compositing` | 33.3 / 33.3 / 33.4 | 34.7 / 34.5 / 34.4 | **35.4 / 35.0 / 35.0** | 37.8 / 40.3 / 36.9 | 30.08 / 29.98 / 29.99 | 215 / 215 / 276 |
| testdata/spike.html | `--disable-gpu` | 33.4 / 33.4 / 33.3 | 34.4 / 34.4 / 34.4 | **34.6 / 34.9 / 34.9** | 34.9 / 35.3 / 35.1 | 30.10 / 30.03 / 30.03 | 90 / 98 / 115 |
| SignalRGB `Effects/Static/Rainbow.html` (read in place) | GPU compositing | 33.2 / 33.3 / 33.2 | 40.2 / 41.4 / 42.1 | **46.3 / 45.8 / 46.0** | 47.8 / 46.0 / 46.9 | 30.14 / 30.14 / 30.18 | 238 / 237 / 216 |
| same | `--software-compositing` | 33.3 / 33.3 / 33.4 | 34.4 / 34.6 / 34.7 | **34.8 / 35.0 / 35.1** | 35.2 / 44.9 / 35.4 | 30.01 / 30.04 / 30.13 | 194 / 203 / 202 |
| same | `--disable-gpu` | 33.3 / 33.3 / 33.4 | 34.5 / 34.5 / 34.6 | **35.0 / 34.8 / 35.0** | 35.4 / 35.0 / 35.9 | 30.09 / 30.07 / 30.08 | 89 / 75 / 82 |

All times are in ms; "first paint" is measured from browser creation. Before the review fixes
(network not blocked) the same matrix, also three passes, gave the same picture: software
compositing / `--disable-gpu` p99 34.6-35.5 in 12/12 runs, GPU compositing p99 35.7 once and
46.2-48.0 in the other 5 runs. The builder's single-pass table (GPU p99 47.95 / 47.49,
software 34.67 / 34.67) is within these ranges.

- **Acceptance.** 300 consecutive frames at 30 fps with p99 < 40 ms: met with
  `--software-compositing` (and `--disable-gpu`) in every run. Not met reliably with the default
  GPU compositing: 2 of 12 default runs had p99 < 40 ms.
- **Snapshots.** Frames 10 and 100 are never black: non-black fraction 0.985 and 0.9835 for
  spike, 1.0 and 1.0 for Rainbow. Their hashes always differ between frame 10 and frame 100.
  spike.html hashes depend on the raster path (GPU vs software text and gradients) and on
  whether the first paint came before the first `requestAnimationFrame` (then frame 10 shows
  "Lightloom 9"); Rainbow's are identical in all modes. Pixels are exact: Rainbow's columns read
  back as pure `255/0` channel values in GPU and software mode (no colour-profile transform).
- **GPU compositing jitter is quantized to the display's vsync.** In the 6 final GPU runs,
  355 of 1776 intervals (after the first 3 of each run) were more than 3 ms off the 33.3 ms
  period; binned in units of the 144 Hz monitor's 6.94 ms frame they fall at −2: 33, −1: 120,
  0: 37, +1: 116, +2: 49. Software / `--disable-gpu` runs: 0 of 3552 intervals off by more than
  3 ms. So the GPU readback is delivered on a later display refresh (late-then-early pairs such
  as 45 then 22 ms; the average cadence stays 30.1 fps, nothing is dropped). Switches tried on
  GPU compositing (spike, p99 per run): `disable-gpu-vsync` 47.7 / 47.5 / 35.1 / 36.8 / 36.9;
  `disable-direct-composition` 47.7 / 46.8 / 36.2 / 37.1; `use-angle=gl` 36.7 / 47.9 / 48.9;
  `disable-background-networking` 47.1 / 47.4 / 48.0; builder: `disable-gpu-rasterization`
  37.4 / 47.7 / 35.5. None fixes it.
- **Recommendation.** Make the M3 renderer default to software compositing for the 320x200
  view. For heavy WebGL effects, look at `shared_texture_enabled` (`OnAcceleratedPaint`) with
  our own D3D11 readback, or external begin frames. (M3 did: `lightloom effect` and
  `CefRenderer` composite in software unless asked for `--gpu-compositing`; 20 effects in both
  modes are measured in docs/hardware/effects-m3.md.)
- **Memory** of the renderer process tree (final binary), sampled every 1.5 s during a run: 6
  processes, working set 284-301 MB (shared pages counted once per process), private bytes
  159-169 MB, browser process 99 MB. With `--software-compositing`: 271-286 MB and
  147-155 MB.
- **Builder's debug-build figure** (not re-measured): GPU compositing p99 36.7 ms, publish
  p99 1.67 ms.

## Workspace / CI notes

- **CMake and Ninja are needed for every `--features cef` rebuild.** Any cargo command that
  has to (re)run cef-dll-sys's build script needs CMake and Ninja on PATH and, without
  `CEF_PATH`, network access. The script re-runs on a new hash after dependency or profile
  changes, or when `CEF_PATH` changes. Builds without the feature never run it.
- **One CEF copy per build-script hash.** `-p`, `--workspace`, test and release builds can
  resolve features differently, so each gets its own cef-dll-sys hash and, without `CEF_PATH`,
  its own ~722 MB download + extraction under `target/<profile>/build/`. During the spike this
  piled up a dozen copies. Set `CEF_PATH` once, persistently: as a user environment variable,
  or in an uncommitted `.cargo/config.toml` in a directory *above* the checkout
  (`[env] CEF_PATH = "..."`, which cargo also hands to build scripts; the repo's own
  `.cargo/config.toml` is committed). Keep the cargo target dir outside folders synced by cloud
  clients.
- **Static CRT.** The repo's `.cargo/config.toml` builds Windows MSVC targets with
  `+crt-static`. cef-dll-sys builds libcef_dll_wrapper with `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded`
  (`/MT`) regardless, so the two match; `libcef.dll` is a separate DLL with its own static CRT.
- **CI** (`.github/workflows/ci.yml`): the `rust` jobs build the workspace without `cef`. The
  Windows `renderer-cef` job runs `cargo clippy|build -p lightloom-renderer --features cef`
  with CMake + Ninja from `tools/cef-env.sh` and `CEF_PATH` cached by CEF version, then the
  client test against the real renderer (`lightloom-render/tests/cef_real.rs`, `--ignored`,
  `LIGHTLOOM_RENDERER_EXE`). `renderer-cef-linux` (Ubuntu 24.04) does the same with the linux64
  distribution, checks the rpath, makes a 60-frame smoke run of `testdata/spike.html` and runs
  the client test, each headless first and under Xvfb as a fallback.
- **cargo-deny** runs with `all-features = true` so the CEF graph is checked too. Two
  **build-only** dependencies of `download-cef` need crate-scoped exceptions in `deny.toml`:
  `libbz2-rs-sys` (`bzip2-1.0.6`) and `webpki-roots` (`CDLA-Permissive-2.0`).
- The root pin is `cef = "=154.5.0"`: cargo ignores semver build metadata, so the former
  `=154.5.0+154.0.34` only produced a warning. The CEF build itself (154.0.34) is fixed by the
  crate version.
