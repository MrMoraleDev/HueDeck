# Audio input (`engine.audio`)

Lightloom feeds `engine.audio` to effects from a loopback capture of what the PC plays.
Code: `crates/lightloom-inputs/src/audio/` (capture and DSP; `wasapi.rs` on Windows, `pulse.rs` on Linux) and `crates/lightloom-daemon/src/audio.rs` (wiring).
Research and design notes: `docs/research/m6-inputs-network.md` §1.

## Privacy: loopback only

- Lightloom captures **only output (render) devices** in WASAPI loopback mode: the mix the PC sends to the speakers or headphones.
- On Linux it records the **monitor of an output sink**, which is the same mix. It lists only sinks, and before recording it checks that the source is that sink's own monitor (see [Capture (Linux)](#capture-linux)).
- It never enumerates or opens a microphone or any other capture endpoint. There is no setting to turn one on.
- Samples stay in memory inside `lightloomd`. Only the 200 bands and four numbers per tick go to the renderer. Nothing is stored or sent anywhere else.
- Capture runs **only while the active effect reads `engine.audio`**. The effect parser sets `uses.audio` when the HTML mentions `engine.audio`. When you switch to another effect, the stream closes. `[audio] enabled = false` turns capture off for every effect.

## Settings (`config.toml`)

```toml
[audio]
enabled = true          # capture while the active effect reads engine.audio
# device = "Speakers (Realtek(R) Audio)"   # an output device by name; default: the default output
                                           # (Linux: the sink's description or its name, e.g. alsa_output.pci-0000_0b_00.4.analog-stereo)
scale = "log"           # "log" (20 Hz-20 kHz) or "linear" (0-fs/2)
```

- **Default device.** With no `device`, the stream follows the Windows default output. If the default changes and cpal reports the stream invalidated, Lightloom reopens it after 300 ms. On Linux the stream follows the default sink the same way (below).
- **Named device.** The device name is the one Windows shows in Sound settings. On Linux it is the sink's description, as `pactl list sinks` and the desktop's sound settings show it, or its name.
- **Failures.** A name that does not match, or a device that cannot open, is logged once. Lightloom then retries every 2 s, and effects see silence meanwhile.
- **Dashboard picker.** Settings → Inputs lists the output devices (`GET /v1/inputs`, from `lightloom_inputs::audio::output_devices()`). A device chosen there is saved in the settings database, wins over `device` in `config.toml`, and moves the running capture at once; *Default output device* goes back to `config.toml`'s choice.

## Capture

- **Stream.** cpal 0.18.2 on WASAPI: an input stream built on an output device gets `AUDCLNT_STREAMFLAGS_LOOPBACK`, set at cpal `src/host/wasapi/device.rs:855` (verified in the source). It runs in shared mode in the device mix format (`GetMixFormat`), as f32, i16 or i32.
- **Callback.** The real-time callback copies whole buffers into a lock-free `rtrb` ring that holds 0.5 s. A buffer that does not fit is dropped whole, so channels never misalign.
- **Analysis thread.** `lightloom-audio` wakes at the push rate, which is the effect fps clamped to 30-60 Hz. Each tick it drains the ring, analyzes, and pushes.
- **Silence.** Loopback delivers no packets while nothing plays. After two empty ticks, the thread reads the gap as digital silence, so bars fall to 0 and `level` goes to -100.
- **Errors.** Xruns (discontinuities) are counted and logged at debug level; they do not rebuild the stream. Any other stream error rebuilds it.
- **Not handled yet.** Endpoint mute is not tracked, because cpal has no endpoint-volume API. If Windows keeps delivering audio while the device is muted, the bars keep moving.

## Capture (Linux)

`audio/pulse.rs`, cargo feature `pulseaudio` (on by default). It speaks the native PulseAudio protocol through the pure-Rust `pulseaudio` crate (MIT), so it needs no system library at build or run time. It works with:

- PipeWire through `pipewire-pulse`, which current desktops (Ubuntu 22.10+, Fedora 34+) run by default;
- PulseAudio itself;
- a Flatpak with `--socket=pulseaudio` (the manifest has it; there is no audio portal).

How it works:

1. **Connect.** The socket is `PULSE_SERVER` (`unix:` paths only), else `$PULSE_RUNTIME_PATH/native`, else `$XDG_RUNTIME_DIR/pulse/native`. Authentication sends the cookie from `PULSE_COOKIE` or `~/.config/pulse/cookie`, or 256 zero bytes when there is none, as libpulse does. Every request has a 2 s timeout, so a hung server cannot hang the analysis thread.
2. **Pick the sink.** `GetSinkInfoList`, then the sink named in `device` (by description, then by name), else the server's default sink, else the first sink. `output_devices()` (the dashboard picker) lists the same sinks by description.
3. **Never a microphone.** The source recorded is the sink's own `monitor_source_index`. Before the stream is created, `GetSourceInfo` must report that source's `monitor_of_sink_index` as this sink, or the open fails.
4. **Stream.** A record stream on the monitor: f32 little-endian at the sink's rate and channel map, 10 ms fragments, `adjust_latency`. The server converts from the sink's format. A named sink sets `no_move`, so the stream is never moved to another device.
5. **Reader thread.** `lightloom-pulse` reads packets, keeps a partial frame for the next packet so channels stay aligned, and pushes whole frames into the same 0.5 s `rtrb` ring as on Windows. A packet that does not fit is dropped whole.
6. **Following the default.** With no `device`, the connection subscribes to server events. A server change, which is how a new default sink shows up, ends the stream, and the capture thread reopens it on the new default 300 ms later. A killed stream, a closed socket or a protocol error also ends it.

A monitor delivers silence while nothing plays, so bars fall to 0 as on Windows. Checked against WSLg's PulseAudio server (the `RDP Sink` monitor, 44.1 kHz stereo, a steady stream of samples); not yet on a desktop Linux machine with PipeWire.

## Analysis

The analysis runs once per tick (`analyzer.rs`):

| Output | How |
|---|---|
| FFT | Mono mix (mean of all channels) into an N-sample history: N = 2048 up to 50 kHz (42.7 ms at 48 kHz), 4096 up to 100 kHz, 8192 above. Periodic Hann window, real FFT (`realfft`) of the newest N samples every tick. Hops of 800-1600 samples, so windows overlap 20-60 %. Amplitude `A = 2|X|/(N/2)`, in dBFS. |
| `freq` | 200 integers `0..=127`. Each band takes the loudest bin inside it, or interpolates at its centre when no bin falls inside. Log mode adds a +3 dB/octave tilt around 1 kHz. The band level is smoothed in dB (attack 10 ms, release 150 ms), then mapped linearly from -80..-12 dBFS to 0..127. It is a plain JS `Array`, replaced on each push. |
| `level` | dBFS of the mean square of the tick's samples, smoothed (attack 30 ms, release 300 ms), clamped to -100..0. A full-scale sine reads -3 dB; a -6 dBFS sine reads -9 dB. |
| `rawlevel` | RMS of the tick's samples, unsmoothed, 0..1. The meaning SignalRGB gives this value is unknown. |
| `width` | Side energy / (mid + side) energy over the history window: mono 0, uncorrelated stereo ~0.5, anti-phase 1. Smoothed 250 ms; a one-channel device reads 0. |
| `density` | Spectral flatness, 50 Hz-16 kHz, of an averaged power spectrum, divided by the flatness this analyzer measures for white noise (0.83). Pure tone ~0, white noise ~1. Gated below -80 dB; smoothed 250 ms. |

### How the `freq` scale was chosen

The scale comes from the public effects contract and public effect behaviour, never from SignalRGB code:

- **The contract.** It promises "an array of 200 elements" (SignalRGB developer docs, audio-visualizer and meter-tags pages).
- **The docs' tutorial.** It wraps the array in `new Int8Array(...)`, keeps every fourth element, warns that "sometimes an element will come in with a negative value", and applies `Math.abs` before a min/max normalization.
  - Values in **0..127** survive `Int8Array` unchanged, so `Math.abs` is a no-op and nothing wraps negative.
  - The starter Audio Bars and third-party effects divide by 127. Under these conventions, 0..127 is the widest range that is safe.
- **Spacing.** The docs say the data "massively favors some of our sound bars and often leaves others completely invisible". That fits **linear** bins up to fs/2, where music sits in the first few bands.
  - Lightloom defaults to **log** spacing, 66.7 bands per decade from 20 Hz to 20 kHz, with a pink-noise tilt, so every bar moves with music.
  - `scale = "linear"` gives the linear layout, for effects tuned to it.
  - Whether SignalRGB itself is linear is still unverified. The calibration procedure is in the research note §1.6.
- **Ceiling.** The -12 dBFS ceiling leaves headroom: loud mastered music peaks around -20 to -10 dBFS per band.

## Push path

`daemon/src/audio.rs` serializes each snapshot as `{"audio":{freq,level,rawlevel,width,density}}`:

- Floats are rounded to 3 decimals. A push is under 1.5 KB.
- The snapshot goes through `CefRenderer::engine_sink()`, the same stdin `engine` command the renderer already merges into `window.engine`.
- The sink belongs to the current renderer and follows its restarts.
- Starting another effect drops the capture, then starts a new one if the new effect reads audio.

## Latency target

**Acceptance: Audio Bars follows music with under 100 ms from input to paint.** The coordinator measures it on the user's PC.

Expected budget at 48 kHz and 60 Hz:

| Stage | Time |
|---|---|
| WASAPI period | ~10 ms |
| Wait for the tick | ≤ 16.7 ms |
| Window centre (N/2) | 21.3 ms |
| Attack smoothing | ~10 ms |
| Renderer paint | ≤ 16.7 ms |
| Canvas to devices | ~10 ms |

That is about 60-85 ms. At 30 fps, the tick and paint stages double, to about 33 ms each.

## Tests

No audio device is used in tests.

- **`analyzer.rs`:**
  - sine placement: a 1 kHz sine lands in the 1 kHz band in both scales, and 60 Hz, 250 Hz, 4 kHz and 12 kHz sines land in their log bands;
  - level: a -6 dBFS sine reads -9.03 dB, a full-scale sine -3.01 dB, and `rawlevel` equals the RMS;
  - silence;
  - attack and release smoothing;
  - width: mono, uncorrelated and anti-phase;
  - density: white noise against a tone, and the white-noise calibration constant;
  - `freq` never wrapping.
- **`audio/mod.rs`:** the capture thread with fake sources: a real-time sine, a source that fails and is reopened, a source that cannot open, and a quiet source.
- **`audio/pulse.rs`** (Linux): sink choice by description, name and default; frame alignment across packets; the socket lookup. Manual, against a running server: `cargo test -p lightloom-inputs pulse -- --ignored --nocapture` lists the sinks and records the default monitor for 1 s, printing only the sample count and RMS.
- **`daemon/src/audio.rs`:** the engine-data shape and the conditions under which capture starts.
- **`lightloom-render/tests/cef_client.rs`:** a push through `engine_sink()` from another thread reaches the (fake) renderer.
