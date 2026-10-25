# Screen input (`engine.zone`, `engine.vision`)

Lightloom feeds `engine.zone` (screen ambience) and `engine.vision` (screen meters) to effects from a capture of one display.
Code: `crates/lightloom-inputs/src/screen/` (capture, reduction, meters; `dxgi.rs` on Windows, `linux/` on Linux) and `crates/lightloom-daemon/src/screen.rs` (wiring).
Research and design notes: `docs/research/m6-inputs-network.md` §2.

## Privacy: in-process, never stored or sent

- The captured desktop image stays in memory inside `lightloomd`. Each frame is read once, reduced, and overwritten by the next one.
- Only the reduction leaves the capture thread: the 160x100 thumbnail, the 28x20 HSL grid and the meter values. They go to the local effect renderer over its stdin pipe. Nothing is written to disk, logged, or sent over the network.
- Capture runs **only while the active effect needs it**: the effect parser sets `uses.video` when the HTML mentions `engine.zone`, and the effect's `<meta meter>` tags list its meters. Switching to another effect releases the duplication. `[screen] enabled = false` turns capture off for every effect.
- Desktop Duplication cannot see the secure desktop (UAC prompts, the lock screen). Windows blacks out DRM-protected video in the copy.
- On Wayland the compositor decides: the first capture shows its screen-sharing dialog, and most desktops show a "sharing your screen" indicator while capture runs. Stopping the share there stops the capture.

## Settings (`config.toml`)

```toml
[screen]
enabled = true          # capture while the active effect reads engine.zone or declares meters
# display = "DISPLAY2"  # a display by device name; default: the primary display
fps = 30                # capture rate, 10-60
```

- **Display.** On Windows the name is the DXGI device name, `\\.\DISPLAY1`, `\\.\DISPLAY2`, … (`DISPLAY2` and `display2` also work). On Linux it is the output name, `DP-1`, `HDMI-A-1`, … (case-insensitive); on Wayland the portal dialog picks the monitor instead (see [Capture (Linux)](#capture-linux)). The debug log (`RUST_LOG=lightloom_daemon=debug`) lists the displays at capture start, and the info line `screen: capturing \\.\DISPLAY1 (2560x1440, primary) on <adapter>` names the one in use. A name that matches nothing is logged with the list of names, and Lightloom retries every 2 s.
- **Dashboard picker.** Settings → Inputs lists the displays (`GET /v1/inputs`, from `lightloom_inputs::screen::displays()`). A display chosen there is saved in the settings database, wins over `display` in `config.toml`, and moves the running capture at once; *Primary display* goes back to `config.toml`'s choice.
- **Rate.** At most `fps` reductions per second. While the desktop does not change, Windows delivers no new image, so nothing is computed. The last snapshot is still pushed once a second, so a restarted renderer gets it.

## Capture (Windows)

DXGI Desktop Duplication, written against the `windows` crate (`screen/dxgi.rs`):

1. Walk `CreateDXGIFactory1` → `EnumAdapters1` → `EnumOutputs` and pick the output attached to the desktop by name, or the one at the desktop origin (primary).
2. Create the D3D11 device **on that output's adapter**, as `DuplicateOutput` requires, so a display on a second GPU works. Neither `xcap` nor `windows-capture` does this; they use the default adapter only (research §2.1-2.2), which is why neither is used.
3. `IDXGIOutput1::DuplicateOutput`: the image is always `B8G8R8A8_UNORM`. No process-wide DPI change is made.
4. Per tick: `AcquireNextFrame(1000/fps ms)`. A timeout or a pointer-only update (`LastPresentTime == 0`) is "nothing new". A new image is copied (`CopyResource`) into a CPU-readable staging texture, the frame is released at once, the staging texture is mapped, reduced on the CPU and unmapped.
5. Any other error, such as `ACCESS_LOST` after a mode change or a full-screen switch, or `ACCESS_DENIED` on the secure desktop, drops the duplication. It is reopened after 250 ms, doubling up to 2 s. After 2 s without an image the zones go black and meters read 0.
6. Rotation (`DXGI_OUTDUPL_DESC.Rotation`) is folded into the sampling, so portrait displays come out upright.

Measured on the development PC (RTX 3060, one 2560x1440 display at 144 Hz, release build): 114 new images in 2 s, about 0.55 ms of CPU reduction per image, plus the GPU-to-CPU copy.

### Not handled yet

- **HDR.** Windows converts an HDR desktop to SDR for the legacy `DuplicateOutput`. The FP16 path (`DuplicateOutput1` with `R16G16B16A16_FLOAT`) and the meters' `hdr-h/s/l` windows are not used: meters match the SDR window, or the HDR one only when no SDR window is given.
- **Fallbacks.** No WGC or GDI fallback when duplication is unsupported (some hybrid-GPU laptops) or when four duplications already exist.
- **GPU downscale.** The whole desktop is read back (14.7 MB per image at 2560x1440). A mip-chain reduction on the GPU (research §2.4 step 6) would cut that to about 230 KB.

## Capture (Linux)

`screen/linux/`. The backend follows the session:

| Session | Backend | Cargo feature (on by default) |
|---|---|---|
| `WAYLAND_DISPLAY` set, or `XDG_SESSION_TYPE=wayland` | ScreenCast portal + PipeWire (`portal.rs`) | `screencast` |
| otherwise, `DISPLAY` set | MIT-SHM grabs (`x11.rs`) | `x11` |
| neither (for example a systemd user service that did not import the session environment) | the portal | `screencast` |

`LIGHTLOOM_SCREEN_BACKEND=portal` or `=x11` overrides the choice. Both backends hand BGRA frames to the same reduction and meters as Windows. Building `screencast` needs `libpipewire-0.3-dev` and `libclang-dev` (bindgen); `--no-default-features --features pulseaudio,x11` builds without them.

### Wayland: ScreenCast portal

The xdg-desktop-portal ScreenCast interface through `ashpd` 0.13 (MIT; its `pipewire` feature stays off, `docs/research/m7-linux-backend.md` §11), and a PipeWire stream through `pipewire` 0.10 (MIT, links `libpipewire-0.3`).

1. **Session.** `CreateSession`, then `SelectSources` for one monitor, cursor hidden, `persist_mode = 2` (until revoked) and the saved restore token. These calls get 10 s.
2. **Start.** This is where the compositor may show its dialog. It runs in the background: until the user answers, the capture reports "nothing new", so switching or stopping the effect never waits for the dialog.
3. **Restore token.** `Start` returns a token for the next session. Lightloom keeps it in `<data>/screencast-token` (mode 0600; not kept with `--ephemeral`), together with the `display` setting it was granted for. The next start passes it, and the portal restores the choice without a dialog. Tokens are single-use, so each start replaces the file. Changing `display` asks again: on Wayland the setting only selects which saved grant to use, and the dialog decides the monitor.
4. **Declined.** Cancelling the dialog forgets the token, and that effect run does not ask again; the next effect that reads the screen does. A token the portal rejects is also forgotten.
5. **Stream.** `OpenPipeWireRemote` gives a PipeWire fd. A `lightloom-pipewire` thread runs a main loop on it and connects to the granted node. The format offered is BGRx, BGRA, RGBx or RGBA, any size, without DMA-BUF modifiers, so compositors send shared-memory buffers. RGB orders are swizzled to BGRA while copying.
6. **Frames.** A frame is copied, packed, only while the capture thread waits for one, so there are at most `fps` copies a second whatever the monitor's refresh rate. Compositors send frames only when the screen changes, like Desktop Duplication.
7. **Errors.** A stream error, a disconnect or a PipeWire core error ends the source, and the capture thread reopens it after 250 ms, doubling to 2 s; with a token that is silent. Without a portal (no `xdg-desktop-portal` with a ScreenCast backend), the open fails, is logged once, and is retried every 2 s.

Not run yet on a real Wayland desktop. The code compiles and its parts are unit-tested; in WSL, which has no portal, the open fails cleanly ("The name org.freedesktop.portal.Desktop was not provided by any .service files"). Open questions for the first Linux run (WP-LINUX-RUN): whether GNOME and KDE both send SHM buffers for this offer, and rotated monitors (the PipeWire video-transform metadata is ignored; frames are taken as upright).

### X11: MIT-SHM

`x11rb` 0.14 (MIT/Apache-2.0, pure Rust, no libxcb):

1. **Displays** are the RandR 1.5 monitors (`GetMonitors`): name, size and primary flag. With no `display`, the primary monitor, else the first.
2. **Grab.** Each tick copies the monitor's area of the root window with `ShmGetImage` into a segment the X server creates and passes as an fd (MIT-SHM 1.2 `ShmCreateSegment`), mapped read-only here. This also works across IPC namespaces, as in a Flatpak. Without MIT-SHM 1.2 (a remote display) it falls back to a plain `GetImage`.
3. X11 has no "new frame" signal, so every tick is a new image: at most `fps` grabs a second.
4. Every 2 s the monitor layout is checked; a change ends the grab and it is reopened.
5. The root visual must be 24 or 32-bit TrueColor (every current desktop); others are refused with a message.

Measured in WSLg (XWayland, one 2560x1440 monitor, release build): 2.9 ms per grab plus reduction. Under XWayland on a Wayland desktop, X11 grabs see only X11 windows, which is why a Wayland session uses the portal.

### Display list on Linux

`displays()` lists the RandR monitors on X11. On Wayland, and when RandR fails, it lists the connected DRM connectors from `/sys/class/drm` (`card1-DP-1` becomes `DP-1`, the size is the preferred mode, the adapter is the GPU driver), read-only.

## Reduction

| Output | How |
|---|---|
| `zone.imagedata` | 160x100 RGBA, stretched to the display (aspect not kept). Each pixel is the mean of up to 4x4 evenly spaced source pixels of the area it covers: 256,000 reads per frame whatever the display size. Alpha is 255. |
| `zone.hue/saturation/lightness` | Each of the 28x20 cells is the area-weighted mean RGB of the thumbnail pixels it covers (5.714 x 5 px, fractional edges), converted with the CSS HSL formulas: hue `round(h) % 360` (Int16), saturation and lightness `round(·100)` (Int8, 0..100). Cell `i` is row `floor(i/28)`, column `i % 28`. |

## Meters (`engine.vision.<name>`)

Meters come from the effect's `<meta meter="name" type="…" x y width [height] h s l>` tags (parsed by `lightloom-effects`). Regions are fractions of the display, evaluated on the full-resolution image, with at most 256 samples along a region and 32 across it.

| `type` | Value |
|---|---|
| `area` | Fraction `0..1` of the region's pixels whose HSL falls in every given window. |
| `linear` | A bar along the region's long axis, cut into slices. A slice is lit when at least half its pixels match. The value is the fraction `0..1` of lit slices: a bar filled to 70 % reads 0.7, and a notch or a divider does not empty it. |
| `colormean` | `[h, s, l]` of the region's mean colour. |
| `ocr`, `ocr_numeric`, `ocr_textmatch` | Out of scope: 0. |

- **Windows.** `h` is in 0..360 and wraps when `lo > hi` (`h="340-20"` is red). `s` and `l` are in 0..100. A missing window accepts any value. Pixel HSL is rounded to whole degrees and percent before the comparison.
- **No height.** A meter without `height` is a one-pixel band at `y`.
- **`<resolution>` children.** A child whose `size="WxH"` equals the display size replaces the region. Otherwise, a child whose `aspect="W:H"` matches the display's aspect ratio does. Otherwise the base region is used.
- Values are rounded to 3 decimals and pushed with every frame.

## Push path

`daemon/src/screen.rs` serializes each snapshot as `{"zone": {hue, saturation, lightness, imagedata}, "vision": {name: value}}`:

- `zone` is sent only when the effect reads `engine.zone`, and `vision` only when it declares meters.
- `imagedata` travels as `"base64:…"`, which is 85 KB instead of about 230 KB of JSON numbers. The renderer's `engine` merge (`lightloom-renderer/src/control.rs`) decodes such a string into the existing `Uint8ClampedArray`, so effects that keep a reference to it see the new pixels. One push is under 100 KB, well within the 512 KiB control-line limit.
- The snapshot goes through `CefRenderer::engine_sink()`, the same path as `engine.audio`. The sink follows renderer restarts.

## Tests

All tests use synthetic frames; none captures the screen.

- `screen/sample.rs`: CSS HSL conversion; a grey vertical gradient gives the expected lightness per zone row; red and blue halves give hues 0 and 240; a hue ramp gives hue `12·column` within 3 degrees; rotation mapping; tiny displays.
- `screen/meters.rs`: a region of the matching colour reads 1 (area and linear); a 70 % bar reads 0.7; a notched bar stays full; vertical bars; hue wrap; colormean; one-pixel bands; `<resolution>` selection.
- `screen/mod.rs`: the capture thread with fake sources (zones and meters pushed, reopening after errors, black after 2 s, once-a-second keepalive).
- `screen/linux/` (Linux only): backend choice per session; DRM connectors from a fake sysfs; restore-token storage per display setting (mode 0600); BGRx/RGBx row copies with stride; X11 visual checks; the PipeWire frame hand-off (copied only when wanted, the end reported to the waiting side); the SPA format pod.
- `daemon/src/screen.rs`: base64, the push format, gating on `uses.video` / meters / `enabled`, conversion from parsed `<meta meter>` tags.
- Manual, real desktop: `cargo test --release -p lightloom-inputs dxgi -- --ignored --nocapture` duplicates the primary display for 2 s and prints the display list, the frame count and the reduction time. It prints nothing of the image. On Linux, `x11` grabs the primary X11 monitor the same way, and `portal` asks the compositor to share a monitor (the dialog the first time) and waits for 30 frames.
