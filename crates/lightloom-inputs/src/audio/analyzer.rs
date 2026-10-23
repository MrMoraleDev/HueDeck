// SPDX-License-Identifier: GPL-3.0-or-later
//! The `engine.audio` DSP: interleaved samples in, one [`AudioSnapshot`] per
//! tick out (docs/research/m6-inputs-network.md §1.4, docs/audio.md).
//!
//! * Mono mix of every channel into an `N`-sample history (`N` = 2048 up to
//!   50 kHz, 4096 up to 100 kHz, 8192 above), periodic Hann window, real FFT
//!   on the newest `N` samples each tick (the hop is one tick's samples, so
//!   windows overlap by `N - hop`).
//! * Amplitude `A[k] = 2|X[k]| / (N/2)` (a full-scale on-bin sine is 1.0),
//!   in dBFS.
//! * 200 bands, log-spaced 20 Hz-20 kHz ([`BandScale::Log`], default) or
//!   linear 0-fs/2 ([`BandScale::Linear`]); a band takes the loudest bin in it,
//!   or interpolates at its centre when no bin falls inside. Log mode adds a
//!   3 dB/octave tilt around 1 kHz so music (pink-ish) reads flat.
//! * Bands smoothed in dB (attack 10 ms, release 150 ms), then mapped
//!   -80..-12 dBFS -> 0..127 integers.
//! * `level`: dBFS of the smoothed mean square of the hop (attack 30 ms,
//!   release 300 ms), -100..0; a full-scale sine reads -3 dB.
//! * `rawlevel`: RMS of the hop, unsmoothed, 0..1.
//! * `width`: side energy over total energy of the history (0 mono, ~0.5
//!   uncorrelated, 1 anti-phase), smoothed 250 ms.
//! * `density`: spectral flatness (50 Hz-16 kHz) of an averaged power
//!   spectrum, normalized so white noise reads ~1 and a tone ~0, smoothed 250 ms.

use super::BandScale;
use crate::{AudioSnapshot, AUDIO_FREQ_BINS};
use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// `freq` value of the loudest band.
pub const FREQ_MAX: u8 = 127;
/// Band level mapped to `freq` 0.
pub const FREQ_FLOOR_DB: f32 = -80.0;
/// Band level mapped to `freq` 127.
pub const FREQ_CEIL_DB: f32 = -12.0;
/// Silence, in dB (`level` floor, band floor).
pub const SILENT_DB: f32 = -100.0;

const LOG_LOW_HZ: f32 = 20.0;
/// 20 Hz * 1000 = 20 kHz.
const LOG_SPAN: f32 = 1000.0;
const TILT_DB_PER_OCTAVE: f32 = 3.0;
const BAND_ATTACK_S: f32 = 0.010;
const BAND_RELEASE_S: f32 = 0.150;
const LEVEL_ATTACK_S: f32 = 0.030;
const LEVEL_RELEASE_S: f32 = 0.300;
const WIDTH_TAU_S: f32 = 0.250;
const DENSITY_TAU_S: f32 = 0.250;
/// Per-tick weight of the averaged power spectrum behind `density`.
const DENSITY_EMA: f32 = 0.25;
const DENSITY_LOW_HZ: f32 = 50.0;
const DENSITY_HIGH_HZ: f32 = 16_000.0;
/// Below this mean band power `density` reads 0 (no flatness of silence).
const DENSITY_GATE_DB: f32 = -80.0;
/// Spectral flatness of white noise through this analyzer (Hann window,
/// overlapping hops, the EMA above); measured by the `white_noise_*` test.
const FLATNESS_WHITE: f32 = 0.83;
/// Mean square below which the history counts as silent for `width`.
const WIDTH_GATE: f32 = 1e-10;

/// Which FFT bins one output band reads.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Band {
    /// The loudest bin of `lo..hi`.
    Bins { lo: usize, hi: usize },
    /// No bin centre inside: interpolate between bin `k` and `k + 1`.
    Interp { k: usize, frac: f32 },
    /// Above the usable range (0.95 * fs/2).
    Off,
}

/// FFT size for a sample rate.
pub fn fft_size(sample_rate: u32) -> usize {
    match sample_rate {
        0..=50_000 => 2048,
        50_001..=100_000 => 4096,
        _ => 8192,
    }
}

/// Smoothing weight for one step of `dt` seconds with time constant `tau`.
fn alpha(dt: f32, tau: f32) -> f32 {
    1.0 - (-dt / tau).exp()
}

fn to_db(amplitude: f32) -> f32 {
    (20.0 * (amplitude + 1e-9).log10()).max(SILENT_DB)
}

/// Map a band level in dBFS to the `freq` scale (0..=127).
pub fn freq_value(db: f32) -> u8 {
    let v = f32::from(FREQ_MAX) * (db - FREQ_FLOOR_DB) / (FREQ_CEIL_DB - FREQ_FLOOR_DB);
    v.round().clamp(0.0, f32::from(FREQ_MAX)) as u8
}

/// Lower and upper edge (Hz) and centre of band `i` for `scale` at `fs`.
pub fn band_edges(scale: BandScale, i: usize, fs: f32) -> (f32, f32, f32) {
    let n = AUDIO_FREQ_BINS as f32;
    let i = i as f32;
    match scale {
        BandScale::Log => (
            LOG_LOW_HZ * LOG_SPAN.powf(i / n),
            LOG_LOW_HZ * LOG_SPAN.powf((i + 1.0) / n),
            LOG_LOW_HZ * LOG_SPAN.powf((i + 0.5) / n),
        ),
        BandScale::Linear => {
            let w = fs / 2.0 / n;
            (i * w, (i + 1.0) * w, (i + 0.5) * w)
        }
    }
}

fn band_map(scale: BandScale, fs: f32, n: usize) -> Vec<Band> {
    let df = fs / n as f32;
    let cap = 0.95 * fs / 2.0;
    let last_bin = n / 2;
    (0..AUDIO_FREQ_BINS)
        .map(|i| {
            let (lo, hi, centre) = band_edges(scale, i, fs);
            if lo >= cap {
                return Band::Off;
            }
            let hi = hi.min(cap);
            // Bins whose centre k*df lies in [lo, hi); DC (k = 0) never counts.
            let k_lo = ((lo / df).ceil() as usize).max(1);
            let k_hi = ((hi / df).ceil() as usize).min(last_bin + 1);
            if k_lo < k_hi {
                Band::Bins { lo: k_lo, hi: k_hi }
            } else {
                let pos = (centre.min(cap) / df).max(1.0);
                let k = (pos.floor() as usize).min(last_bin - 1);
                Band::Interp {
                    k,
                    frac: (pos - k as f32).clamp(0.0, 1.0),
                }
            }
        })
        .collect()
}

/// The analyzer for one stream format (see the module docs).
pub struct Analyzer {
    fs: f32,
    channels: usize,
    n: usize,
    r2c: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    /// Per-bin amplitude in dBFS of the last tick.
    amp_db: Vec<f32>,
    /// Per-bin averaged power (density).
    avg_power: Vec<f32>,
    density_bins: (usize, usize),
    /// History rings (oldest at `pos`).
    mono: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
    pos: usize,
    bands: Vec<Band>,
    tilt: Vec<f32>,
    /// Raw band levels (dB, before smoothing) of the last tick.
    band_db: Vec<f32>,
    smooth_db: Vec<f32>,
    level_power: f32,
    width: f32,
    density: f32,
}

impl std::fmt::Debug for Analyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analyzer")
            .field("fs", &self.fs)
            .field("channels", &self.channels)
            .field("n", &self.n)
            .finish_non_exhaustive()
    }
}

impl Analyzer {
    /// An analyzer for interleaved `channels`-channel audio at `sample_rate`.
    pub fn new(sample_rate: u32, channels: u16, scale: BandScale) -> Self {
        let sample_rate = sample_rate.max(8_000);
        let fs = sample_rate as f32;
        let n = fft_size(sample_rate);
        let r2c = RealFftPlanner::<f32>::new().plan_fft_forward(n);
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())
            .collect();
        let (input, spectrum, scratch) = (
            r2c.make_input_vec(),
            r2c.make_output_vec(),
            r2c.make_scratch_vec(),
        );
        let df = fs / n as f32;
        let density_hi = DENSITY_HIGH_HZ.min(0.9 * fs / 2.0);
        let density_bins = (
            ((DENSITY_LOW_HZ / df).ceil() as usize).max(1),
            ((density_hi / df).floor() as usize).min(n / 2),
        );
        let tilt = (0..AUDIO_FREQ_BINS)
            .map(|i| match scale {
                BandScale::Log => TILT_DB_PER_OCTAVE * (band_edges(scale, i, fs).2 / 1000.0).log2(),
                BandScale::Linear => 0.0,
            })
            .collect();
        Self {
            fs,
            channels: usize::from(channels.max(1)),
            n,
            r2c,
            window,
            input,
            spectrum,
            scratch,
            amp_db: vec![SILENT_DB; n / 2 + 1],
            avg_power: vec![0.0; n / 2 + 1],
            density_bins,
            mono: vec![0.0; n],
            left: vec![0.0; n],
            right: vec![0.0; n],
            pos: 0,
            bands: band_map(scale, fs, n),
            tilt,
            band_db: vec![SILENT_DB; AUDIO_FREQ_BINS],
            smooth_db: vec![SILENT_DB; AUDIO_FREQ_BINS],
            level_power: 0.0,
            width: 0.0,
            density: 0.0,
        }
    }

    /// FFT size.
    pub fn fft_size(&self) -> usize {
        self.n
    }

    /// Raw (unsmoothed, tilted) band levels in dBFS of the last tick.
    pub fn band_db(&self) -> &[f32] {
        &self.band_db
    }

    /// Feed one tick: `hop` holds the interleaved samples captured since the
    /// last tick (may be empty), `dt` the seconds since the last tick.
    pub fn process(&mut self, hop: &[f32], dt: f32) -> AudioSnapshot {
        let dt = if dt > 0.0 { dt } else { 1.0 / 60.0 };
        let ch = self.channels;
        let mut sum_sq = 0.0f64;
        for frame in hop.chunks_exact(ch) {
            let l = frame[0];
            let r = if ch > 1 { frame[1] } else { l };
            let m = frame.iter().sum::<f32>() / ch as f32;
            sum_sq += frame.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>();
            self.mono[self.pos] = m;
            self.left[self.pos] = l;
            self.right[self.pos] = r;
            self.pos = (self.pos + 1) % self.n;
        }
        let samples = (hop.len() / ch) * ch;
        let ms = if samples == 0 {
            0.0
        } else {
            (sum_sq / samples as f64) as f32
        };

        self.spectrum_db();
        self.update_bands(dt);
        let freq = self.smooth_db.iter().map(|&db| freq_value(db)).collect();

        let a = alpha(
            dt,
            if ms > self.level_power {
                LEVEL_ATTACK_S
            } else {
                LEVEL_RELEASE_S
            },
        );
        self.level_power += a * (ms - self.level_power);
        let level = (10.0 * (self.level_power + 1e-12).log10()).clamp(SILENT_DB, 0.0);

        let width = self.width_target();
        self.width += alpha(dt, WIDTH_TAU_S) * (width - self.width);
        let density = self.density_target();
        self.density += alpha(dt, DENSITY_TAU_S) * (density - self.density);

        AudioSnapshot {
            freq,
            level,
            rawlevel: ms.sqrt().clamp(0.0, 1.0),
            width: self.width.clamp(0.0, 1.0),
            density: self.density.clamp(0.0, 1.0),
        }
    }

    /// Window the history, FFT it, fill `amp_db` and `avg_power`.
    fn spectrum_db(&mut self) {
        let n = self.n;
        for i in 0..n {
            self.input[i] = self.mono[(self.pos + i) % n] * self.window[i];
        }
        // Sizes come from the plan itself, so this cannot fail.
        let _ = self
            .r2c
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch);
        let scale = 4.0 / n as f32;
        for (k, c) in self.spectrum.iter().enumerate() {
            let a = c.norm() * scale;
            self.amp_db[k] = to_db(a);
            self.avg_power[k] += DENSITY_EMA * (a * a - self.avg_power[k]);
        }
    }

    fn update_bands(&mut self, dt: f32) {
        let attack = alpha(dt, BAND_ATTACK_S);
        let release = alpha(dt, BAND_RELEASE_S);
        for (i, band) in self.bands.iter().enumerate() {
            let raw = match *band {
                Band::Bins { lo, hi } => self.amp_db[lo..hi].iter().copied().fold(SILENT_DB, f32::max),
                Band::Interp { k, frac } => self.amp_db[k] + frac * (self.amp_db[k + 1] - self.amp_db[k]),
                Band::Off => SILENT_DB,
            };
            let b = if raw <= SILENT_DB {
                SILENT_DB
            } else {
                (raw + self.tilt[i]).max(SILENT_DB)
            };
            self.band_db[i] = b;
            let y = &mut self.smooth_db[i];
            *y += if b > *y { attack } else { release } * (b - *y);
        }
    }

    fn width_target(&self) -> f32 {
        let (mut mid, mut side) = (0.0f64, 0.0f64);
        for (&l, &r) in self.left.iter().zip(&self.right) {
            let m = f64::from(l + r) / 2.0;
            let s = f64::from(l - r) / 2.0;
            mid += m * m;
            side += s * s;
        }
        let total = mid + side;
        if total / (self.n as f64) < f64::from(WIDTH_GATE) {
            0.0
        } else {
            (side / total) as f32
        }
    }

    fn density_target(&self) -> f32 {
        let (lo, hi) = self.density_bins;
        if hi <= lo {
            return 0.0;
        }
        let bins = &self.avg_power[lo..=hi];
        let count = bins.len() as f64;
        let mean = bins.iter().map(|&p| f64::from(p)).sum::<f64>() / count;
        if 10.0 * (mean + 1e-20).log10() < f64::from(DENSITY_GATE_DB) {
            return 0.0;
        }
        let eps = 1e-20;
        let log_mean = bins.iter().map(|&p| (f64::from(p) + eps).ln()).sum::<f64>() / count;
        let flatness = log_mean.exp() / (mean + eps);
        (flatness as f32 / FLATNESS_WHITE).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const FS: u32 = 48_000;

    /// Interleaved stereo sine, `amp` peak, same on both channels.
    pub(crate) fn sine(freq: f32, amp: f32, frames: usize, start: usize) -> Vec<f32> {
        (start..start + frames)
            .flat_map(|i| {
                let x = amp * (std::f32::consts::TAU * freq * i as f32 / FS as f32).sin();
                [x, x]
            })
            .collect()
    }

    /// Deterministic uniform noise in -1..1 (xorshift).
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 23) as f32 - 1.0
        }
    }

    /// Run `ticks` hops of 800 frames (60 Hz at 48 kHz) of `gen(frame)` -> (L, R).
    fn run(a: &mut Analyzer, ticks: usize, mut gen: impl FnMut(usize) -> (f32, f32)) -> AudioSnapshot {
        let hop = (FS / 60) as usize;
        let mut snap = AudioSnapshot::default();
        for t in 0..ticks {
            let buf: Vec<f32> = (t * hop..(t + 1) * hop)
                .flat_map(|i| {
                    let (l, r) = gen(i);
                    [l, r]
                })
                .collect();
            snap = a.process(&buf, 1.0 / 60.0);
        }
        snap
    }

    fn argmax(v: &[f32]) -> usize {
        v.iter()
            .enumerate()
            .fold((0, f32::MIN), |b, (i, &x)| if x > b.1 { (i, x) } else { b })
            .0
    }

    #[test]
    fn fft_size_follows_the_sample_rate() {
        assert_eq!(fft_size(44_100), 2048);
        assert_eq!(fft_size(48_000), 2048);
        assert_eq!(fft_size(96_000), 4096);
        assert_eq!(fft_size(192_000), 8192);
    }

    #[test]
    fn log_bands_span_20_hz_to_20_khz() {
        let (lo, _, _) = band_edges(BandScale::Log, 0, 48_000.0);
        let (_, hi, _) = band_edges(BandScale::Log, 199, 48_000.0);
        assert!((lo - 20.0).abs() < 1e-3);
        assert!((hi - 20_000.0).abs() < 1.0);
        // Ratio 1000^(1/200) between neighbours.
        let (a, b, _) = band_edges(BandScale::Log, 100, 48_000.0);
        assert!((b / a - 1.035_14).abs() < 1e-4);
        // The linear scale covers 0..fs/2 in 120 Hz steps at 48 kHz.
        assert_eq!(band_edges(BandScale::Linear, 1, 48_000.0).0, 120.0);
    }

    #[test]
    fn every_log_band_reads_some_bin_up_to_the_cap() {
        let bands = band_map(BandScale::Log, 48_000.0, 2048);
        assert!(bands.iter().all(|b| *b != Band::Off), "20 kHz < 0.95 * 24 kHz");
        let bands = band_map(BandScale::Log, 32_000.0, 2048);
        assert!(matches!(bands[199], Band::Off), "20 kHz is above 0.95 * 16 kHz");
    }

    #[test]
    fn a_1_khz_sine_lands_in_the_1_khz_band() {
        for scale in [BandScale::Log, BandScale::Linear] {
            let mut a = Analyzer::new(FS, 2, scale);
            // -30 dBFS keeps the output below the 127 clamp.
            let amp = 10f32.powf(-30.0 / 20.0);
            let snap = run(&mut a, 30, |i| {
                let x = amp * (std::f32::consts::TAU * 1000.0 * i as f32 / FS as f32).sin();
                (x, x)
            });
            let peak = argmax(a.band_db());
            let (_, _, centre) = band_edges(scale, peak, FS as f32);
            let tol = match scale {
                BandScale::Log => 0.036 * 1000.0,
                BandScale::Linear => 60.0,
            };
            assert!(
                (centre - 1000.0).abs() <= tol,
                "{scale:?}: band {peak} at {centre} Hz"
            );
            // The output peaks there too, and far bands stay dark.
            let out_peak = snap.freq.iter().copied().max().unwrap();
            assert_eq!(snap.freq[peak], out_peak, "{scale:?}");
            assert!(out_peak > 60, "{scale:?}: {out_peak}");
            let far = match scale {
                BandScale::Log => 30, // ~56 Hz
                BandScale::Linear => 150,
            };
            assert_eq!(snap.freq[far], 0, "{scale:?}");
        }
    }

    #[test]
    fn sines_across_the_range_land_in_their_log_band() {
        for f in [60.0f32, 250.0, 4000.0, 12_000.0] {
            let mut a = Analyzer::new(FS, 2, BandScale::Log);
            run(&mut a, 20, |i| {
                let x = 0.1 * (std::f32::consts::TAU * f * i as f32 / FS as f32).sin();
                (x, x)
            });
            let peak = argmax(a.band_db());
            let (lo, hi, _) = band_edges(BandScale::Log, peak, FS as f32);
            // Bin spacing is 23.4 Hz: low bands share bins, so allow one bin.
            assert!(
                lo - 23.5 <= f && f <= hi + 23.5,
                "{f} Hz -> band {peak} ({lo}..{hi})"
            );
        }
    }

    #[test]
    fn level_matches_the_sine_rms() {
        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let amp = 0.5; // -6.02 dBFS peak, RMS -9.03 dBFS
        let snap = run(&mut a, 120, |i| {
            let x = amp * (std::f32::consts::TAU * 1000.0 * i as f32 / FS as f32).sin();
            (x, x)
        });
        assert!((snap.level - -9.03).abs() < 0.1, "level {}", snap.level);
        assert!(
            (snap.rawlevel - amp / 2f32.sqrt()).abs() < 1e-3,
            "rawlevel {}",
            snap.rawlevel
        );
        // A full-scale sine reads -3 dB.
        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let snap = run(&mut a, 120, |i| {
            let x = (std::f32::consts::TAU * 440.0 * i as f32 / FS as f32).sin();
            (x, x)
        });
        assert!((snap.level - -3.01).abs() < 0.1, "level {}", snap.level);
    }

    #[test]
    fn silence_reads_floor() {
        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let snap = run(&mut a, 10, |_| (0.0, 0.0));
        assert_eq!(snap.level, SILENT_DB);
        assert_eq!(snap.rawlevel, 0.0);
        assert!(snap.freq.iter().all(|&v| v == 0));
        assert_eq!(snap.width, 0.0);
        assert_eq!(snap.density, 0.0);
        // An empty hop (no samples this tick) is silence too.
        let snap = a.process(&[], 1.0 / 60.0);
        assert_eq!(snap.level, SILENT_DB);
    }

    #[test]
    fn smoothing_attacks_fast_and_releases_slowly() {
        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let amp = 10f32.powf(-30.0 / 20.0);
        let tone = |i: usize| {
            let x = amp * (std::f32::consts::TAU * 1000.0 * i as f32 / FS as f32).sin();
            (x, x)
        };
        let full = run(&mut a, 30, tone);
        let peak = argmax(a.band_db());
        let top = full.freq[peak];
        // Release: one silent hop pushes zeros into a 2048-sample window that
        // still holds 1248 tone samples, so the raw band drops only a little;
        // after the window empties the smoothed band decays over ~150 ms.
        let hop = vec![0.0f32; 1600];
        let mut prev = top;
        let mut fell_to_zero_after = None;
        for t in 0..40 {
            let v = a.process(&hop, 1.0 / 60.0).freq[peak];
            assert!(v <= prev, "release is monotonic");
            prev = v;
            if v == 0 {
                fell_to_zero_after = Some(t + 1);
                break;
            }
        }
        let ticks = fell_to_zero_after.expect("released");
        assert!((4..=40).contains(&ticks), "released after {ticks} ticks");
        // Attack: back to (nearly) full within a few ticks of the tone returning.
        let mut b = Analyzer::new(FS, 2, BandScale::Log);
        let first = run(&mut b, 3, tone).freq[peak];
        assert!(first + 6 >= top, "attack {first} vs {top}");
    }

    #[test]
    fn width_reads_mono_uncorrelated_and_anti_phase() {
        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let mono = run(&mut a, 120, |i| {
            let x = 0.3 * (i as f32 * 0.05).sin();
            (x, x)
        });
        assert!(mono.width < 0.01, "mono {}", mono.width);

        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let (mut nl, mut nr) = (Noise(1), Noise(2));
        let wide = run(&mut a, 120, |_| (0.3 * nl.next(), 0.3 * nr.next()));
        assert!((wide.width - 0.5).abs() < 0.05, "uncorrelated {}", wide.width);

        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let anti = run(&mut a, 120, |i| {
            let x = 0.3 * (i as f32 * 0.05).sin();
            (x, -x)
        });
        assert!(anti.width > 0.95, "anti-phase {}", anti.width);

        // One channel: always 0.
        let mut a = Analyzer::new(FS, 1, BandScale::Log);
        let mut n = Noise(3);
        let mut snap = AudioSnapshot::default();
        for _ in 0..30 {
            let hop: Vec<f32> = (0..800).map(|_| 0.3 * n.next()).collect();
            snap = a.process(&hop, 1.0 / 60.0);
        }
        assert_eq!(snap.width, 0.0);
    }

    #[test]
    fn white_noise_is_dense_and_a_tone_is_not() {
        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let mut n = Noise(7);
        let noise = run(&mut a, 180, |_| {
            let x = 0.3 * n.next();
            (x, x)
        });
        assert!(noise.density >= 0.85, "white noise density {}", noise.density);

        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let tone = run(&mut a, 180, |i| {
            let x = 0.3 * (std::f32::consts::TAU * 1000.0 * i as f32 / FS as f32).sin();
            (x, x)
        });
        assert!(tone.density < 0.1, "tone density {}", tone.density);
    }

    #[test]
    fn white_noise_flatness_matches_the_calibration_constant() {
        // FLATNESS_WHITE is the raw flatness this test measures (rounded).
        let mut a = Analyzer::new(FS, 2, BandScale::Log);
        let mut n = Noise(11);
        let mut sum = 0.0;
        for t in 0..400 {
            let hop: Vec<f32> = (0..800)
                .flat_map(|_| {
                    let x = 0.3 * n.next();
                    [x, x]
                })
                .collect();
            a.process(&hop, 1.0 / 60.0);
            if t >= 100 {
                sum += a.density_target() * FLATNESS_WHITE;
            }
        }
        let raw = sum / 300.0;
        assert!(
            (raw - FLATNESS_WHITE).abs() < 0.04,
            "measured white flatness {raw}"
        );
    }

    #[test]
    fn freq_values_never_wrap_an_int8_array() {
        assert_eq!(freq_value(0.0), 127);
        assert_eq!(freq_value(-12.0), 127);
        assert_eq!(freq_value(-80.0), 0);
        assert_eq!(freq_value(-200.0), 0);
        assert_eq!(freq_value(-46.0), 64);
    }
}
