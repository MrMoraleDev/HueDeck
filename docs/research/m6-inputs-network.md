<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Research brief for M6. Produced 2026-10-18 by a research pass and an independent fact-check pass; facts carry inline provenance tags. -->

# Lightloom M6: inputs (audio, screen) and network (mDNS, UDP, XHR, DTLS-PSK, ARP). Design brief, independently verified

Prepared 2026-10-18 and independently re-verified the same day. The verifier re-fetched every crate fact from the crates.io API and the published `.crate` sources (read, never executed), re-fetched every web source, and re-ran the read-only local queries.

**Tags**
- **[V]**: verified against a primary source. URLs are in the Sources list.
- **[V-local]**: verified from files or read-only queries on this machine.
- **[U]**: unverified (an inference or an estimate).
- **[D]**: a Lightloom design choice. It says nothing about SignalRGB.
- **(corrected)**: the verifier changed this statement.
- **(added)**: the verifier filled a gap the original brief missed.

**Verifier downloads** (untrusted data, never executed): `<research-cache>`, in the subfolders `api\`, `crates\` and `web\`. The repo was not modified.

**Repo context [V-local]**
- **Workspace:**
  - Pins `windows = 0.62`; Cargo.lock resolves 0.62.2.
  - `rust-version = "1.85"`, edition 2021.
  - `resolver = "2"`, which is not MSRV-aware (added).
  - `rust-toolchain.toml` uses channel `stable`. The local rustc is 1.99.1.
- **deny.toml:**
  - Allow list: MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, MPL-2.0, Unicode-3.0, Unicode-DFS-2016, CC0-1.0, 0BSD, OpenSSL, BSL-1.0.
  - Copyleft is denied by omission from that list.
  - `[graph] all-features = false`, with targets x86_64-pc-windows-msvc and x86_64-unknown-linux-gnu (added).
- **lightloom-hal:**
  - `AudioBackend`: start, sample_rate, channels, `read_samples` (interleaved f32), stop.
  - `CaptureBackend`: `displays()` and `capture(display, w, h) -> CaptureFrame`.
  - `CaptureFrame` is **BGRA8** (corrected). `engine.zone.imagedata` needs RGBA, so a swizzle is required.
- **lightloom-inputs:**
  - `AudioSnapshot { freq: Vec<f32> /* doc 0..=1 */, level (default -100), rawlevel, width, density }`.
  - `ZoneSnapshot { hue: Vec<i16>, saturation: Vec<i8>, lightness: Vec<i8>, imagedata: Vec<u8> /* 64000 */ }`.
- **Renderer** (`inject.rs`): injects `engine.audio.freq = new Int8Array(200)` and typed `engine.zone` arrays.
- **lightloom-effects** `parse.rs:158-160` flags (added):
  - `audio` = `engine.audio`
  - `video` = `engine.zone`
  - `meters` = meter tags or `engine.vision`
- **Approved plan pins:** `cpal 0.18 + rustfft 6`, `xcap 0.9.8`, `mdns-sd 0.21`, `reqwest 0.12 (blocking facade)`, `windows 0.62`, `triple_buffer 9`.

---
## 0. Recommendations at a glance

| Need | Pick | Version, license (all [V] crates.io) | Status vs plan |
|---|---|---|---|
| System-output audio (loopback) | `cpal` | 0.18.2 (2026-08-16), Apache-2.0, MSRV 1.85, `windows` 0.62 | as planned |
| FFT | `realfft` on `rustfft` | realfft 3.5.0 (MIT). rustfft 6.4.1 (MIT OR Apache-2.0, MSRV 1.61) | refines the plan |
| Audio callback to analyzer | `rtrb` SPSC ring | 0.4.0, MIT OR Apache-2.0, MSRV 1.38 | new |
| Screen on Windows | Own DXGI Desktop Duplication (`IDXGIOutput5::DuplicateOutput1`) plus D3D11 `GenerateMips` downscale, on `windows` 0.62. Fallback: WGC via `windows-capture`, or a GDI path like SignalRGB's "Compatibility Mode" (added) | windows-capture 2.0.1 (2026-08-08), MIT, edition 2024 | **deviates from the plan ("xcap")**. Keep xcap 0.9.8 (Apache-2.0, edition 2024) for Linux in M7; see the MSRV caveat in §5 |
| mDNS / DNS-SD | `mdns-sd`, one shared `ServiceDaemon` | 0.21.5 (2026-10-05), Apache-2.0 OR MIT, MSRV 1.71 | as planned |
| UDP sockets | `std::net` + `socket2` + `if-addrs`, plus the `windows` crate for the Winsock ioctls and `SO_EXCLUSIVEADDRUSE` | socket2 0.6.5 (MIT OR Apache-2.0, MSRV 1.70). if-addrs 0.15.0 (MIT OR BSD-3-Clause), already a dependency of mdns-sd | new |
| HTTP for `XMLHttpRequest` | `ureq` (blocking) | 3.4.2 (2026-09-13), MIT OR Apache-2.0, MSRV 1.85, edition 2024 | **deviates from the plan ("reqwest 0.12 blocking")**. reqwest is now 0.13.5 |
| DTLS 1.2 PSK client (Hue Entertainment) | `rtc-dtls` (sans-IO, ring) **if the workspace accepts Rust ≥ 1.88**. Otherwise `dtls` 0.17.2 (tokio) (added). `openssl` fallback behind a feature | rtc-dtls 0.21.1 (2026-10-11), MIT/Apache-2.0, edition 2024. Its dependency tree needs Rust ≥ 1.88 via rcgen ≥ 0.14.8 (added). dtls 0.17.2 (2026-07-20), MIT/Apache-2.0, edition 2021 | new |
| ARP / MAC for `Discovered.id` | Windows: `GetIpNetTable2`, then `SendARP` (`windows` 0.62 IpHelper). Linux: `/proc/net/arp` | no new crate | new |

---
## 1. Audio (`engine.audio`)

### 1.1 Contract facts

**SignalRGB docs: audio-visualizer page [V docs.signalrgb.com/developer/lightscripts/audio-visualizer]**
- "engine.audio.level - returns a number between -100 and 0 representing the overall loudness of the track." "0 is loud, -100 is very low." Also "This property simply returns the loudness of the track in decibels".
- "engine.audio.density - returns a number between 0 and 1 representing the roughness of the tone, with test tones returning 0 and white noise 1."
- "engine.audio.freq - returns an array of 200 elements containing the track's frequency data."
- The tutorial code:
  - does `var frequency = new Int8Array(engine.audio.freq)`;
  - keeps every fourth element (`index % 4 === 0`);
  - warns "Sometimes an element will come in with a negative value";
  - uses `Math.abs`, then normalizes with `(x - min) / (max - min)`.
- The page says the data "massively favors some of our sound bars and often leaves others completely invisible".

**SignalRGB docs: meter-tags page [V docs.signalrgb.com/developer/lightscripts/meter-tags]**
- "engine.audio.level returns the current audio level in decibels." (added)
- "engine.audio.width gives the perceived stereo width of the incoming audio, as a ratio from 0 to 1."
- density: "white noise produces values near 1.0, while pure tones return values closer to 0".
- "engine.audio.freq returns an array with 200 elements, each representing the current level of a specific frequency band."
- `engine.zone.hue/saturation/lightness`: 560 elements "sampled from a 28 x 20 grid". Hue range 0 to 360; saturation and lightness 0% to 100%.
- Neither page mentions `rawlevel`.

**SignalRGB binary and local files**
- [V-local `docs/host-api/effects-engine.md:60`] The bootstrap is `engine.audio.freq = [...%1]`, so it is a **plain Array**. `rawlevel` exists, scale UNKNOWN.
- [V-local] String dump: `<research-cache>`.
  - `AudioManager` (~lines 112773-112790):
    - signals `SampleRateChanged, LatencyChanged, DeviceNameChanged, DeviceTypeChanged, RmsLevelChanged, CaptureChanged, InitializedChanged, DisabledChanged`;
    - props `sampleRate, latency, deviceName, deviceType, dbLevel, capturing`.
  - `AudioCore` (149220-149240):
    - `OnCaptureFailure(ahResultCode, asContext)`, `OnSampleRateChanged(aiNewSampleRate)`, `OnInterfacesChanged`, `AnalysisFinished`, `FFTStream`, `ProcessSamples(QList<float> avpData, aiFrameSize, aiChannelCount, aiSampleRate)`;
    - implements `IMMNotificationClient` and `IAudioEndpointVolumeCallback` (RTTI `.?AUIAudioEndpointVolumeCallback@@`).
  - (added) `IntelligentAnalyzer` (149201-149209): `AnalysisComplete`, `FftDataReady(float* pvData, aiSize, anPeak)`. The internal FFT output is **float** [V-local]. What `anPeak` means is [U].
  - (corrected) "This size is not a default fixed size of QRealFourier. Using a variable size instead." and "Invalid FFT size." are **SignalRGB's own log strings**. They do not appear in the QRealFourier sources.
    - They react to `QFourierTransformer::setSize()` returning FixedSize, VariableSize or InvalidSize [V QRealFourier source].
    - So SignalRGB's FFT size is configurable or derived. It is still a power of two, because `isValidSize` requires that. The actual size is [U].
  - (corrected) Settings strings (122598-122621 and 124133-124142):
    - **Audio:** "Audio Device", "Audio Latency", "Sample Rate", "Device Type", "Capture Enabled", "Threading Priority", and "...(by default we capture 'what you hear')...".
    - **"Compatibility Mode (Slower)" is a video option**: "When enabled, uses a slower capture mechanism supported by older operating systems. (W7)" and "Use GDI for capture (slower), instead of directx. This may be required for older Windows 7 machines, or on some laptops."
    - **"Capture Framerate"** has two descriptions:
      - "This setting adjusts the CPU intensity and framerate limit of background capture and analysis. (Only valid during video and audio playback)";
      - "This setting adjusts the maximum framerate of background color capture. This setting only applies to lightscripts that make use of screen ambience."
      - Whether it governs the audio analysis rate is [U].
  - Conclusion: SignalRGB captures the render-endpoint mix by default ("what you hear"). That is consistent with WASAPI loopback, but the mechanism is [U]. The level is RMS-derived dB ("RmsLevelChanged", "dbLevel"); exact constants [U].
- **QRealFourier** [V github.com/visore/QRealFourier]:
  - LGPLv3 (License.txt). Do not use it.
  - Built on FFTReal.
  - Fixed calculators 2^3..2^14 plus a variable calculator. Sizes must be powers of two.
  - Window list "Rectangular", "Hamming", "Hann". `createFunction` returns NULL for Rectangular, so Rectangular means no window.
  - SignalRGB's RTTI contains `QFourierFixedCalculator<3..14>`, `FFTReal<float>`, `QHammingFunction<float>`, `QHannFunction<float>` [V-local].
  - Which window, size and bin mapping SignalRGB uses: [U].

**Still unknown**
- [U] Scale and spacing of `freq`. Two hypotheses:
  - **H1:** unsigned 0..255 values. These wrap in `new Int8Array` (Node: `new Int8Array([200,255,128])` gives `[-56,-1,-128]`) [V JS semantics]. That fits "sometimes negative".
  - **H2:** signed dB-like floats. The internal `float*` FFT signal fits either hypothesis.
- **Linear vs log hints:**
  - hyperb1iss/lightscript-workshop (MIT) assumes 200 linear bins up to 22.05 kHz (`FREQ_PER_BIN = (44100/2)/200`) and normalizes `Math.abs(v)/128` [V that it assumes this; U that it is right]. Its dev simulator emits 0..127 and a constant width 0.5 [V].
  - The catalog has a community "Logarithmic Visualizer" (id `-NZhF99k6cK__Zyh4yNx`, type Audio, "Reflects the audio visualizers of digital audio workstations (DAWs).") [V-local catalog_en.json].

### 1.2 Capture with cpal 0.18.2 [V, cpal-0.18.2 source]

**How loopback works**
- `src/host/wasapi/mod.rs`: "If you use a WASAPI output device as an input device it will transparently enable loopback mode".
- `device.rs`:
  - `DEFAULT_FLAGS = AUDCLNT_STREAMFLAGS_EVENTCALLBACK`;
  - `if self.data_flow() == eRender { stream_flags |= AUDCLNT_STREAMFLAGS_LOOPBACK }`;
  - `AUDCLNT_SHAREMODE_SHARED`.
- Capture streams do **not** set AUTOCONVERTPCM ("Capture streams do not; only native formats will work"). Use the mix format.
- `default_input_config()` on a render device returns `ErrorKind::UnsupportedOperation` ("Device does not support input"). Use `default_output_config()`.
- (added) On WASAPI, `default_output_config()` is `default_format()`, which is `GetMixFormat`. The generic 0.18 UPGRADING note "prefer 48 kHz" does not change the WASAPI result.

**API in 0.18**
- `build_input_stream<T,D,E>(&self, config: StreamConfig, data: D, err: E, timeout: Option<Duration>) -> Result<Stream, cpal::Error>`, with `D: FnMut(&[T], &InputCallbackInfo) + Send + 'static` and `E: FnMut(Error) + Send + 'static`.
- `build_input_stream_raw(config, SampleFormat, FnMut(&Data,&InputCallbackInfo), FnMut(Error), Option<Duration>)`.
- `StreamConfig { channels: u16, sample_rate: u32, buffer_size }` is `Copy` and passed by value.
- A single `cpal::Error { kind, message }` with `kind()`. `ErrorKind` is `#[non_exhaustive]` and `Copy` (added).
- Streams are returned paused on every backend (UPGRADING "7."), so call `play()`.
- `SampleRate = u32`.
- WASAPI `Stream` has been `Send + Sync` since 0.17 (CHANGELOG 0.17.0).

**(added) Xruns.** cpal 0.18.2 reports "WASAPI: Capture xruns ... as `ErrorKind::Xrun`", raised on `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY`. The error handler must count these and must **not** rebuild the stream on them.

**Default-device switching**
- The default output is activated with `ActivateAudioInterfaceAsync(DEVINTERFACE_AUDIO_RENDER)`.
- `DefaultDeviceMonitor` (`IMMNotificationClient`, eConsole) signals changes.
- On a change, the error callback gets `StreamInvalidated`, or `DeviceNotAvailable` when no default exists.
- (corrected) cpal's own comments conflict:
  - `device.rs` says the virtual default device is used "so that the Windows audio engine automatically reroutes the stream";
  - `stream.rs` says "WASAPI never rebinds the IAudioClient".
  - After emitting the error, the capture loop continues (`ControlFlow::Continue`).
  - Whether audio keeps flowing after a default change is [U]. Rebuilding the stream on these errors, debounced about 300 ms, is the safe policy [D].

**WASAPI facts [V learn.microsoft.com loopback-recording]**
- Event-driven loopback clients are supported on Windows 10 1703 and later.
- "A client can enable loopback mode only for a shared-mode stream."
- (added) "WASAPI loopback by default contains the mix of all audio being played, regardless of the Terminal Services session". A session-0 service would capture all sessions; lightloomd runs as the user, per architecture-synthesis.
- (corrected) The DRM sentence ("a trusted audio driver does not permit a loopback device to capture digital streams that contain protected content") is about loopback devices and trusted drivers. Silence for DRM content through WASAPI loopback mode is [U].
- That other applications' exclusive-mode streams are missing from loopback is also [U], inferred because loopback copies the audio-engine output.

**No packets while idle [V GStreamer commit a668de74, part of MR 1588]**
- The commit says: "capture client will not produce any data if there's no outputting sound to corresponding render client. In other words, if there's no sound to render, capture task will stall."
- GStreamer chose to keep pushing silence through a dummy render client ("silence feed client", `fill_loopback_silence` in gstwasapi2rbuf.cpp).
- Lightloom options [D]: a zero-fill watchdog, or a silent render stream on the same endpoint.

**Mute**
- GStreamer wasapi2src has the property `loopback-silence-on-device-mute`: "When loopback recording, if the device is muted, inject silence in the pipeline" (since 1.24). It tracks endpoint mute through an `IAudioEndpointVolume` callback [V docs and source].
- That loopback keeps delivering audio while muted is implied, not stated [U]. Follow mute through `IAudioEndpointVolume` (feature `Win32_Media_Audio_Endpoints` [V windows-docs-rs]) and feed silence [D]. SignalRGB implements `IAudioEndpointVolumeCallback` [V-local].

```rust
// sketch, lightloom-hal-windows/src/audio.rs (not compiled)
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Data, ErrorKind, InputCallbackInfo, SampleFormat};
pub fn open_loopback(mut tx: rtrb::Producer<f32>, errs: crossbeam_channel::Sender<cpal::Error>)
    -> Result<(cpal::Stream, u32, u16), cpal::Error> {
    let host = cpal::default_host();                                   // WASAPI on Windows
    let dev = host.default_output_device().ok_or_else(|| cpal::Error::from(ErrorKind::DeviceNotAvailable))?;
    let sc = dev.default_output_config()?;                             // = GetMixFormat on WASAPI
    let (cfg, fmt) = (sc.config(), sc.sample_format());
    let stream = dev.build_input_stream_raw(cfg, fmt,
        move |d: &Data, _i: &InputCallbackInfo| {                     // RT thread: no alloc/lock; drop on overflow
            match d.sample_format() {
                SampleFormat::F32 => { let _ = tx.push_partial_slice(d.as_slice::<f32>().unwrap()); }
                SampleFormat::I16 => for &s in d.as_slice::<i16>().unwrap() { let _ = tx.push(s as f32 / 32768.0); },
                SampleFormat::I32 => for &s in d.as_slice::<i32>().unwrap() { let _ = tx.push(s as f32 / 2147483648.0); },
                _ => {}
            }
        },
        move |e: cpal::Error| { let _ = errs.try_send(e); },          // consumer: Xrun => count; StreamInvalidated/DeviceNotAvailable => rebuild
        Some(std::time::Duration::from_secs(2)))?;
    stream.play()?;                                                    // 0.18: streams start paused
    Ok((stream, cfg.sample_rate, cfg.channels))
}
```

`rtrb::Producer::push_partial_slice` (for `T: Copy`, `#[must_use]`) and `write_chunk_uninit` exist in rtrb 0.4.0 [V source]. Ring reads and writes are "lock-free and wait-free" [V crate docs].

**Linux in M7 [V cpal-0.18.2 `host/pipewire/device.rs`, `platform/mod.rs`, README]**
- `Audio/Sink` nodes are exposed as Duplex. An input stream on one sets `STREAM_CAPTURE_SINK` ("stream.capture.sink"), which captures what plays on that sink.
- With the `pipewire` feature, `default_host()` prefers PipeWire when it is available.
- Requirements:
  - `libpipewire-0.3-dev` and PipeWire ≥ 0.3.53;
  - (added) the ALSA headers (`libasound2-dev`) are needed even when PipeWire is used.

### 1.3 Alternative: wasapi 0.26.0 [V source]

MIT, 2026-10-11, MSRV 1.85, edition 2024, `windows` 0.62.2.

```rust
let dev = DeviceEnumerator::new()?.get_default_device(&Direction::Render)?;
let mut ac = dev.get_iaudioclient()?;
ac.initialize_client(&fmt, &Direction::Capture,
    &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns })?; // Render dev + Capture dir + Shared => AUDCLNT_STREAMFLAGS_LOOPBACK
                                                                            // autoconvert => AUTOCONVERTPCM | SRC_DEFAULT_QUALITY
```

- Then call `set_get_eventhandle()`, `get_audiocaptureclient()`, `start_stream()`, then `wait_for_event(ms)` on the handle and `read_from_device_to_deque(&mut VecDeque<u8>)` on the capture client.
- Exclusive mode returns `WasapiError::LoopbackWithExclusiveMode`.
- Per-application capture: `AudioClient::new_application_loopback_client(pid, include_tree)` (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`). (added) Process loopback requires Windows 10 build 20348 or later [V GStreamer wasapi2src docs]; this PC is on 26200.
- Use it only if you need per-app capture or 48 kHz fixed by autoconvert. It is Windows-only.

### 1.4 DSP spec [D unless noted]

The parameters below are knobs in `config.toml`.

| Item | Spec |
|---|---|
| Input | Interleaved f32 at `fs`, with `C` channels as delivered. Mono `m = mean_c(x_c)`. `L = x0`, `R = x1` (`R = L` if C=1). |
| Tick | 60 Hz analysis on a dedicated thread. Hop `H = round(fs/60)`. (corrected) SignalRGB's "Capture Framerate" is documented for screen ambience; its relation to the audio analysis rate is [U]. |
| FFT | `N = 2048` (fs ≤ 50 kHz), `4096` (≤ 100 kHz), `8192` otherwise. That is about 42.7 ms at 48 kHz. Periodic Hann `w[n]=0.5-0.5cos(2πn/N)`, `S_w = N/2`. `realfft` R2C gives N/2+1 bins and "does not normalize" [V README]. Use `process_with_scratch` to avoid allocation [V trait docs]. |
| Amplitude | `A[k] = 2|X[k]|/S_w`; a full-scale on-bin sine gives 1.0 [V math]. `dB[k] = 20·log10(A[k]+1e-9)`. Skip DC. |
| Bands, log mode | Edges `f_i = 20·1000^(i/200)`, i = 0..200 (ratio 1.03514, 66.7 bands/decade [V math]), capped at `0.95·fs/2`. Centre `fc_i = 20·1000^((i+0.5)/200)`. If the band holds at least one bin centre, `B_i = max dB[k]`. Otherwise interpolate dB linearly at `fc_i` (Δf = 23.4375 Hz at N=2048, 48 kHz). |
| Bands, linear-compat mode | `f_i = i·(fs/2)/200`, same max rule. Switch to it if calibration (§1.6) shows SignalRGB is linear. |
| Tilt | Log mode: `B_i += 3 dB·log2(fc_i/1000)`. Compat mode: 0. |
| Smoothing | In dB: `y += α(B−y)`, `α = 1−exp(−Δt/τ)`. Attack τ 10 ms (α = 0.811 at 60 Hz). Release τ 150 ms (α = 0.105) [V math]. |
| `freq` output | `v_i = clamp(round(127·(y_i+80)/68), 0, 127)` (D_min −80, D_max −12 dBFS). 0..127 never wraps in `Int8Array`, so the docs' `Math.abs` is a no-op. Optional `freq_max = 255` knob for H1. |
| `level` | `ms` = mean over channels and the last H frames of x², smoothed in linear power (attack 30 ms, release 300 ms). `level = clamp(10·log10(ms_s+1e-12), −100, 0)`. A full-scale sine gives −3.01 dBFS [V math]. |
| `rawlevel` | `sqrt(ms)` of the last hop, unsmoothed, 0..1 [U semantics]. |
| `width` | Over the last N frames: `E_M=Σ((L+R)/2)²`, `E_S=Σ((L−R)/2)²`, `width = E_S/(E_M+E_S)`. Mono 0, L-only 0.5, uncorrelated ≈0.5, anti-phase 1 [V math]. Silence gate; smoothing τ 250 ms. The 0..1 range is [V]; the formula is [D]. |
| `density` | Spectral flatness of the EMA power `P̄[k] += 0.25·(A[k]²−P̄[k])` over 50 Hz to min(16 kHz, 0.9·fs/2): `SF = exp(mean ln(P̄+ε))/(mean P̄+ε)`, then `density = clamp(SF/SF_white, 0, 1)`. Theory: a single periodogram of white noise gives e^−γ = 0.5615. α=0.25 equals about K=7 averages, giving exp(ψ(7) − ln 7) = 0.9295 [V math]. Overlap and Hann correlation lower it [U exact]. Calibrate `SF_white` in a unit test. Gate below −80 dB; smoothing τ 250 ms. |
| Silence and mute | No samples for more than 2 hops, or endpoint muted: feed zeros (freq → 0, level → −100). Or keep a silent render stream alive (GStreamer approach) [D]. |

**Unit tests**
- 1 kHz sine at −6 dBFS: argmax centre within ±3.6%, and `level` ≈ −9.03 dB.
- White noise: `density` ≥ 0.85.
- Identical L/R: `width` ≈ 0. Independent L/R noise: ≈ 0.5.
- Digital silence: `level` = −100, `freq` all 0.

### 1.5 Analyzer sketch (APIs [V realfft 3.5.0 source])

```rust
use realfft::{RealFftPlanner, RealToComplex, num_complex::Complex32}; // realfft re-exports rustfft::num_complex
pub struct Analyzer { n: usize, fs: f32, r2c: std::sync::Arc<dyn RealToComplex<f32>>, win: Vec<f32>,
    inp: Vec<f32>, spec: Vec<Complex32>, scratch: Vec<Complex32>, bands: Vec<BandMap>, y_db: [f32; 200], /* … */ }
impl Analyzer {
    pub fn new(fs: u32) -> Self {
        let n = if fs <= 50_000 { 2048 } else if fs <= 100_000 { 4096 } else { 8192 };
        let r2c = RealFftPlanner::<f32>::new().plan_fft_forward(n);
        let win = (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()).collect();
        let (inp, spec, scratch) = (r2c.make_input_vec(), r2c.make_output_vec(), r2c.make_scratch_vec());
        todo!() /* precompute BandMap for 200 bands */
    }
    pub fn tick(&mut self, mono_last_n: &[f32], l: &[f32], r: &[f32], hop: &[f32]) -> AudioSnapshot {
        for (o, (x, w)) in self.inp.iter_mut().zip(mono_last_n.iter().zip(&self.win)) { *o = x * w; }
        self.r2c.process_with_scratch(&mut self.inp, &mut self.spec, &mut self.scratch).unwrap(); // input is clobbered
        todo!()
    }
}
```

**Threading [D].** The cpal callback pushes into `rtrb`. The analyzer thread drains it at 60 Hz and keeps an N-sample history. It publishes `AudioSnapshot` to the engine-data shm (seqlock), and the renderer's V8 extension reads it on rAF.

### 1.6 Latency and calibration

**Budget [U estimate]:** about 60-85 ms in total.
- WASAPI period about 10 ms
- Hop ≤ 16.7 ms
- Window centre N/2 = 21.3 ms at 48 kHz
- Attack about 10 ms
- shm to V8 ≤ 16.7 ms
- Canvas-to-wire p50 ≤ 10 ms, p99 ≤ 25 ms: the M3 acceptance numbers [V-local plan]

Measure it with `lightloom bench audio-latency` [D].

**Calibration against SignalRGB** [D; needs user action, not done because it writes to Documents]:
1. Install a dev lightscript that prints argmax, min, max, level, rawlevel, width and density.
2. Play a sine sweep 50 Hz-16 kHz at −6 dBFS, white and pink noise, silence, L-only and anti-phase.
3. Decide log or linear spacing, the range, 127 vs 255, the rawlevel fit, and the width/density behaviour.

### 1.7 Repo deltas [D]

- `AudioSnapshot.freq`: replace f32 0..=1 with 0..127 integers.
- shm: `architecture.md:84` says "audio 203 f32"; it needs 200 + 4 (level, rawlevel, width, density) [V-local].
- Expose `engine.audio.freq` as a **plain Array**, as SignalRGB's `[...%1]` does. (corrected) With the current `Int8Array`, `freq.map(v => v/127)` returns an Int8Array of 0s (only 127 maps to 1), because typed-array `map` coerces results to Int8 [V Node].
- `engine.zone.*` stay typed arrays updated with `.set()` [V-local effects-engine.md:62-66].

---
## 2. Screen capture (`engine.zone`: 28x20 HSL + 160x100 RGBA, 10-60 fps)

### 2.0 What SignalRGB does (added) [V-local strings]

- **DXGI Desktop Duplication.** Strings: "AcquireNextFrame failed with hr: ", "Unhandled DXGI Error: %1", "RELEASING FRAME", "Rotate90/Rotate180/Rotate270".
- **Recovery:** "Display adapter removed or changed - reinitializing. %1", "Permissions Error for Screen Capture - reinitializing. %1", "Bad capture dimension ... Attempting reinit", "System going to sleep, stopping capture."
- **Adapter mismatch:** "Screen-reactive effects can't use fast capture on this display. Switch to 'Compatibility Mode' in Video settings, or make sure SignalRGB is running on the same graphics adapter as your display."
- **GDI fallback:** "Use GDI for capture (slower), instead of directx" and signal `ForceGdiChanged`.
- **HDR:** an HDR colourspace flag (`bUseHdrColorspace`, `HdrChanged`) and separate meter HDR ranges ("Invalid HDR hue/sat/lightness range", `HdrHColor/HdrLColor`).
- No `Windows.Graphics.Capture` or `Direct3D11CaptureFramePool` strings were found. So SignalRGB is DDA-primary with a GDI fallback; that WGC is absent is [U].

### 2.1 xcap 0.9.8 [V xcap-0.9.8 source]

Apache-2.0, 2026-08-01, edition 2024.

**Windows backends**
- `Monitor::capture_image()` and `capture_region(x,y,w,h)` use GDI `BitBlt` + `GetDIBits` at full resolution.
- With feature `wgc`, each call creates a one-shot WGC session: `CreateFreeThreaded` frame pool (B8G8R8A8), new session, `SetIsBorderRequired(false)`, `SetIsCursorCaptureEnabled(false)`, `recv_timeout(3000 ms)`.
- `video_recorder()` uses legacy `IDXGIOutput1::DuplicateOutput` by default, or WGC with `wgc`.

**DXGI recorder per frame**
- A new staging `CreateTexture2D`.
- A full-resolution `CopySubresourceRegion`, `Map`, a new `Vec` and a BGRA→RGBA swizzle.
- On any error other than timeout the thread `break`s with "DXGI_ERROR_UNSUPPORTED", so there is no ACCESS_LOST recovery.
- It calls `DuplicateOutput` on every output while searching.
- (added) It creates its device with `D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, …)`, which is the **default adapter only**. Monitors on other adapters are never found.

**Other limits**
- No downscale option.
- No HDR: always 8-bit BGRA.
- Cost [U estimate]: about 3-10 ms CPU and 14.7 MB copied per frame at 2560x1440 (2560×1440×4 = 14,745,600 bytes).

**Linux (M7) [V source]**
- Uses `pipewire` 0.10 (MIT), `zbus` "5.17" (MIT), `libwayshot-xcap` 0.3.3 (BSD-2-Clause) and `xcb` 1.7 (MIT).
- (added) On Wayland, `capture_image` uses the portal Screenshot API (one-shot). Continuous capture needs `video_recorder()` (portal ScreenCast + PipeWire).
- (added) zbus ≥ 5.14 declares MSRV 1.87 [V crates.io], above the workspace's 1.85.

Verdict: do not use xcap on Windows.

### 2.2 windows-capture 2.0.1 [V source]

MIT, NiiightmareXD, edition 2024.

**WGC path**
- `GraphicsCaptureApiHandler { new(ctx), on_frame_arrived(&mut self, &mut Frame, InternalCaptureControl), on_closed() }`.
- `Settings::new(item, CursorCaptureSettings, DrawBorderSettings, SecondaryWindowSettings, MinimumUpdateIntervalSettings::{Default|Custom(Duration)}, DirtyRegionSettings, ColorFormat::{Rgba16F|Rgba8|Bgra8}, flags)`.
- `start_free_threaded(settings) -> Result<CaptureControl, _>`.
- `Frame` exposes `as_raw_texture()`, `device()` and `device_context()`, so GPU downscale is possible.

**DXGI path (`DxgiDuplicationApi`)**
- `DuplicateOutput1` with `[R16G16B16A16_FLOAT, R8G8B8A8_UNORM, B8G8R8A8_UNORM]`.
- `acquire_next_frame(ms)` returns a frame with `texture()`, `device()`, `device_context()` and `frame_info()`.
- `recreate()` after `Error::AccessLost`.
- It calls `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` (process-wide; `E_ACCESSDENIED` is ignored).
- (added) It also uses the **default adapter only** (`D3D11CreateDevice(None, HARDWARE…)`, then `find_output_for_monitor` on that adapter). A monitor on another adapter gives `Error::OutputNotFound`.

**Dependencies:** windows 0.62.2 (including Media features for its encoder), rayon 1.12, parking_lot 0.12.5, thiserror 2, windows-future 0.3.2.

**WGC platform facts [V MS]**
- `GraphicsCaptureSession.MinUpdateInterval` is documented only for WinRT 26100 and later. This PC is on 26200.
- `IsBorderRequired` arrived in 10.0.20348 (UniversalApiContract v12). Disabling the border needs `GraphicsCaptureAccess.RequestAccessAsync(GraphicsCaptureAccessKind.Borderless)` plus the `graphicsCaptureWithoutBorder` package capability. Unpackaged-app behaviour [U].

### 2.3 Desktop Duplication facts [V Microsoft docs]

**Formats**
- Legacy `DuplicateOutput`: "The format of the desktop image is always DXGI_FORMAT_B8G8R8A8_UNORM".
- `DuplicateOutput1(pDevice, Flags, SupportedFormatsCount, pSupportedFormats)` can deliver the original full-screen buffer format. Otherwise DXGI converts to one of the listed formats.
- The list "should always contain DXGI_FORMAT_B8G8R8A8_UNORM".
- The device "must be created from the adapter to which the output is connected".
- windows-rs 0.62 signature: `DuplicateOutput1(&self, pdevice, flags: u32, psupportedformats: &[DXGI_FORMAT]) -> Result<IDXGIOutputDuplication>` [V windows-docs-rs].

**Errors**
- `E_INVALIDARG`: wrong adapter, or (added) "The calling application is already duplicating this desktop output".
- `E_ACCESSDENIED`: no access to the current desktop. "only an application that runs at LOCAL_SYSTEM can access the secure desktop".
- `DXGI_ERROR_UNSUPPORTED`: for example 8bpp or non-DWM modes. Retry after `EVENT_SYSTEM_DESKTOPSWITCH` or `WM_DISPLAYCHANGE`.
- `DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`: the concurrent-duplication limit was reached ("default of four"). SignalRGB itself also duplicates.
- `DXGI_ERROR_SESSION_DISCONNECTED`.

**Rotation:** the surface "is always in the un-rotated orientation". Read `DXGI_OUTDUPL_DESC.Rotation`.

**Pointer** (corrected): it is **either** already drawn into the desktop image **or** reported separately. `DXGI_OUTDUPL_FRAME_INFO.PointerPosition.Visible` says which. For ambience, ignore the separate pointer.

**Hybrid GPUs:** MS KB 3019314 says DDA against the discrete GPU of a "Microsoft Hybrid system" fails by design with `DXGI_ERROR_UNSUPPORTED`. The article "Applies to: Windows 8.1"; on Win10/11 it is [U], but SignalRGB's adapter-mismatch message suggests it persists. Fall back to WGC or GDI.

**HDR**
- Without FP16 in the list, "captured texture ... is converted frame by OS unless we use IDXGIOutput5::DuplicateOutput1() with DXGI_FORMAT_R16G16B16A16_FLOAT ... for captured frame to be scRGB" [V GStreamer commit 0afeacce, part of MR 5671].
- scRGB (1.0,1.0,1.0) "encodes the standard D65 white at 80 nits" [V MS Advanced Color doc].
- (corrected) The MS Q&A thread does **not** show FP16 being reported when only BGRA was requested; the asker had requested FP16. That claim is withdrawn [U]. Branching on the actual `GetDesc().ModeDesc.Format` remains a design rule [D].
- SDR white level: `DISPLAYCONFIG_SDR_WHITE_LEVEL.SDRWhiteLevel / 1000 * 80` nits. Query it with `DisplayConfigGetDeviceInfo` and `QueryDisplayConfig`.
- (added) HDR detection: `IDXGIOutput6::GetDesc1().ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020`. SDR Advanced-Color displays also report `G22_NONE_P709`.
- (added) Win32 apps should poll `IDXGIFactory1::IsCurrent()` to detect capability changes.

**GenerateMips**
- Without `D3D11_BIND_RENDER_TARGET`, `D3D11_BIND_SHADER_RESOURCE` and `D3D11_RESOURCE_MISC_GENERATE_MIPS`, "the call to GenerateMips has no effect".
- Mip generation is supported for B8G8R8A8_UNORM and _SRGB at FL 9.1+, and for R16G16B16A16_FLOAT at FL 9.2+ [V GenerateMips doc].

**DPI:** that `DuplicateOutput1` needs PerMonitorV2 is a single Stack Overflow answer (SO 48207677, via a mirror) [U]. windows-capture codifies it. Declare PerMonitorV2 in the `lightloomd.exe` manifest [D].

### 2.4 Recommendation and pipeline [D]

**Primary backend:** hand-written DDA in `lightloom-hal-windows`, about 400-600 LOC [U estimate], on `windows` 0.62.

**Fallbacks**, behind feature `capture-wgc` or a GDI mode, used on `UNSUPPORTED`, `NOT_CURRENTLY_AVAILABLE`, or when the user asks:
- `windows-capture` WGC;
- (added) a GDI fallback like SignalRGB's "Compatibility Mode": `StretchBlt` straight to 160x100 with `HALFTONE`. GDI on HDR desktops [U].

`windows-capture`'s `DxgiDuplicationApi` is an acceptable stand-in only for single-adapter systems.

**Reasons**
- No new dependencies.
- No process-wide side effects.
- FP16/HDR support.
- Explicit recovery.
- Adapter selection: both crates use the default adapter only [V].
- GPU-side reduction, so only 230,400 B (BGRA8) or 460,800 B (FP16) are read back at 320x180 [V math].

**Pipeline**
1. **Pick the output.** Walk `CreateDXGIFactory1` → `EnumAdapters1(i)` → `EnumOutputs(j)` → `GetDesc()` (`DeviceName`, `Monitor`, `DesktopCoordinates`, `Rotation`). Choose by setting; the default is the primary.
   - This PC [V-local]: one output `\\.\DISPLAY1`, 2560x1440 at 144 Hz, on an NVIDIA RTX 3060 (PCI\VEN_10DE&DEV_2504&SUBSYS_39031462).
   - The monitor is a Samsung LC27G5xT (WmiMonitorID). Its HDR state is [U]: MonitorDataStore has no AdvancedColorEnabled/HDREnabled values.
   - A "Parsec Virtual Display Adapter" (ROOT\DISPLAY\0000) is installed with no active mode. That it is IddCx, and DDA behaviour on it, are [U].
2. **Create the device.** `D3D11CreateDevice(Some(&adapter), D3D_DRIVER_TYPE_UNKNOWN, …, D3D11_CREATE_DEVICE_BGRA_SUPPORT, …)`.
3. **Detect HDR** with `GetDesc1().ColorSpace`.
4. **Duplicate.** `DuplicateOutput1(&dev, 0, &[R16G16B16A16_FLOAT, B8G8R8A8_UNORM])`, then read the real format from `dupl.GetDesc().ModeDesc.Format`. Use exactly one duplication per output per process.
5. **Loop at F fps** (10..60, default 30): `AcquireNextFrame(1000/F, &mut info, &mut res)`.
   - `WAIT_TIMEOUT`: keep the last snapshot.
   - `ACCESS_LOST`: recreate, with backoff from 250 ms to 2 s.
   - `E_ACCESSDENIED`: back off; zones fade to black after 2 s.
   - `UNSUPPORTED` or `NOT_CURRENTLY_AVAILABLE`: switch to the fallback.
6. **When `info.LastPresentTime != 0`:**
   - `CopySubresourceRegion(mip_tex, 0, 0,0,0, desktop_tex, 0, None)`. `mip_tex` uses the same format, the full mip chain, `RENDER_TARGET|SHADER_RESOURCE` and `GENERATE_MIPS`.
   - `GenerateMips(&srv)`.
   - Choose the largest k with `(W>>k) ≥ 160` and `(H>>k) ≥ 100`. Here k = 3 gives 320x180 [V math].
   - Copy level k to `staging_k`.
   - `ReleaseFrame()` right away.
   - `Map(staging_k, 0, D3D11_MAP_READ)`.
   - Optional: a `_SRGB` SRV over a typeless texture, for linear-light averaging.
7. **CPU work**
   - **FP16:** `c' = c·80/SDRwhite_nits`. Clamp negatives (out of the BT.709 gamut). Soft-knee or clip above 1.0. Apply the sRGB OETF, then convert to u8.
   - If the white level is unknown, assume 200 nits. (corrected) MS: "On desktop HDR monitors, SDR reference white levels are typically set to around 200 nits" [V].
   - Fix rotation.
   - Area-resample, stretched, to 160x100. Swizzle BGRA→RGBA into `engine.zone.imagedata` (64,000 bytes).
8. **Zones.** Each 28x20 cell is the mean of the 160x100 pixels it covers: 5.714 px wide (use fractional weights) and 5 px tall.
   - Basis: `ExEngineBindings::BuildZones` sits next to the string `nSmpIdx < 16000` [V-local strings 112019-112020]. Whether SignalRGB averages or point-samples, and stretches or letterboxes, is [U].
   - HSL: CSS formulas. `hue = round(h) % 360` (Int16). Saturation and lightness `round(·100)` (Int8 0..100) [V ranges: meter-tags docs + effects-engine.md].
9. **Publish** `ZoneSnapshot` to shm. The renderer applies `.set()`.
10. **Power.** Capture only while `uses.video || uses.meters` (`parse.rs:159-160`) [V-local]. Release the duplication when idle.

GPU time is about 0.1-0.5 ms and CPU about 0.2 ms per frame [U estimates]. A 2-slot staging ring avoids `Map` stalls.

**Linux (M7):** xcap 0.9.8 `video_recorder()` (ScreenCast + PipeWire) on Wayland, or xcb on X11, plus a CPU box filter. fast_image_resize 6.1.0 needs MSRV 1.87 [V], so a hand-written box filter is preferable.

---
## 3. Network plugins

### 3.0 Execution model [D unless noted]

JS runs only on the owning plugin or service thread.

- **Synchronous host calls**
  - `socket.send/write/bind/connect/close`, sync XHR, and `dtls.send` (enqueue) return immediately.
  - Return values follow SignalRGB: bytes written or −1 [V-local network-services.md].
- **I/O completions**
  - These become `HostEvent`s in a per-plugin bounded `crossbeam-channel` mailbox (already a workspace dependency [V-local]).
  - 4096 entries, drop-oldest for datagrams, with a counter.
- **Plugin loop:** `select!{ frame_seq → Render() | mailbox → dispatch | ~1 s tick → Update() }`. The Update period is [U].
- **Receive side:** receive halves run on tokio (already in the workspace). Sockets are created with socket2, converted to std, then passed to `tokio::net::UdpSocket::from_std`. The plugin thread keeps cloned std handles for synchronous `send_to`.
- **Lifetime:** sockets are registered per plugin and closed on stop.
- **Capabilities** (added, per network-services.md [V-local]):
  - TCP sockets must be refused in the service (discovery) context.
  - `XMLHttpRequest` exists in both the service context and the controller context.
  - `udp`, `dtls` and `base64` are `device.addFeature()` features in the controller context.
- **JS contract [V-local network-services.md]**
  - Socket events are `message|close|listening|connection|error`. `error` is PLAUSIBLE; the docs' `connected`/`disconnected` do not exist in the binary.
  - The message object is `{data, buffer, utf16, timestamp}` plus `address`/`port` (inferred).
  - `address()` returns `{address, port, protocol}`.
  - Legacy `udp.send(host,port,bytes,endianness)` is capped at 8192 bytes.

### 3.1 mDNS: mdns-sd 0.21.5 [V source]

**API**
- `ServiceDaemon::new()` "spawns a thread to run the daemon". It "does not open the mDNS multicast sockets — those are opened lazily". Failures arrive through `monitor() -> Receiver<DaemonEvent>` (bounded 100).
- `browse(&self, ty) -> Result<flume::Receiver<ServiceEvent>>`.
  - Errors with `Error::Msg` unless the type ends with `._tcp.local.` or `._udp.local.`, and with (added) `Error::Again` if the command queue is full. Normalize a missing trailing dot.
- `stop_browse(ty)`.
- `resolve_hostname(host, Option<u64 ms>) -> Receiver<HostnameResolutionEvent>`.
  - The host must end with `.local.` (added).
  - Events: `AddressesFound(String, HashSet<ScopedIp>)`, `AddressesRemoved`, `SearchTimeout`, …
- `shutdown()`.
- `enable_interface` / `disable_interface(IfKind::{All, IPv4, IPv6, Name, Addr, LoopbackV4, LoopbackV6, IndexV4, IndexV6, Predicate(IfPredicate)})`. (added) The `Predicate` variant.
- `ServiceEvent` (`#[non_exhaustive]`): `SearchStarted(String) | ServiceFound(ty, fullname) | ServiceResolved(Box<ResolvedService>) | ServiceRemoved(ty, fullname) | SearchStopped(String)`.
- `ResolvedService { ty_domain, sub_ty_domain, fullname, host, port, addresses: HashSet<ScopedIp>, txt_properties }`. Helpers: `get_addresses_v4() -> HashSet<Ipv4Addr>`, `ScopedIp::to_ip_addr()`, `TxtProperties::into_property_map_str()`.
- Default features: `async` (flume async) and `logging`.
- Dependencies: flume 0.12, if-addrs 0.15, mio 1.2 (MIT), socket-pktinfo 0.4 (MIT), socket2 0.6 `all`.

**Gotcha:** every browse and resolve channel is `bounded(10)`, and the daemon uses a blocking `send` (`call_service_listener`). One undrained receiver stalls the daemon. Use one forwarder per receiver (`recv_async()` into the mailbox).

**Sockets**
- `SO_REUSEADDR` always, plus `SO_REUSEPORT` on unix.
- IPv4 binds the **wildcard** `0.0.0.0:5353`.
- (corrected) A re-query on 2026-10-18 shows `SignalRgb` (PID 19420) holding **both** `0.0.0.0:5353` and `[::]:5353` [V-local Get-NetUDPEndpoint]. The earlier observation of chrome on 0.0.0.0:5353 could not be reproduced [U].
- (added) Upstream QMdnsEngine binds with `QAbstractSocket::ReuseAddressHint` on Windows [V qmdnsengine server.cpp], which Qt maps to `SO_REUSEADDR` [V Qt docs]. Coexistence with mdns-sd is therefore expected. Whether SignalRGB's bundled copy is unmodified is [U].
- Windows rule: a second `SO_REUSEADDR` wildcard bind succeeds only if the first socket also set `SO_REUSEADDR` [V MS table].

**Mapping [D]**
- One daemon per process. Disable IPv6 and loopback unless needed.
- `ServiceResolved` → `Discovered({hostname: host /*keep trailing '.'*/, name, port, ip: first IPv4, ...TXT flattened, fullname, type})`.
- `ServiceRemoved` → `Removal({...})`.
- `service.resolve(host, cb)`: `.local.` names use `resolve_hostname`; others use `ToSocketAddrs` on a blocking pool. `cb({protocol:"IPV4"|"IPV6", ip})` is called per address. SignalRGB uses QHostInfo here [V-local].

### 3.2 UDP facade (socket2 0.6.5 + std + windows)

socket2 provides [V source]:
- `Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))`
- `set_reuse_address`, `set_broadcast`, `join_multicast_v4(&group, &iface)`, `set_multicast_if_v4`, `set_multicast_ttl_v4`, `set_multicast_loop_v4`, `set_nonblocking`, `set_read_timeout`
- No `SIO_UDP_CONNRESET` / `SIO_UDP_NETRESET` / `SO_EXCLUSIVEADDRUSE` API (grep).

**1. ICMP errors**
- recvfrom `WSAECONNRESET`: "On a UDP-datagram socket this error indicates a previous send operation resulted in an ICMP Port Unreachable message".
- `WSAENETRESET`: "For a datagram socket, this error indicates that the time to live has expired" [V MS recvfrom].
- Disable both per socket, before use:
  - `SIO_UDP_CONNRESET = 2550136844u32` ("Set to FALSE to disable reporting");
  - (added) `SIO_UDP_NETRESET = 2550136847u32` [V MS winsock-ioctls + windows 0.62].
- Also treat `ConnectionReset` as non-fatal.

**2. Port sharing** [V MS SO_REUSEADDR/SO_EXCLUSIVEADDRUSE article]
- After a second `SO_REUSEADDR` bind, behaviour "is indeterminate".
- Exception: sockets bound to the same interface and port that are members of the same multicast group all receive the data.
- `SO_EXCLUSIVEADDRUSE` must be set **before** `bind`. A conflict fails with `WSAEADDRINUSE` or `WSAEACCES`.
- Policy [D]:
  - Fixed unicast listener ports (Govee 4002, Twinkly 59136): `SO_EXCLUSIVEADDRUSE`, and report the conflict.
  - Multicast-joined sockets (Govee 239.255.255.250:4001): `SO_REUSEADDR`.
  - Device control sockets: `bind(0)`.
  - (added) **Cololight sets `UdpBroadcastPort = UdpListenPort = 12345`** [V-local Cololight.js:664-665]. When the ports are equal, open **one** socket for both roles; a second bind with `SO_EXCLUSIVEADDRUSE` would fail.
  - Qt's `DefaultForPlatform` on Windows is "ShareAddress", which "is the default behavior, so this option is ignored" [V Qt docs]. Which flags SignalRGB's discovery sockets use is [U].

**3. Broadcast**
- `SO_BROADCAST` is required.
- On multi-homed hosts, 255.255.255.255 behaviour is [U] (forum anecdote).
- Send a directed broadcast per up IPv4 interface: `if_addrs::get_if_addrs()` → `Interface { name, addr: IfAddr::V4(Ifv4Addr { ip, netmask, prefixlen, broadcast: Option<Ipv4Addr> }), index, oper_status, is_p2p, .. }` with `is_oper_up()` [V source].
- (added) On Windows `broadcast` comes from the first matching on-link prefix. Computing `ip | !mask(prefixlen)` yourself is safer [D].

**4. Multicast:** join on every up IPv4 interface; `set_multicast_if_v4` per send; TTL 1-4; loop off [D].

**5. Firewall**
- With "Allow unicast response to multicast or broadcast requests" enabled, WFAS "waits as long as three seconds for unicast responses from the other computers and then blocks all later responses" [V MS WS2008 doc].
- (corrected) Whether replies to a *different* local port (Govee 4001 → 4002) fall outside this window is [U]. Plan an inbound rule anyway.
- SignalRGB has **40** inbound Allow rules named `signalrgb.exe`: TCP+UDP pairs per `vortxengine\app-2.5.x` path, Public profile [V-local Get-NetFirewallRule].
- Lightloom: install to a stable path and have the WiX MSI add a program rule [D].

**6. Excluded ports** [V-local `netsh`]
- UDP excluded 50000-50059, marked "*" = "administered port exclusion" (corrected: not shown as a Hyper-V/WinNAT reservation).
- Dynamic range 49152-65535. Twinkly's 59136 lies inside it, so a bind can collide with ephemeral ports.
- Binds into excluded ranges fail with `WSAEACCES` [U general]. Surface the failure as a socket `error` event.

```rust
// sketch (not compiled); windows features: Win32_Networking_WinSock + Win32_System_IO (WSAIoctl uses OVERLAPPED)
#[cfg(windows)]
fn harden_udp(s: &socket2::Socket) -> std::io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows::Win32::Networking::WinSock::{WSAIoctl, SIO_UDP_CONNRESET, SIO_UDP_NETRESET, SOCKET};
    for code in [SIO_UDP_CONNRESET, SIO_UDP_NETRESET] {
        let off: u32 = 0; let mut ret = 0u32;                      // FALSE = do not report
        let rc = unsafe { WSAIoctl(SOCKET(s.as_raw_socket() as usize), code,
            Some(&off as *const u32 as *const _), 4, None, 0, &mut ret, None, None) };
        if rc != 0 { return Err(std::io::Error::last_os_error()); }
    }
    Ok(())
}
#[cfg(windows)]
fn exclusive_addr_use(s: &socket2::Socket) -> std::io::Result<()> { // call BEFORE bind
    use std::os::windows::io::AsRawSocket;
    use windows::Win32::Networking::WinSock::{setsockopt, SOCKET, SOL_SOCKET, SO_EXCLUSIVEADDRUSE};
    let rc = unsafe { setsockopt(SOCKET(s.as_raw_socket() as usize), SOL_SOCKET, SO_EXCLUSIVEADDRUSE, Some(&1i32.to_ne_bytes())) };
    if rc == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}
```

**Verified windows 0.62 facts [V windows-docs-rs + windows-rs 0.62.0 source]**
- `WSAIoctl(s: SOCKET, dwiocontrolcode: u32, lpvinbuffer: Option<*const c_void>, cbinbuffer: u32, lpvoutbuffer: Option<*mut c_void>, cboutbuffer: u32, lpcbbytesreturned: *mut u32, lpoverlapped: Option<*mut OVERLAPPED>, lpcompletionroutine: LPWSAOVERLAPPED_COMPLETION_ROUTINE) -> i32`, gated by `#[cfg(feature = "Win32_System_IO")]`.
- `setsockopt(s, level: i32, optname: i32, optval: Option<&[u8]>) -> i32`.
- `SOCKET(pub usize)`, `SOL_SOCKET = 65535`, `SO_EXCLUSIVEADDRUSE = -5`.

**Discovery sockets (service context)**
- Bind `UdpBroadcastPort`, join the group if `UdpBroadcastAddress` is multicast, and bind the listener on `UdpListenPort` (0 means ephemeral). Use a single socket when the two ports are equal.
- Every datagram becomes `Discovered({id: <sender MAC via §4>, ip, port, response: utf8_lossy(payload)})`.
- Look up the MAC before dispatch, with a timeout of 500 ms or less [D].

### 3.3 HTTP for `XMLHttpRequest`: ureq 3.4.2 vs reqwest 0.13.5 [V sources]

| | ureq 3.4.2 | reqwest 0.13.5 (`blocking`) |
|---|---|---|
| License, MSRV, edition | MIT OR Apache-2.0, 1.85, 2024 | MIT OR Apache-2.0, 1.85, 2021 |
| Model | Blocking API, matches sync XHR on the plugin thread | `reqwest::blocking::Client` spawns "reqwest-internal-sync-runtime" with a current-thread tokio runtime |
| Default TLS | Feature `rustls` = `rustls-no-provider` + `_ring` + `rustls-webpki-roots`, i.e. rustls 0.23 + ring + webpki-roots 1.0 (**CDLA-Permissive-2.0, not in deny.toml**). `platform-verifier` is available. `rustls-no-provider` is listed under "Unstable" | `default-tls = rustls` = aws-lc-rs + rustls-platform-verifier. (corrected) reqwest **0.12** defaulted to native-tls [V reqwest-0.12.28 Cargo.toml: `default-tls = [hyper-tls, native-tls-crate, …]`]. aws-lc-sys on Windows wants NASM, or prebuilt-nasm objects via the `prebuilt-nasm` feature or `AWS_LC_SYS_PREBUILT_NASM=1` [V aws-lc-rs docs] |
| Timeouts (added) | Defaults all `None` except `await_100 = 1 s`, so set `timeout_global` yourself. Knobs: `timeout_global`, `timeout_resolve`, `timeout_connect`, `timeout_send_request`, `timeout_recv_response`, `timeout_recv_body` | Blocking client default total timeout **30 s**; `timeout()`, `connect_timeout()` |
| Defaults that bite | `http_status_as_error: true`, `proxy: Proxy::try_from_env()`, `max_redirects: 10` | Default feature `system-proxy` (hyper-util `client-proxy-system`). Whether LAN calls get proxied is [U] |
| Footprint | Own HTTP/1.1 stack (ureq-proto 0.6, MSRV 1.85) | hyper-util, h2 (`http2` default), tower, tower-http, aws-lc-rs C build |

**Recommendation [D]: ureq.** Two options:
- **Option A:** default features, and add `CDLA-Permissive-2.0` to deny.toml.
- **Option B (no webpki-roots)** (corrected, from ureq 3.4.2 `tls/rustls.rs`):
  1. `ureq` with `default-features = false, features = ["rustls-no-provider","platform-verifier","gzip"]`. **`default-features = false` must sit in `[workspace.dependencies]`**: edition-2021 members cannot turn defaults off [V Cargo reference].
  2. You **must** set `root_certs(RootCerts::PlatformVerifier)`. The default `RootCerts::WebPki` panics with "WebPki is disabled. You need to explicitly configure root certs on Agent".
  3. Provider order: TlsConfig provider → `rustls::crypto::CryptoProvider::get_default()` → ring if `_ring` → panic. So add `rustls` (features `ring, std, tls12, logging`) and call `install_default()` once. That avoids the unversioned `unversioned_rustls_crypto_provider` API.

```rust
let _ = rustls::crypto::ring::default_provider().install_default(); // Option B only
let tls = ureq::tls::TlsConfig::builder()
    .root_certs(ureq::tls::RootCerts::PlatformVerifier)              // required when rustls-webpki-roots is off
    .build();
let agent: ureq::Agent = ureq::Agent::config_builder()
    .tls_config(tls)
    .http_status_as_error(false)                 // Hue/Nanoleaf pairing relies on 403 bodies reaching JS
    .proxy(None)                                 // never route LAN device calls via HTTP(S)_PROXY
    .timeout_connect(Some(Duration::from_secs(3)))
    .timeout_global(Some(Duration::from_secs(10)))
    .max_redirects(5)
    .build().new_agent();
let req = ureq::http::Request::builder().method(m).uri(url) /* .header(k,v)… */ .body(body_vec)?; // Vec<u8>: AsSendBody
match agent.run(req) {
    Ok(mut resp) => { let status = resp.status().as_u16();
                      let bytes = resp.body_mut().with_config().limit(8 << 20).read_to_vec()?; }
    Err(_) => { /* XHR: readyState=4, status=0, onerror */ }
}
```

Verified ureq names [V source]: `Agent::config_builder`, `Config::new_agent`, `Agent::run`, `Body::with_config().limit().read_to_vec()`, `ureq::http` re-export, `TlsConfig::builder().disable_verification(true)` (maps `ignoreSslErrors(true)`).

**XHR semantics [D, following network-services.md]**
- `open(method, url, async=true)`.
- Sync path: run on the plugin thread, capped by `xhr.timeout` or 10 s. Fire `onreadystatechange` (readyState 4) **before `send()` returns**; plugins rely on this [V-local].
- Async path: a small blocking pool, then deliver to the mailbox.
- Provide `status`, `statusText`, `responseText`, `response`, `getResponseHeader`, `getAllResponseHeaders`, and `onload/onerror/ontimeout`.

**Network policy**
- architecture-synthesis currently says "RFC1918/loopback denied unless declared" [V-local line 144]. Network plugins **must** reach RFC1918 and link-local addresses [D].
- Deny loopback and 0.0.0.0, and check the resolved IP at connect time.
- (corrected) The hook exists: `ureq::unversioned::resolver::Resolver::resolve(&self, uri, config, timeout) -> Result<ResolvedSocketAddrs, Error>` plus `Agent::with_parts(config, connector, resolver)`. It lives in the `unversioned` module (may change in minor versions) [V source].

### 3.4 DTLS 1.2 PSK client (Philips Hue Entertainment)

**Facts**
- [V-local network-services.md]
  - The SignalRGB Hue plugin uses the v1 API and `dtls.createConnection(ip, 2100, username, clientkey)`.
  - Host `dtls`: `createConnection, send(data, endianness) -> int, hasEncryptedConnection, CloseConnection, onConnectionEstablished/Closed/Error, ignoreSslErrors`.
  - (added) SignalRGB ships `libssl-3-x64.dll`/`libcrypto-3-x64.dll` (OpenSSL 3) and `datachannel.dll`. Which one backs `dtls` is UNKNOWN.
- [V diyHue `entertainment.py`, an emulator, repo license "Other"]: `openssl s_server -4 -dtls1_2 -listen -cipher PSK-AES128-GCM-SHA256:PSK-AES128-CCM8:PSK-AES128-CCM:@SECLEVEL=0 -psk <client_key> -psk_identity <username> -nocert -accept 2100`. It reads decrypted HueStream from stdout.
- [V OpenSSL 3.0 s_server docs]
  - `-psk` is "a hexadecimal number".
  - `-listen` answers cookie-less ClientHellos with HelloVerifyRequest.
  - (added) `-psk_identity` only "warn[s]" on a mismatched identity; it does not reject.
- [V UCL page] node-dtls-client with `psk: {[username]: Buffer.from(clientkey,'hex')}`, `ciphers: ["TLS_PSK_WITH_AES_128_GCM_SHA256"]`, port 2100.
- [V] The official Hue Entertainment docs are behind a login wall.
- Summary: DTLS 1.2, cipher 0x00A8 (OpenSSL: `0x00,0xA8 PSK-AES128-GCM-SHA256 TLSv1.2` [V-local]).
  - Identity = username (v1). For v2 it would be the `hue-application-id` response header, but that comes only from the third-party `hues` crate [U].
  - Key = hex-decoded clientkey (32 hex chars → 16 bytes) [U official; consistent with diyHue and UCL].

**Candidates**

Downloads are crates.io totals / last 90 days, 2026-10-18 [V].

| Crate | Version, license | PSK-AES128-GCM? | Notes |
|---|---|---|---|
| **rtc-dtls** | 0.21.1, MIT/Apache-2.0, edition 2024, default `crypto-ring` | **yes**, `Tls_Psk_With_Aes_128_Gcm_Sha256 = 0x00a8` [V] | Sans-IO `Endpoint`. Handles HelloVerifyRequest (flight3) [V]. `webrtc` 0.21.1 depends on `rtc ^0.21.1` [V]. 46 releases since 2024-03, with alpha/beta/rc → 0.21.0 (2026-09-19) → 0.21.1 [V], so pin `=0.21.1`. (added) Depends on `rcgen ^0.14.8`; every rcgen ≥ 0.14.8 declares **MSRV 1.88** [V], so the tree needs Rust ≥ 1.88. Downloads 1.26 M / 1.23 M |
| **dtls** (added) | 0.17.2 (2026-07-20), MIT/Apache-2.0, edition 2021 | **yes** (0x00a8) [V source] | The renamed continuation of webrtc-dtls (same repo path `webrtc-rs/webrtc/tree/master/dtls`). Async on tokio, `DTLSConn::new(conn, Config, is_client, state)` with the same `Config { psk, psk_identity_hint, cipher_suites, extended_master_secret, … }` as hues. Uses rcgen 0.13 (no declared MSRV); deps' declared MSRVs ≤ 1.65 → likely fits 1.85 [U compile check]. Downloads 2.39 M |
| webrtc-dtls | 0.12.0 (2025-05-14), MIT OR Apache-2.0 | yes [V] | Async on tokio. Superseded by `dtls`/`rtc-dtls`. (corrected) `hues` 0.1.3 depends on **webrtc-dtls 0.8** (not 0.12). It uses `ExtendedMasterSecretType::Require`, `insecure_skip_verify: true`, and passes **`client_key.as_bytes()` (ASCII, not hex-decoded)** as the PSK, which contradicts diyHue/UCL. So hues proves nothing about interop (3.7 k downloads) [V source] |
| openssl | 0.10.81 (Apache-2.0, MSRV 1.80); openssl-sys 0.9.117 (MIT) | yes: `SslMethod::dtls_client()` + `set_psk_client_callback(F: Fn(&mut SslRef, Option<&[u8]>, &mut [u8] /*identity, NUL-terminated*/, &mut [u8] /*psk*/) -> Result<usize, ErrorStack> + 'static + Sync + Send)` [V docs.rs] | (corrected) Vendored builds use `openssl-src ^300.2.0` (openssl-sys requirement), i.e. 300.6.1+3.6.3 = **OpenSSL 3.6.3** (Apache-2.0), not the 400.x/OpenSSL 4 line [V]. MSVC build: Perl required (`OPENSSL_SRC_PERL`/`PERL`); NASM optional, else `no-asm` [V openssl-src source]. A datagram BIO adapter is needed. Closest to what SignalRGB ships. Good fallback |
| mbedtls | 0.13.6, Apache-2.0 OR GPL-2.0-or-later | mbedTLS supports it | Requires `mbedtls-sys-auto ^2.25.0` → 2.28.15 [V]. (corrected) The Mbed TLS 2.28 line is **EOL**: BRANCHES.md lists only 3.6 (to March 2027) and 4.1 (to March 2029) [V]. Heavy C build. Not recommended |
| boring | 5.3.0, Apache-2.0 | **no**: BoringSSL has only PSK-AES128/256-CBC-SHA, ECDHE-PSK-AES128/256-CBC-SHA and ECDHE-PSK-CHACHA20-POLY1305 [V ssl_cipher.cc] | rejected |
| dimpl | 0.7.6, MIT OR Apache-2.0 | **no**: only PSK suite is `PSK_AES128_CCM_8` (0xC0A8); EMS "negotiated and enforced" [V README/source] | rejected. It would interoperate with diyHue (which offers CCM8) but probably not with a real bridge [U] |
| rustls | 0.23.46 | no: TLS1.2/1.3 only; will not support "Ciphersuites without forward secrecy" [V manual] | rejected |

**Recommendation (verifier-adjusted) [D]**
- **rtc-dtls `=0.21.1` + rtc-shared `=0.21.1`**, only if the team accepts or raises the toolchain floor to Rust ≥ 1.88. The workspace already violates 1.85 via triple_buffer 9.0.0 at 1.86.
- Otherwise **dtls `=0.17.2`**.
- Either way, wrap it behind a `DtlsPskClient` trait and keep an `openssl` (vendored) fallback behind a cargo feature.

```rust
// rtc-dtls 0.21.1 names [V source]
use rtc_dtls::{config::{ConfigBuilder, ExtendedMasterSecretType}, cipher_suite::CipherSuiteId,
               endpoint::{Endpoint, EndpointEvent}, crypto_provider};          // crypto_provider = re-export of rtc-crypto
use rtc_shared::TransportProtocol;
let key = hex::decode(&clientkey_hex)?;                                        // 32 hex chars -> 16 bytes
let cfg = std::sync::Arc::new(ConfigBuilder::default()
    .with_crypto_provider(crypto_provider::default_provider()?)                // rtc_crypto::default_provider() -> Result<Arc<dyn RTCCryptoProvider>, CryptoError>
    .with_psk(Some(std::sync::Arc::new(move |_hint: &[u8]| Ok(key.clone()))))
    .with_psk_identity_hint(Some(identity.into_bytes()))                      // client: sent as ClientKeyExchange identity (flight5)
    .with_cipher_suites(vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256])
    .with_extended_master_secret(ExtendedMasterSecretType::Request)           // #[default] Request
    .build(true, Some(remote))?);                                              // errors without a provider; client+psk requires identity
let mut ep = Endpoint::new(sock.local_addr()?, TransportProtocol::UDP, None);
ep.connect(Instant::now(), remote, cfg, None)?;
// loop: while let Some(t) = ep.poll_transmit() { sock.send_to(&t.message, remote) }
// select{ recv -> ep.read(now, remote, None, BytesMut) -> Vec<EndpointEvent{HandshakeComplete|ApplicationData|PeerClosed}>;
//         sleep_until(ep.poll_timeout(&remote)) -> ep.handle_timeout(remote, now);
//         cmd Send(b) -> ep.write(now, remote, &b); Close -> ep.close(now) }
```

Defaults [V]: MTU 1200 bytes, flight interval 1 s.

**`dtls` JS mapping [D]**
- `createConnection` spawns the task, with a 10 s handshake timeout.
- `send(bytes)` returns `bytes.length` when established, else −1.
- `hasEncryptedConnection()` reads an atomic.
- Callbacks go through the mailbox.
- `ignoreSslErrors` is a no-op for PSK.

**CI test [D]**
- Run the diyHue-style `openssl s_server … -psk <hex> -psk_identity <id> -nocert -accept 2100` and assert that HueStream bytes appear on stdout.
- Also assert that stderr has no PSK-identity warning, because a wrong identity only warns [V].
- Git for Windows ships `openssl.exe` (3.2.4 locally) with PSK-AES128-GCM-SHA256 [V-local]. Availability on hosted CI images [U].
- Real-bridge interop is untested: no Hue hardware on this PC.

---
## 4. ARP/MAC lookup for `Discovered.id`

**SignalRGB behaviour [V-local]**
- Formats the id as `%1:%2:%3:%4:%5:%6` and logs `Failed to fetch mac.` (network-services.md).
- Letter case [U]. Emit lowercase [D].
- (corrected) Plugins use `value.id` as the controller key: `service.getController(value.id)` in Cololight.js:713 and Govee.js:443.
- Govee and Twinkly also use it as a cache key: `this.cache.Has(value.id)`.
- Cololight uses `value.id.slice(-8)` only for the display name (Cololight.js:774).
- Twinkly notes that the UDP MAC "differs from the MAC returned by the HTTP gestalt API (last octet is off by 1)" (Twinkly.js:269-270).
- So ids must be stable across sessions.

**Windows [V MS docs + windows-rs 0.62]**
- **`GetIpNetTable2(family: ADDRESS_FAMILY, table: *mut *mut MIB_IPNET_TABLE2) -> WIN32_ERROR`**
  - Feature-gated on `Win32_NetworkManagement_Ndis` + `Win32_Networking_WinSock`.
  - Family is `AF_INET`, `AF_INET6` or `AF_UNSPEC`.
  - Returns `NO_ERROR` or `ERROR_NOT_FOUND` (success with no rows).
  - Free with `FreeMibTable`.
  - "Any access to a MIB_IPNET_ROW2 array entry should assume padding may exist", so index through `Table`.
  - `MIB_IPNET_ROW2 { Address: SOCKADDR_INET, InterfaceIndex, InterfaceLuid: NET_LUID_LH, PhysicalAddress: [u8; 32], PhysicalAddressLength: u32, State: NL_NEIGHBOR_STATE, … }`.
  - States: Unreachable 0, Incomplete 1, Probe 2, Delay 3, Stale 4, Reachable 5, Permanent 6.
  - `WIN32_ERROR` has `is_ok()/is_err()/ok()`.
- **Fallback 1: `SendARP(destip: u32, srcip: u32, pmacaddr: *mut c_void, phyaddrlen: *mut u32) -> u32`**
  - IPv4 only, local subnet only.
  - Sends ARP if the entry is not cached.
  - `ERROR_BAD_NET_NAME` when there is no reply.
  - On Vista+, a success updates the ARP table.
  - NULL pointers cause an access violation.
  - The buffer must be ≥ 6 bytes (MS sample: `ULONG MacAddr[2]`).
  - It blocks, so run it on a worker [D].
- **Fallback 2: `ResolveIpNetEntry2(row, None)`**
  - Needs `Address` plus `InterfaceLuid` or `InterfaceIndex`.
  - "flushes any existing neighbor entry … then resolves".
  - The doc states no privilege requirement [U]. Prefer SendARP.

```rust
unsafe fn arp_lookup_win(ip: std::net::Ipv4Addr) -> Option<[u8; 6]> { // sketch, needs a compile check
    use windows::Win32::NetworkManagement::IpHelper::{GetIpNetTable2, FreeMibTable, MIB_IPNET_TABLE2};
    use windows::Win32::Networking::WinSock::{AF_INET, NlnsIncomplete, NlnsUnreachable};
    let mut t: *mut MIB_IPNET_TABLE2 = std::ptr::null_mut();
    let rc = GetIpNetTable2(AF_INET, &mut t);
    if t.is_null() { return None; }
    let mut hit = None;
    if rc.is_ok() {
        let rows = std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize);
        hit = rows.iter().find(|r| r.Address.si_family == AF_INET
                && r.Address.Ipv4.sin_addr.S_un.S_addr == u32::from_ne_bytes(ip.octets())
                && r.PhysicalAddressLength == 6
                && r.State != NlnsUnreachable && r.State != NlnsIncomplete)
            .map(|r| { let mut m = [0u8; 6]; m.copy_from_slice(&r.PhysicalAddress[..6]); m });
    }
    FreeMibTable(t as *const _);                                         // free whenever non-null
    hit                                                                  // 30 s cache; miss -> SendARP on a worker
}
```

**Linux [V man7 proc_net(5) + kernel if_arp.h]**
- `/proc/net/arp` columns: `IP address  HW type  Flags  HW address  Mask  Device`.
- Accept `flags & ATF_COM (0x02)` with a non-zero MAC. `ATF_PERM` = 0x04, `ATF_PUBL` = 0x08.
- IPv4 only. For IPv6, use netlink RTM_GETNEIGH: `rtnetlink` 0.23.0 (MIT) or `neli` 0.7.4 (BSD-3-Clause) [V crates.io].
- On a miss, poke `ip:9` with an empty UDP datagram and re-read after about 200 ms [D/U].
- The entry is usually present already, because the device just ARPed for us [U on Windows].

---
## 5. Cargo, `windows` features, deny.toml, MSRV

```toml
# [workspace.dependencies] additions (suggested). `optional` is NOT allowed in this table [V Cargo reference];
# put `optional = true` in the member: `windows-capture = { workspace = true, optional = true }`.
cpal = "0.18.2"
realfft = "3.5"
rtrb = "0.4"
mdns-sd = "0.21.5"
socket2 = { version = "0.6", features = ["all"] }
if-addrs = "0.15"
# Option A: default features => webpki-roots (CDLA-Permissive-2.0) => add that id to deny.toml
ureq = { version = "3.4", features = ["platform-verifier"] }
# Option B (no webpki-roots; default-features must be disabled HERE for edition-2021 members):
# ureq   = { version = "3.4", default-features = false, features = ["rustls-no-provider", "platform-verifier", "gzip"] }
# rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12", "logging"] }
rtc-dtls = "=0.21.1"            # tree needs Rust >= 1.88 (rcgen ^0.14.8); MSRV-friendly alternative: dtls = "=0.17.2"
rtc-shared = "=0.21.1"
windows-capture = "=2.0.1"      # optional in the member crate (WGC fallback)
wasapi = "0.26"                 # optional in the member crate (per-app loopback)
# windows 0.62 features to add: Win32_Graphics_Dxgi, Win32_Graphics_Dxgi_Common, Win32_Graphics_Direct3D,
# Win32_Graphics_Direct3D11, Win32_Graphics_Gdi, Win32_Devices_Display, Win32_Media_Audio, Win32_Media_Audio_Endpoints,
# Win32_System_Com, Win32_System_IO (required by WSAIoctl), Win32_NetworkManagement_IpHelper, Win32_NetworkManagement_Ndis,
# Win32_Networking_WinSock, Win32_UI_HiDpi (only if not using the manifest), optional Win32_UI_Accessibility (SetWinEventHook)
```

**One `windows` version.** cpal (`0.62`), wasapi (`0.62.2`), xcap (`0.62`) and windows-capture (`0.62.2`) all use `windows` 0.62 [V Cargo.toml]. Transitive `windows-sys` duplicates may still trigger the `multiple-versions = "warn"` lint [U].

**Licenses [V crates.io]**
- ring is `Apache-2.0 AND ISC`.
- rustls is `Apache-2.0 OR ISC OR MIT`.
- rustls-platform-verifier 0.7.1 is MIT OR Apache-2.0 (MSRV 1.85).
- aws-lc-rs 1.18.2 / aws-lc-sys 0.46.0 have compound permissive expressions.
- webpki-roots and webpki-root-certs 1.0.9 are **CDLA-Permissive-2.0** and fail the current deny.toml.
- socket-pktinfo, mio, pipewire, xcb and zbus are MIT. libwayshot-xcap is BSD-2-Clause. x25519-dalek (via dtls) is BSD-3-Clause. All pass.
- QRealFourier is LGPLv3 and must not be used.

**MSRV against workspace 1.85 [V crates.io declared `rust_version`]**
- **≤ 1.85:** cpal 1.85, ureq 1.85 (ureq-proto 1.85), wasapi 1.85, reqwest 1.85, rustls-platform-verifier 1.85, windows 1.82, mdns-sd 1.71, socket2 1.70, rustfft 1.61, rtrb 1.38.
- **Edition 2024 (≥ 1.85):** windows-capture, rtc-dtls, xcap, ureq, wasapi, dimpl (corrected list).
  - windows-capture's dependency MSRVs are ≤ 1.82 (rayon 1.80, windows-future 1.82, thiserror 1.77, parking_lot 1.71), so it fits 1.85.
- **Above 1.85:**
  - rtc-dtls via rcgen ≥ 0.14.8 needs **1.88** (added);
  - xcap on Linux via zbus ≥ 5.14 needs **1.87** (added);
  - fast_image_resize 6.1.0 needs 1.87;
  - the already-pinned triple_buffer 9.0.0 declares 1.86.
- The workspace uses `resolver = "2"`, which does not pick MSRV-compatible versions. Builds pass on the local stable 1.99.1.

---
## 6. Risks and how to close them (by priority)

1. **`engine.audio.freq` scale and spacing** [U]. Ship both mappings plus `freq_max`, and calibrate (§1.6). The float `FftDataReady` signal is a new hint [V-local].
2. **`rawlevel`, width and density exact formulas** [U]. Calibrate.
3. **Loopback edge cases**
   - No packets while idle [V GStreamer]: zero-fill, or a silent render stream.
   - Mute [U implied]: `IAudioEndpointVolume`.
   - Default-device change [V StreamInvalidated; U reroute]: rebuild.
   - Xrun errors [V]: count, do not rebuild.
   - DRM and exclusive-mode streams [U].
4. **DDA failure modes [V MS]:** `ACCESS_LOST`, `E_ACCESSDENIED` (secure desktop), `UNSUPPORTED` (hybrid GPU or adapter mismatch), `NOT_CURRENTLY_AVAILABLE` (more than 4 duplicators, SignalRGB included), `E_INVALIDARG` (double duplication). Mitigate with backoff plus the WGC/GDI fallback.
5. **HDR tone mapping:** query the SDR white level per DisplayConfig target. The 200-nit fallback is typical per MS. This monitor's HDR state is [U].
6. **UDP coexistence with SignalRGB**
   - Port sharing is indeterminate [V].
   - Cololight needs a single socket on 12345 (added).
   - Exclusive binds give clear errors.
   - Install the firewall rule; the different-port-reply behaviour is [U].
   - Administered exclusion 50000-50059 [V-local].
7. **mDNS stall** from an undrained receiver [V]: forwarder tasks.
8. **XHR blocking:** sync XHR inside `Render()`. ureq has no default global timeout [V], so always set `timeout_global`.
9. **TLS config panic** (added): ureq without `rustls-webpki-roots` panics unless `RootCerts::PlatformVerifier` (or Specific) is configured [V].
10. **DTLS**
    - rtc-dtls API churn (46 releases) and MSRV 1.88 (added).
    - Real-bridge interop unproven [U]: EMS, cookie, MTU 1200, key encoding.
    - Keep the `dtls` 0.17.2 or openssl fallback.
11. **ARP misses:** use a stable fallback id, for example `"00:00:" + IPv4 bytes`, and log `Failed to fetch mac.` [D].
12. **Supply chain / MSRV:** decide on CDLA-Permissive-2.0 vs ureq's unstable `rustls-no-provider`. Decide whether to raise `rust-version`: it is already exceeded by triple_buffer 1.86, rtc-dtls 1.88 and xcap Linux 1.87.

**Tests to add in M6**
- Analyzer golden tests.
- Capture benchmark (DDA vs WGC vs GDI) at 30 and 60 fps, reporting CPU %, GPU ms and p99 frame age.
- UDP loopback test: broadcast, multicast, CONNRESET/NETRESET suppression, equal-port single socket.
- mDNS test against a local `mdns-sd` responder (`register`).
- XHR sync/async semantics.
- DTLS PSK handshake against `openssl s_server`, with an identity-warning check.
- Windows ARP lookup against the gateway.

---
## 7. Local evidence used (read-only, re-checked 2026-10-18)

**Repo (not modified)**
- `docs\host-api\network-services.md`
- `docs\host-api\effects-engine.md`
- `docs\architecture.md` (line 84)
- `docs\architecture-synthesis.md` (lines 81, 144, 157, 185, 225)
- `Cargo.toml`, `Cargo.lock`, `deny.toml`, `rust-toolchain.toml`
- `crates\lightloom-inputs\src\lib.rs`
- `crates\lightloom-hal\src\{audio,capture}.rs`
- `crates\lightloom-renderer\src\inject.rs`
- `crates\lightloom-effects\src\parse.rs:155-166`
- Plan: `%USERPROFILE%\.claude\plans\use-rea-to-find-serialized-salamander.md` (pins at line 85; M3/M6 at lines 112 and 115)

**SignalRGB string dump** (`...\scratchpad\signalrgb_exe_strings.txt`), lines:
- 112019-112020 (BuildZones)
- 112101-112107 (HDR meter ranges)
- ~112773-112790 (AudioManager)
- 116120-116131 (ForceGdiChanged, HdrChanged)
- 116906-116923 (capture recovery strings)
- 122598-122621 and 124133-124142 (media settings)
- 149201-149240 (IntelligentAnalyzer, VisionCore, AudioCore)
- 151063-151068 (AcquireNextFrame, Rotate*)
- 183862-183879 (QRealFourier RTTI)

**SignalRGB catalog and plugins**
- `%USERPROFILE%\AppData\Local\WhirlwindFX\SignalRgb\cache\catalog_en.json`
- Plugins under `...\SignalRgb\cache\addons\`: `8d5fcd947b3893aafda11395839514da\Govee.js`, `2e841e8769dc21903231c8550b29bcca\Twinkly.js`, `460de8393ca8ccef9b3a183899cd80a9\Cololight.js`, `3e3612c48d8c2ca84e86270858a40d21\Yeelight.js`

**OS queries**
- `Get-NetUDPEndpoint -LocalPort 5353` (SignalRgb on 0.0.0.0 and ::)
- `Get-NetFirewallRule -DisplayName signalrgb.exe` (40, Inbound/Allow/Public)
- `netsh int ipv4 show excludedportrange protocol=udp` (50000-50059 administered); `show dynamicport udp` (49152 + 16384)
- `Win32_VideoController`, `System.Windows.Forms.Screen`, `WmiMonitorID`, `HKLM\...\GraphicsDrivers\MonitorDataStore`
- `openssl version` / `openssl ciphers` (Git for Windows 3.2.4)
- `node` typed-array checks

**Downloads:** `...\scratchpad\research\_downloads\inputs-network-verify\`
- `crates\`: cpal-0.18.2, wasapi-0.26.0, xcap-0.9.8, windows-capture-2.0.1, mdns-sd-0.21.5, rtc-dtls-0.21.1, rtc-shared-0.21.1, rtc-crypto-0.21.1, webrtc-dtls-0.12.0, dtls-0.17.2, dimpl-0.7.6, ureq-3.4.2, reqwest-0.13.5, reqwest-0.12.28, socket2-0.6.5, if-addrs-0.15.0, hues-0.1.3, realfft-3.5.0, rtrb-0.4.0
- `api\`: crates.io JSON
- `web\`: QRealFourier, lightscript-workshop, gst, winrs, linux, boringssl, diyhue, mbedtls, openssl-src, qmdnsengine

---
## Sources

**SignalRGB docs**
- https://docs.signalrgb.com/developer/lightscripts/audio-visualizer/
- https://docs.signalrgb.com/developer/lightscripts/meter-tags/

**crates.io metadata**
- `https://crates.io/api/v1/crates/<name>` for: cpal, realfft, rustfft, rtrb, windows-capture, xcap, mdns-sd, socket2, if-addrs, ureq, ureq-proto, reqwest, rtc-dtls, rtc-shared, rtc, rtc-crypto, webrtc, webrtc-dtls, dtls, openssl, openssl-sys, openssl-src, mbedtls, mbedtls-sys-auto, boring, dimpl, rustls, webpki-roots, webpki-root-certs, ring, rustls-platform-verifier, aws-lc-rs, aws-lc-sys, wasapi, hues, fast_image_resize, triple_buffer, rtnetlink, neli, zbus, rcgen, pipewire, libwayshot-xcap, xcb, socket-pktinfo, mio, windows, crossbeam-channel, flume
- Dependency listings:
  - https://crates.io/api/v1/crates/webrtc/0.21.1/dependencies
  - https://crates.io/api/v1/crates/openssl-sys/0.9.117/dependencies
  - https://crates.io/api/v1/crates/mbedtls/0.13.6/dependencies
  - https://crates.io/api/v1/crates/dtls/0.17.2/dependencies
- Search: https://crates.io/api/v1/crates?q=dtls

**Crate sources** (`https://static.crates.io/crates/<name>/<name>-<ver>.crate`)
- cpal 0.18.2 (src/host/wasapi/{mod,device,stream}.rs, src/traits.rs, src/lib.rs, src/error.rs, src/platform/mod.rs, src/host/pipewire/device.rs, CHANGELOG.md, UPGRADING.md, README.md)
- wasapi 0.26.0
- xcap 0.9.8
- windows-capture 2.0.1
- mdns-sd 0.21.5
- rtc-dtls 0.21.1, rtc-shared 0.21.1, rtc-crypto 0.21.1
- webrtc-dtls 0.12.0, dtls 0.17.2, dimpl 0.7.6
- ureq 3.4.2
- reqwest 0.13.5, reqwest 0.12.28
- socket2 0.6.5, if-addrs 0.15.0
- hues 0.1.3
- realfft 3.5.0, rtrb 0.4.0

**Microsoft documentation**
- https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording
- https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_5/nf-dxgi1_5-idxgioutput5-duplicateoutput1
- https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api
- https://learn.microsoft.com/en-us/troubleshoot/windows-client/shell-experience/error-when-dda-capable-app-is-against-gpu
- https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-generatemips
- https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_sdr_white_level
- https://learn.microsoft.com/en-us/windows/win32/direct3darticles/high-dynamic-range
- https://learn.microsoft.com/en-us/answers/questions/1457052/using-the-desktop-duplication-api-with-hdr-interpr
- https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.minupdateinterval
- https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.isborderrequired
- https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse
- https://learn.microsoft.com/en-us/windows/win32/winsock/winsock-ioctls
- https://learn.microsoft.com/en-us/windows/win32/api/winsock/nf-winsock-recvfrom
- https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2008-r2-and-2008/cc742429(v=ws.10)
- https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-getipnettable2
- https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-sendarp
- https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-resolveipnetentry2

**GStreamer**
- https://github.com/GStreamer/gst-plugins-bad/commit/a668de747fd3bffb78550c37f82a33a30d489b27 (part of gitlab MR 1588; gitlab.freedesktop.org is behind Anubis)
- https://github.com/GStreamer/gstreamer/commit/0afeacce921f31558968233033fe0c2312876e30 (MR 5671)
- https://github.com/GStreamer/gstreamer/commit/126d6f0815f231fc2eb6e92bb1a1bd0e1ab1f159 (MR 8227)
- https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2rbuf.cpp
- https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-bad/sys/wasapi2/gstwasapi2src.cpp
- https://gstreamer.freedesktop.org/documentation/wasapi2/wasapi2src.html

**Third-party sources**
- https://github.com/visore/QRealFourier (License.txt, code/headers/qwindowfunction.h, code/sources/qwindowfunction.cpp, code/sources/qfouriertransformer.cpp)
- https://github.com/hyperb1iss/lightscript-workshop (packages/core/src/utils/audio.ts, packages/dev/src/audio/audio-analyzer.ts)
- https://windows-hexerror.linestarve.com/q/so48207677-dxgi-15-duplicateoutput1-fails-with-dxgierrorunsupported-0x887a0004 (mirror of SO 48207677)

**windows-rs**
- https://microsoft.github.io/windows-docs-rs/doc/windows/ pages: SIO_UDP_CONNRESET, WSAIoctl, GetIpNetTable2, SendARP, ResolveIpNetEntry2, MIB_IPNET_ROW2, MIB_IPNET_TABLE2, FreeMibTable, SOCKADDR_INET, IDXGIOutput5, IDXGIOutputDuplication, DisplayConfigGetDeviceInfo, DISPLAYCONFIG_SDR_WHITE_LEVEL, IAudioEndpointVolume
- https://raw.githubusercontent.com/microsoft/windows-rs/0.62.0/crates/libs/windows/src/Windows/Win32/Networking/WinSock/mod.rs (plus IpHelper/mod.rs, Foundation/mod.rs, UI/Accessibility/mod.rs, extensions/Win32/Foundation/WIN32_ERROR.rs)

**Linux**
- https://man7.org/linux/man-pages/man5/proc_net.5.html
- https://raw.githubusercontent.com/torvalds/linux/master/include/uapi/linux/if_arp.h

**TLS / DTLS**
- https://aws.github.io/aws-lc-rs/requirements/windows.html
- https://docs.rs/openssl/0.10.81/openssl/ssl/struct.SslContextBuilder.html
- https://docs.rs/openssl/0.10.81/openssl/ssl/struct.SslMethod.html
- https://raw.githubusercontent.com/alexcrichton/openssl-src-rs/main/README.md and src/lib.rs
- https://boringssl.googlesource.com/boringssl/+/refs/heads/main/ssl/ssl_cipher.cc
- https://docs.rs/rustls/latest/rustls/manual/_04_features/index.html
- https://raw.githubusercontent.com/Mbed-TLS/mbedtls/development/BRANCHES.md

**Hue**
- https://raw.githubusercontent.com/diyhue/diyHue/master/BridgeEmulator/services/entertainment.py
- https://docs.openssl.org/3.0/man1/openssl-s_server/
- https://students.cs.ucl.ac.uk/2024/group1/docs/implementation/phillips-hue
- https://developers.meethue.com/develop/hue-entertainment/hue-entertainment-api/ (login wall)

**Qt and mDNS**
- https://raw.githubusercontent.com/nitroshare/qmdnsengine/master/src/src/server.cpp
- https://doc.qt.io/qt-6/qabstractsocket.html

**Cargo**
- https://doc.rust-lang.org/cargo/reference/workspaces.html
- https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html

---

## Fact-check corrections applied to the first draft

- Repo context: lightloom-hal CaptureFrame is BGRA8 (not RGBA); engine.zone.imagedata needs a BGRA->RGBA swizzle. Added: workspace uses resolver = "2" (not MSRV-aware); deny.toml also allows Apache-2.0 WITH LLVM-exception, Unicode-3.0/Unicode-DFS-2016, CC0-1.0 and sets [graph] all-features=false with Windows+Linux targets.
- SignalRGB strings: 'Capture Framerate' and 'Compatibility Mode (Slower)' are media/VIDEO capture options (Compatibility Mode = GDI capture: 'Use GDI for capture (slower), instead of directx'), not audio settings; the brief's use of 'Capture Framerate' as audio-tick evidence is downgraded to [U].
- 'This size is not a default fixed size of QRealFourier. Using a variable size instead.' and 'Invalid FFT size.' are SignalRGB's own log strings (absent from QRealFourier source), reacting to QFourierTransformer::setSize(); SignalRGB's FFT size is configurable/derived (still power-of-two).
- Added local evidence: IntelligentAnalyzer::FftDataReady(float* pvData, aiSize, anPeak) — SignalRGB's internal FFT output is float.
- QRealFourier detail: 'Rectangular' means no window (createFunction returns NULL); RTTI in SignalRGB shows QFourierFixedCalculator<3..14>, FFTReal<float>, QHammingFunction/QHannFunction<float>.
- cpal 0.18.2: capture xruns are now reported as ErrorKind::Xrun on WASAPI (DATA_DISCONTINUITY); error handler must not rebuild on Xrun; ErrorKind is #[non_exhaustive]. Code sketch updated (also uses rtrb push_partial_slice for F32).
- cpal: on WASAPI default_output_config() = GetMixFormat (the generic 0.18 'prefer 48 kHz' rule does not change it). cpal source is internally inconsistent about default-device rerouting (device.rs says the engine reroutes; stream.rs says WASAPI never rebinds and emits StreamInvalidated, loop continues) -> reroute behaviour [U], rebuild policy kept.
- cpal Linux: besides the pipewire feature and libpipewire-0.3-dev, ALSA headers (libasound2-dev) are always required and PipeWire >= 0.3.53; default_host() prefers PipeWire when the feature is enabled and available.
- WASAPI facts: the MS DRM sentence concerns trusted drivers/loopback devices, so 'DRM streams are silent in WASAPI loopback' is [U]; exclusive-mode apps missing from loopback is [U]. Added MS fact: loopback captures the mix of all sessions (session-0 service capture works).
- GStreamer MR 1588 claim verified via the GitHub mirror commit a668de74 (gitlab is Anubis-blocked); GStreamer's fix is a dummy render client feeding silence. 'Loopback still captures while muted' is implied by loopback-silence-on-device-mute, not stated -> [U].
- wasapi 0.26: added that process loopback requires Windows 10 build 20348 (GStreamer docs); MSRV 1.85, edition 2024.
- JS semantics: Int8Array.map(v=>v/127) returns 0 for every element except 127 -> 1 (not literally all zeros) [V Node]; new Int8Array([200,255,128]) -> [-56,-1,-128] confirms H1 wrap.
- lightloom-effects parse.rs:160 already has a separate meters flag (meter tags || engine.vision); capture gating should be video || meters.
- Added §2.0: SignalRGB video capture is DXGI Desktop Duplication (AcquireNextFrame/Unhandled DXGI Error/Rotate90..270 strings) with a GDI fallback ('Compatibility Mode'), an HDR colourspace flag, and an adapter-mismatch error; no WGC strings found.
- xcap 0.9.8 and windows-capture's DxgiDuplicationApi both create the D3D device on the default adapter only (D3D11CreateDevice(None, HARDWARE)); monitors on other adapters are not found (windows-capture returns Error::OutputNotFound).
- xcap Linux: Wayland capture_image uses the portal Screenshot API (one-shot); continuous capture requires video_recorder() (portal ScreenCast + PipeWire). xcap requires zbus >= 5.17, whose MSRV is 1.87 (> workspace 1.85).
- DDA pointer claim corrected: per MS, the pointer is either already drawn into the desktop image or reported separately (PointerPosition.Visible), not always separate.
- Withdrawn: 'DuplicateOutput1 may report FP16 in GetDesc() even when only BGRA was requested [V MS Q&A]' — the cited Q&A does not show this (asker requested FP16) -> [U].
- Added DDA facts: E_INVALIDARG when the app already duplicates the output; MS hybrid-GPU KB 3019314 applies to Windows 8.1 (Win10/11 [U]); HDR detection via GetDesc1 ColorSpace (SDR Advanced Color also reports G22_P709); Win32 apps poll IDXGIFactory1::IsCurrent; GenerateMips support is FL9.1+ for BGRA8 and FL9.2+ for FP16 (GenerateMips doc).
- SDR white fallback of 200 nits upgraded to [V]: MS says desktop HDR SDR reference white is 'typically set to around 200 nits'.
- This PC: monitor identified as Samsung LC27G5xT (WmiMonitorID); HDR state [U]; Parsec adapter being IddCx is [U].
- Execution model: per network-services.md, TCP sockets must be refused in the service context and XMLHttpRequest exists in both service and controller contexts (the brief's 'only network contexts get udp/tcp/dtls/XHR' refined).
- mdns-sd: IfKind also has Predicate(IfPredicate); resolve_hostname requires '.local.'; browse() returns Error::Again when the command queue (bounded 100) is full; IPv4 bind is wildcard 0.0.0.0:5353 with SO_REUSEADDR.
- 5353 observation corrected: re-query shows SignalRgb holding both 0.0.0.0:5353 and [::]:5353; the earlier chrome+SignalRGB sharing could not be reproduced. Added: upstream QMdnsEngine binds with ReuseAddressHint (SO_REUSEADDR on Windows), so coexistence is expected.
- UDP: Cololight uses UdpBroadcastPort = UdpListenPort = 12345, so the SO_EXCLUSIVEADDRUSE policy must use a single socket when ports are equal (the brief listed Cololight 12345 as an exclusive listener).
- UDP: added SIO_UDP_NETRESET (2550136847) to suppress WSAENETRESET (TTL expired) alongside SIO_UDP_CONNRESET; added SO_EXCLUSIVEADDRUSE setsockopt sketch (must be set before bind).
- windows crate: WSAIoctl in windows 0.62 is gated on feature Win32_System_IO (OVERLAPPED in its signature); the brief's feature list omitted it (would not compile).
- Firewall: '3-second unicast-response window' verified, but 'replies to a different port (Govee 4001->4002) need an inbound rule' is an inference -> [U].
- Excluded ports: 50000-50059 is marked '*' = administered port exclusion (manually added), not shown as a Hyper-V/WinNAT reservation.
- if-addrs: on Windows 'broadcast' is derived from the first matching on-link prefix; computing ip|!mask(prefixlen) is safer; Interface::is_oper_up() exists.
- reqwest 0.12 default TLS = native-tls upgraded from [U] to [V] (reqwest-0.12.28 Cargo.toml). Added timeouts: reqwest blocking default 30 s total; ureq has no default global timeout (only await_100 = 1 s).
- ureq Option B corrected: without the rustls-webpki-roots feature the default RootCerts::WebPki panics ('WebPki is disabled...'), so .root_certs(RootCerts::PlatformVerifier) is mandatory; provider order is TlsConfig -> CryptoProvider::get_default() -> ring(_ring) -> panic, so install_default() on a ring-enabled rustls avoids the unversioned API; default-features=false must be set in [workspace.dependencies] for edition-2021 members.
- ureq resolver hook upgraded from [U] to [V]: ureq::unversioned::resolver::Resolver + Agent::with_parts(config, connector, resolver) (unversioned API).
- reqwest system-proxy: feature verified; 'reads the Windows proxy for LAN requests' downgraded to [U].
- hues 0.1.3 corrected: depends on webrtc-dtls 0.8 (not 0.12) and passes client_key.as_bytes() (ASCII, not hex-decoded) as the PSK, contradicting diyHue/UCL; it proves nothing about interop.
- Added missing DTLS candidate: dtls 0.17.2 (2026-07-20, MIT/Apache-2.0, edition 2021), the renamed continuation of webrtc-dtls, tokio-based, supports Tls_Psk_With_Aes_128_Gcm_Sha256 (0x00a8), MSRV-friendly by declared deps.
- rtc-dtls 0.21.1 requires rcgen ^0.14.8 and all rcgen >= 0.14.8 declare MSRV 1.88, so its tree needs Rust >= 1.88 (the brief only said 'edition 2024, exact MSRV [U]'); recommendation adjusted: rtc-dtls only if the floor is raised, else dtls 0.17.2.
- openssl fallback corrected: openssl-sys 0.9.117 requires openssl-src ^300.2.0, so vendored builds use OpenSSL 3.6.3 (300.6.1+3.6.3), not openssl-src 400.x / OpenSSL 4; MSVC build needs Perl, NASM optional (no-asm otherwise) [V openssl-src source].
- mbedtls: 'mbedtls-sys-auto 2.28 is EOL' upgraded to [V] (Mbed TLS BRANCHES.md lists only 3.6 and 4.1 as maintained); mbedtls 0.13.6 requires mbedtls-sys-auto ^2.25.0 (resolves 2.28.15).
- openssl s_server -psk_identity only warns on mismatch (does not reject): the CI test must also check stderr. Git for Windows ships openssl.exe (3.2.4 locally) supporting PSK-AES128-GCM-SHA256 = 0x00,0xA8 [V-local].
- Added: SignalRGB ships OpenSSL 3 (libssl-3-x64.dll) and libdatachannel (network-services.md); which backs dtls is UNKNOWN; supports keeping an OpenSSL fallback.
- dimpl note added: its only PSK suite (PSK_AES128_CCM_8) is offered by diyHue but real-bridge support is [U].
- ARP id usage corrected: plugins use value.id as the controller key (service.getController) and cache key (Govee/Twinkly); Cololight uses value.id.slice(-8) only for its display name.
- ARP sketch fixed: ERROR_NOT_FOUND is a success code, so free the table whenever the pointer is non-null; also check si_family == AF_INET and exclude Unreachable/Incomplete states (NL_NEIGHBOR_STATE values verified).
- Cargo snippet corrected: `optional = true` is not allowed in [workspace.dependencies] (Cargo reference); move it to the member (`{ workspace = true, optional = true }`). Added Win32_System_IO (required) and optional Win32_UI_Accessibility (SetWinEventHook).
- MSRV section corrected: edition-2024 crates also include xcap, ureq, wasapi, dimpl; windows-capture's deps declare MSRV <= 1.82 (fits 1.85); rtc-dtls tree needs 1.88; xcap Linux tree needs 1.87.
- Added maturity data (crates.io downloads total/90d) for DTLS and other candidates.

## Still unverified after the fact-check

- SignalRGB engine.audio.freq numeric scale (0..255 unsigned vs signed dB-like floats) and bin spacing (linear vs log, frequency range); internal FFT is float (FftDataReady), but the mapping to the 200 values is unknown
- SignalRGB engine.audio.rawlevel semantics (proposed: unsmoothed linear RMS 0..1)
- Exact SignalRGB formulas for engine.audio.width and engine.audio.density (docs give only ranges and white-noise/pure-tone behaviour)
- Which QRealFourier window (Rectangular/Hamming/Hann), FFT size (power of two, possibly variable) and update rate SignalRGB uses; meaning of FftDataReady's anPeak
- That engine.audio.level is RMS-based dBFS with specific smoothing constants
- Whether SignalRGB's 'Capture Framerate' setting also governs the audio analysis rate
- Whether WASAPI loopback on the virtual default render device is automatically rerouted on a default-device change (cpal's own comments conflict)
- Whether WASAPI loopback keeps delivering audio while the endpoint is muted (implied by GStreamer's loopback-silence-on-device-mute property, not stated)
- Whether DRM-protected or other apps' exclusive-mode streams are silent in WASAPI loopback mode
- Exact HDR tone-mapping curve for FP16 scRGB desktop frames (200-nit SDR white fallback is MS-'typical', not guaranteed)
- DuplicateOutput1 requiring per-monitor-V2 DPI awareness (single Stack Overflow answer)
- Whether DuplicateOutput1 can report FP16 when only BGRA8 was requested (claim withdrawn; not shown by the cited MS Q&A)
- Microsoft Hybrid-GPU DDA limitation on Windows 10/11 (KB 3019314 is written for Windows 8.1)
- Unpackaged Win32 apps disabling the WGC border without the GraphicsCaptureAccess consent flow
- Desktop Duplication on indirect-display outputs (Parsec Virtual Display; whether it is IddCx)
- Whether this PC's Samsung LC27G5xT output is HDR-capable/enabled
- GDI capture behaviour on HDR desktops (for a GDI fallback)
- Whether SignalRGB's 28x20 zones are cell averages or point samples of the 160x100 image, and stretch vs letterbox
- Per-frame cost figures (xcap GDI/DXGI, DDA+GenerateMips GPU/CPU) and the audio latency budget (estimates to be benchmarked)
- SF_white calibration constant for density given overlap and Hann correlation
- Windows limited-broadcast (255.255.255.255) behaviour on multi-homed hosts
- Whether Windows Firewall's 3-second unicast-response allowance covers replies to a different local port (Govee 4001->4002)
- Whether SignalRGB's discovery sockets use SO_REUSEADDR/ShareAddress (affects port-conflict behaviour); whether its bundled QMdnsEngine is unmodified
- The earlier observation that chrome shared UDP 5353 with SignalRGB (not reproducible at verification time)
- if-addrs Windows prefix ordering yielding the correct broadcast address on every adapter
- SignalRGB 'Discovered' MAC id letter case and its fallback id when ARP fails
- Windows populating the ARP cache from the device's own ARP before its unicast reply; ResolveIpNetEntry2 privilege requirements; Linux zero-length-datagram neighbour poke
- SignalRGB service Update() tick period (~1 s) and UDP message object 'data'/'utf16' encodings
- Hue Entertainment interop of rtc-dtls 0.21.1 or dtls 0.17.2 with a real bridge (EMS, cookie, MTU 1200, PSK key encoding); official Hue docs are behind a login
- Hue v2 PSK identity = hue-application-id (only from the third-party hues crate); clientkey length/format from official docs
- Whether a real Hue bridge accepts TLS_PSK_WITH_AES_128_CCM_8 (dimpl's only PSK suite)
- Which SignalRGB library backs the dtls feature (OpenSSL 3 vs libdatachannel)
- That dtls 0.17.2's full dependency tree compiles under Rust 1.85 (declared MSRVs suggest yes; not compile-tested)
- Whether reqwest's default system-proxy feature routes LAN requests through a configured proxy
- Availability of openssl.exe on hosted CI images (verified only for local Git for Windows)
- WiX Firewall extension specifics for adding a program rule for lightloomd.exe
- Whether the Windows Firewall 'Allow access' prompt created SignalRGB's 40 per-version rules (pattern observed, cause inferred)
- Whether transitive windows-sys duplicates trigger cargo-deny multiple-versions warnings
