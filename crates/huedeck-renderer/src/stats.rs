// SPDX-License-Identifier: GPL-3.0-or-later
//! Paint-interval statistics for the frame pacing acceptance check.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Summary of a series of durations, in milliseconds.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DurationSummary {
    /// Number of samples.
    pub count: usize,
    /// Smallest sample.
    pub min_ms: f64,
    /// Arithmetic mean.
    pub mean_ms: f64,
    /// 50th percentile (nearest rank).
    pub p50_ms: f64,
    /// 95th percentile (nearest rank).
    pub p95_ms: f64,
    /// 99th percentile (nearest rank).
    pub p99_ms: f64,
    /// Largest sample.
    pub max_ms: f64,
}

/// Interval statistics between consecutive paints.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IntervalStats {
    /// Summary of all intervals.
    #[serde(flatten)]
    pub summary: DurationSummary,
    /// Target period (1000 / fps).
    pub period_ms: f64,
    /// Intervals longer than `drop_threshold_ms` (= 2 x period): at least one frame missed.
    pub dropped: usize,
    /// Threshold used for `dropped`.
    pub drop_threshold_ms: f64,
}

/// Nearest-rank percentile of an ascending slice (`p` in 0..=100). `NaN` for an empty slice.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Summarise millisecond samples. `None` when empty.
pub fn summarize_ms(samples: &[f64]) -> Option<DurationSummary> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    Some(DurationSummary {
        count: sorted.len(),
        min_ms: round3(sorted[0]),
        mean_ms: round3(mean),
        p50_ms: round3(percentile(&sorted, 50.0)),
        p95_ms: round3(percentile(&sorted, 95.0)),
        p99_ms: round3(percentile(&sorted, 99.0)),
        max_ms: round3(sorted[sorted.len() - 1]),
    })
}

/// Summarise durations. `None` when empty.
pub fn summarize(durations: &[Duration]) -> Option<DurationSummary> {
    let ms: Vec<f64> = durations.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    summarize_ms(&ms)
}

/// Intervals between consecutive timestamps (`stamps` must be non-decreasing).
pub fn intervals(stamps: &[Instant]) -> Vec<Duration> {
    stamps
        .windows(2)
        .map(|w| w[1].saturating_duration_since(w[0]))
        .collect()
}

/// Interval statistics for paints targeting `fps`. `None` with fewer than two stamps.
pub fn interval_stats(stamps: &[Instant], fps: u32) -> Option<IntervalStats> {
    let period_ms = 1000.0 / f64::from(fps.max(1));
    let iv = intervals(stamps);
    let summary = summarize(&iv)?;
    let threshold = 2.0 * period_ms;
    let dropped = iv.iter().filter(|d| d.as_secs_f64() * 1000.0 > threshold).count();
    Some(IntervalStats {
        summary,
        period_ms: round3(period_ms),
        dropped,
        drop_threshold_ms: round3(threshold),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 50.0), 50.0);
        assert_eq!(percentile(&v, 99.0), 99.0);
        assert_eq!(percentile(&v, 100.0), 100.0);
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&[7.0], 95.0), 7.0);
        assert!(percentile(&[], 50.0).is_nan());
    }

    #[test]
    fn steady_30fps_with_one_hitch() {
        let t0 = Instant::now();
        let mut stamps: Vec<Instant> = (0..300).map(|i| t0 + Duration::from_micros(i * 33_333)).collect();
        // one 100 ms hitch: shift the tail
        for s in stamps.iter_mut().skip(150) {
            *s += Duration::from_millis(67);
        }
        let st = interval_stats(&stamps, 30).unwrap();
        assert_eq!(st.summary.count, 299);
        assert!((st.summary.p50_ms - 33.333).abs() < 0.01, "{st:?}");
        assert!((st.summary.p99_ms - 33.333).abs() < 0.01, "{st:?}");
        assert!(st.summary.max_ms > 99.0);
        assert_eq!(st.dropped, 1);
        assert_eq!(st.period_ms, 33.333);
    }
}
