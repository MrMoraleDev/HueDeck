// SPDX-License-Identifier: Apache-2.0 OR MIT
//! What happens to every painted frame, independent of where it comes from (CEF `OnPaint` in
//! the binary, a fake source in tests): publish into the shm ring, record the paint timestamp
//! and the publish cost, keep snapshot copies of selected frames.

use crate::ring::{RingError, RingWriter};
use crate::snapshot::{png_path_for, Snapshot, SnapshotInfo};
use crate::stats::{interval_stats, intervals, summarize, DurationSummary, IntervalStats};
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, Instant};

/// Sink configuration.
#[derive(Clone, Debug)]
pub struct SinkConfig {
    /// Frames to record; the sink reports [`PaintOutcome::Done`] on this one.
    pub frames: u64,
    /// Target fps (for the interval statistics).
    pub fps: u32,
    /// 1-based frame numbers to keep a copy of.
    pub snapshot_frames: Vec<u64>,
    /// Include every paint interval in the report (`intervals_ms`).
    pub raw_intervals: bool,
}

/// Result of [`FrameSink::on_paint`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintOutcome {
    /// Frame published, more wanted.
    Continue,
    /// Frame published and it was the last one requested: stop the source now.
    Done,
    /// Frame dropped: the sink is already done, or the frame did not fit the ring.
    Ignored,
}

/// Statistics of a finished (or aborted) run.
#[derive(Clone, Debug, Serialize)]
pub struct SinkReport {
    /// Frames requested.
    pub frames_requested: u64,
    /// Frames published into the ring.
    pub frames: u64,
    /// Target fps.
    pub fps_target: u32,
    /// First → last published paint.
    pub elapsed_ms: f64,
    /// (frames - 1) / elapsed.
    pub effective_fps: f64,
    /// Intervals between consecutive published paints.
    pub paint_interval: Option<IntervalStats>,
    /// Time spent copying each frame into the ring.
    pub publish_cost: Option<DurationSummary>,
    /// Paints whose size differed from the ring (not published).
    pub size_mismatches: u64,
    /// Paints that arrived after the last requested frame.
    pub paints_after_done: u64,
    /// Ring `seq` at the end (== frames).
    pub shm_seq: u64,
    /// Snapshot metrics, in frame order.
    pub snapshots: Vec<SnapshotInfo>,
    /// Every paint interval in ms (only with [`SinkConfig::raw_intervals`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intervals_ms: Option<Vec<f64>>,
}

/// Frame consumer: shm ring + paint statistics + snapshots.
pub struct FrameSink {
    ring: RingWriter,
    cfg: SinkConfig,
    stamps: Vec<Instant>,
    publish_cost: Vec<Duration>,
    snapshots: Vec<Snapshot>,
    size_mismatches: u64,
    paints_after_done: u64,
    last_error: Option<String>,
}

impl FrameSink {
    /// New sink writing into `ring`.
    pub fn new(ring: RingWriter, cfg: SinkConfig) -> Self {
        let cap = usize::try_from(cfg.frames).unwrap_or(0).min(1 << 20);
        Self {
            ring,
            cfg,
            stamps: Vec::with_capacity(cap),
            publish_cost: Vec::with_capacity(cap),
            snapshots: Vec::new(),
            size_mismatches: 0,
            paints_after_done: 0,
            last_error: None,
        }
    }

    /// Frames published so far.
    pub fn frames(&self) -> u64 {
        self.stamps.len() as u64
    }

    /// Whether all requested frames were published.
    pub fn is_done(&self) -> bool {
        self.frames() >= self.cfg.frames
    }

    /// Timestamp of the first published frame.
    pub fn first_paint(&self) -> Option<Instant> {
        self.stamps.first().copied()
    }

    /// Last ring error (size mismatch etc.), for diagnostics.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// The ring (e.g. to flush it at the end).
    pub fn ring(&self) -> &RingWriter {
        &self.ring
    }

    /// Consume one painted frame (`width*height*4` BGRA bytes) painted at `at`.
    pub fn on_paint(&mut self, bgra: &[u8], width: u32, height: u32, at: Instant) -> PaintOutcome {
        if self.is_done() {
            self.paints_after_done += 1;
            return PaintOutcome::Ignored;
        }
        let started = Instant::now();
        match self.ring.publish(bgra, width, height) {
            Ok(_) => {}
            Err(err) => {
                if matches!(
                    err,
                    RingError::SizeMismatch { .. } | RingError::BufferLength { .. }
                ) {
                    self.size_mismatches += 1;
                }
                self.last_error = Some(err.to_string());
                return PaintOutcome::Ignored;
            }
        }
        self.publish_cost.push(started.elapsed());
        self.stamps.push(at);
        let frame = self.frames();
        if self.cfg.snapshot_frames.contains(&frame) {
            self.snapshots.push(Snapshot {
                frame,
                width,
                height,
                bgra: bgra.to_vec(),
            });
        }
        if self.is_done() {
            PaintOutcome::Done
        } else {
            PaintOutcome::Continue
        }
    }

    /// Snapshots taken so far.
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// Write PNGs (when `png_base` is set) and build the report. PNG errors are returned
    /// separately so a failed write does not hide the statistics.
    pub fn finish(&self, png_base: Option<&Path>) -> (SinkReport, Vec<String>) {
        let mut errors = Vec::new();
        let only_one = self.cfg.snapshot_frames.len() == 1;
        let mut infos = Vec::with_capacity(self.snapshots.len());
        for snap in &self.snapshots {
            let png = png_base.map(|base| png_path_for(base, snap.frame, only_one));
            if let Some(path) = &png {
                if let Err(err) = snap.write_png(path) {
                    errors.push(format!("png {}: {err}", path.display()));
                }
            }
            infos.push(snap.info(png.as_deref()));
        }
        let elapsed = match (self.stamps.first(), self.stamps.last()) {
            (Some(a), Some(b)) => b.saturating_duration_since(*a),
            _ => Duration::ZERO,
        };
        let elapsed_ms = (elapsed.as_secs_f64() * 1_000_000.0).round() / 1000.0;
        let effective_fps = if self.stamps.len() > 1 && elapsed > Duration::ZERO {
            ((self.stamps.len() - 1) as f64 / elapsed.as_secs_f64() * 1000.0).round() / 1000.0
        } else {
            0.0
        };
        let report = SinkReport {
            frames_requested: self.cfg.frames,
            frames: self.frames(),
            fps_target: self.cfg.fps,
            elapsed_ms,
            effective_fps,
            paint_interval: interval_stats(&self.stamps, self.cfg.fps),
            publish_cost: summarize(&self.publish_cost),
            size_mismatches: self.size_mismatches,
            paints_after_done: self.paints_after_done,
            shm_seq: self.ring.seq(),
            snapshots: infos,
            intervals_ms: self.cfg.raw_intervals.then(|| {
                intervals(&self.stamps)
                    .iter()
                    .map(|d| (d.as_secs_f64() * 1_000_000.0).round() / 1000.0)
                    .collect()
            }),
        };
        (report, errors)
    }
}
