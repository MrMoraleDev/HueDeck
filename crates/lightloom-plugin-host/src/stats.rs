// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Fixed-memory latency histograms.
//!
//! Sessions run until stopped (hours, days), so latencies are binned instead
//! of stored: 1 µs resolution below 10 ms, 1 ms resolution up to 10 s, one
//! overflow bin above. About 160 KB per histogram, whatever the run length.
//! Percentiles use the nearest-rank rule on the bins; the mean and maximum
//! are exact.

use std::time::Duration;

const FINE_LIMIT_US: u64 = 10_000;
const COARSE_STEP_US: u64 = 1_000;
const COARSE_LIMIT_US: u64 = 10_000_000;

/// Percentiles of a latency sample, in milliseconds (nearest rank).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct LatencySummary {
    /// Samples.
    pub count: u64,
    /// Median.
    pub p50_ms: f64,
    /// 95th percentile.
    pub p95_ms: f64,
    /// 99th percentile.
    pub p99_ms: f64,
    /// Maximum.
    pub max_ms: f64,
    /// Mean.
    pub mean_ms: f64,
}

/// Binned latencies (see the module docs).
#[derive(Clone, Debug)]
pub struct LatencyHistogram {
    fine: Vec<u64>,
    coarse: Vec<u64>,
    overflow: u64,
    count: u64,
    sum_us: u128,
    max_us: u64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    /// Empty histogram.
    pub fn new() -> Self {
        Self {
            fine: vec![0; FINE_LIMIT_US as usize],
            coarse: vec![0; ((COARSE_LIMIT_US - FINE_LIMIT_US) / COARSE_STEP_US) as usize],
            overflow: 0,
            count: 0,
            sum_us: 0,
            max_us: 0,
        }
    }

    /// Record one latency.
    pub fn record(&mut self, d: Duration) {
        self.record_micros(d.as_micros().min(u64::MAX as u128) as u64);
    }

    /// Record one latency given in microseconds.
    pub fn record_micros(&mut self, us: u64) {
        if us < FINE_LIMIT_US {
            self.fine[us as usize] += 1;
        } else if us < COARSE_LIMIT_US {
            self.coarse[((us - FINE_LIMIT_US) / COARSE_STEP_US) as usize] += 1;
        } else {
            self.overflow += 1;
        }
        self.count += 1;
        self.sum_us += us as u128;
        self.max_us = self.max_us.max(us);
    }

    /// Number of samples.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Lower bound, in µs, of the bin holding the sample of 1-based `rank`.
    fn value_at_rank(&self, rank: u64) -> u64 {
        let mut seen = 0u64;
        for (us, &n) in self.fine.iter().enumerate() {
            seen += n;
            if seen >= rank {
                return us as u64;
            }
        }
        for (i, &n) in self.coarse.iter().enumerate() {
            seen += n;
            if seen >= rank {
                return FINE_LIMIT_US + i as u64 * COARSE_STEP_US;
            }
        }
        self.max_us
    }

    /// Nearest-rank percentiles, mean and maximum.
    pub fn summary(&self) -> LatencySummary {
        if self.count == 0 {
            return LatencySummary::default();
        }
        let pct = |p: f64| {
            let rank = ((p / 100.0) * self.count as f64).ceil().max(1.0) as u64;
            self.value_at_rank(rank.min(self.count)) as f64 / 1000.0
        };
        LatencySummary {
            count: self.count,
            p50_ms: pct(50.0),
            p95_ms: pct(95.0),
            p99_ms: pct(99.0),
            max_ms: self.max_us as f64 / 1000.0,
            mean_ms: self.sum_us as f64 / self.count as f64 / 1000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_on_fine_bins_is_exact() {
        let mut h = LatencyHistogram::new();
        for v in 1..=100u64 {
            h.record_micros(v * 50); // 0.05 .. 5 ms
        }
        let s = h.summary();
        assert_eq!(s.count, 100);
        assert_eq!((s.p50_ms, s.p95_ms, s.p99_ms, s.max_ms), (2.5, 4.75, 4.95, 5.0));
        assert!((s.mean_ms - 2.525).abs() < 1e-9);
    }

    #[test]
    fn coarse_and_overflow_bins_keep_max_and_mean_exact() {
        let mut h = LatencyHistogram::new();
        h.record(Duration::from_micros(12_345));
        h.record(Duration::from_secs(20));
        let s = h.summary();
        assert_eq!(s.p50_ms, 12.0, "coarse bins have 1 ms resolution");
        assert_eq!(s.max_ms, 20_000.0);
        assert!((s.mean_ms - (12.345 + 20_000.0) / 2.0).abs() < 1e-9);
        assert_eq!(LatencyHistogram::new().summary(), LatencySummary::default());
    }

    #[test]
    fn memory_does_not_grow_with_samples() {
        let mut h = LatencyHistogram::new();
        let before = (h.fine.len(), h.coarse.len());
        for i in 0..200_000u64 {
            h.record_micros(i % 20_000);
        }
        assert_eq!((h.fine.len(), h.coarse.len()), before);
        assert_eq!(h.count(), 200_000);
    }
}
