# Screen input (`engine.zone`, `engine.vision`)

HueDeck feeds `engine.zone` (screen ambience) and `engine.vision` (screen meters) to effects from a capture of one display.
Code: `crates/huedeck-inputs/src/screen/` (capture, reduction, meters) and `crates/huedeck-daemon/src/screen.rs` (wiring).
Research and design notes: `docs/research/m6-inputs-network.md` §2.

## Privacy: in-process, never stored or sent

- The captured desktop image stays in memory inside `huedeckd`. Each frame is read once, reduced, and overwritten by the next one.
- Only the reduction leaves the capture thread: the 160x100 thumbnail, the 28x20 HSL grid and the meter values. They go to the local effect renderer over its stdin pipe. Nothing is written to disk, logged, or sent over the network.
- Capture runs **only while the active effect needs it**: the effect parser sets `uses.video` when the HTML mentions `engine.zone`, and the effect's `<meta meter>` tags list its meters. Switching to another effect releases the duplication. `[screen] enabled = false` turns capture off for every effect.
- Desktop Duplication cannot see the secure desktop (UAC prompts, the lock screen). Windows blacks out DRM-protected video in the copy.

## Settings (`config.toml`)

```toml
[screen]
enabled = true          # capture while the active effect reads engine.zone or declares meters
# display = "DISPLAY2"  # a display by device name; default: the primary display
fps = 30                # capture rate, 10-60
```

- **Display.** The name is the DXGI device name, `\\.\DISPLAY1`, `\\.\DISPLAY2`, … (`DISPLAY2` and `display2` also work). The debug log (`RUST_LOG=huedeck_daemon=debug`) lists the displays at capture start, and the info line `screen: capturing \\.\DISPLAY1 (2560x1440, primary) on <adapter>` names the one in use. A name that matches nothing is logged with the list of names, and HueDeck retries every 2 s.
- **No dashboard picker yet.** `huedeck_inputs::screen::displays()` lists the displays, for a future dashboard select.
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
- **Linux.** WP-L6 (PipeWire ScreenCast).

## Reduction

| Output | How |
|---|---|
| `zone.imagedata` | 160x100 RGBA, stretched to the display (aspect not kept). Each pixel is the mean of up to 4x4 evenly spaced source pixels of the area it covers: 256,000 reads per frame whatever the display size. Alpha is 255. |
| `zone.hue/saturation/lightness` | Each of the 28x20 cells is the area-weighted mean RGB of the thumbnail pixels it covers (5.714 x 5 px, fractional edges), converted with the CSS HSL formulas: hue `round(h) % 360` (Int16), saturation and lightness `round(·100)` (Int8, 0..100). Cell `i` is row `floor(i/28)`, column `i % 28`. |

## Meters (`engine.vision.<name>`)

Meters come from the effect's `<meta meter="name" type="…" x y width [height] h s l>` tags (parsed by `huedeck-effects`). Regions are fractions of the display, evaluated on the full-resolution image, with at most 256 samples along a region and 32 across it.

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
- `imagedata` travels as `"base64:…"`, which is 85 KB instead of about 230 KB of JSON numbers. The renderer's `engine` merge (`huedeck-renderer/src/control.rs`) decodes such a string into the existing `Uint8ClampedArray`, so effects that keep a reference to it see the new pixels. One push is under 100 KB, well within the 512 KiB control-line limit.
- The snapshot goes through `CefRenderer::engine_sink()`, the same path as `engine.audio`. The sink follows renderer restarts.

## Tests

All tests use synthetic frames; none captures the screen.

- `screen/sample.rs`: CSS HSL conversion; a grey vertical gradient gives the expected lightness per zone row; red and blue halves give hues 0 and 240; a hue ramp gives hue `12·column` within 3 degrees; rotation mapping; tiny displays.
- `screen/meters.rs`: a region of the matching colour reads 1 (area and linear); a 70 % bar reads 0.7; a notched bar stays full; vertical bars; hue wrap; colormean; one-pixel bands; `<resolution>` selection.
- `screen/mod.rs`: the capture thread with fake sources (zones and meters pushed, reopening after errors, black after 2 s, once-a-second keepalive).
- `daemon/src/screen.rs`: base64, the push format, gating on `uses.video` / meters / `enabled`, conversion from parsed `<meta meter>` tags.
- Manual, real desktop: `cargo test --release -p huedeck-inputs dxgi -- --ignored --nocapture` duplicates the primary display for 2 s and prints the display list, the frame count and the reduction time. It prints nothing of the image.
