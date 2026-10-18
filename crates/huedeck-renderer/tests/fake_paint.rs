// SPDX-License-Identifier: Apache-2.0 OR MIT
//! SPIKE A plumbing without CEF: a fake `OnPaint` source drives the [`FrameSink`], and the shm
//! ring is checked through a second (read-only) mapping of the same file, the way `huedeckd`
//! will read it. Runs on any host (no Chromium, no GPU, no hardware).

use huedeck_protocol::shm::{FrameHeader, CANVAS_HEIGHT, CANVAS_WIDTH, HEADER_BYTES, SLOT_COUNT};
use huedeck_renderer::ring::{RingError, RingReader, RingWriter};
use huedeck_renderer::sink::{FrameSink, PaintOutcome, SinkConfig};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const W: u32 = CANVAS_WIDTH;
const H: u32 = CANVAS_HEIGHT;

/// Unique temp file, removed on drop.
struct TempFile(PathBuf);

impl TempFile {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "huedeck-renderer-{tag}-{}-{nanos}.shm",
            std::process::id()
        )))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Deterministic animated BGRA frame: horizontal gradient shifted by `tick`, a moving bar.
fn fake_frame(tick: u32) -> Vec<u8> {
    let mut px = Vec::with_capacity((W * H * 4) as usize);
    let bar = (tick * 7) % W;
    for y in 0..H {
        for x in 0..W {
            if x >= bar && x < bar + 10 {
                px.extend_from_slice(&[255, 255, 255, 255]);
            } else {
                let v = ((x + tick * 3) % 256) as u8;
                px.extend_from_slice(&[v, (y % 256) as u8, 255 - v, 255]);
            }
        }
    }
    px
}

#[test]
fn header_layout_on_disk() {
    let tmp = TempFile::new("layout");
    let ring = RingWriter::create_file(&tmp.0, W, H).unwrap();
    let len = std::fs::metadata(&tmp.0).unwrap().len() as usize;
    assert_eq!(len, HEADER_BYTES + SLOT_COUNT * (W * H * 4) as usize);
    let bytes = std::fs::read(&tmp.0).unwrap();
    assert_eq!(&bytes[0..4], b"HDFR");
    let h = FrameHeader::from_bytes(&bytes).unwrap();
    assert_eq!((h.width, h.height, h.seq), (W, H, 0));
    assert_eq!(h.slot_generations, [0, 0, 0]);
    drop(ring);
}

#[test]
fn fake_on_paint_source_fills_ring_and_stats() {
    let tmp = TempFile::new("sink");
    let ring = RingWriter::create_file(&tmp.0, W, H).unwrap();
    let reader = RingReader::open_file(&tmp.0).unwrap();
    let mut out = vec![0u8; (W * H * 4) as usize];
    assert_eq!(reader.read_latest(&mut out).unwrap(), None);

    let frames = 120u64;
    let mut sink = FrameSink::new(
        ring,
        SinkConfig {
            frames,
            fps: 30,
            snapshot_frames: vec![10, 100],
            raw_intervals: true,
            serve: false,
        },
    );
    // 30 fps with +-2 ms jitter and one 90 ms hitch before frame 61.
    let t0 = Instant::now();
    let mut t = t0;
    let mut outcome = PaintOutcome::Continue;
    for tick in 1..=frames as u32 {
        let jitter = if tick % 2 == 0 { 2_000 } else { -2_000i64 };
        let step = if tick == 61 { 90_000 } else { 33_333 + jitter };
        if tick > 1 {
            t += Duration::from_micros(step as u64);
        }
        let frame = fake_frame(tick);
        outcome = sink.on_paint(&frame, W, H, t);
        // every published frame is immediately the latest one a reader sees
        assert_eq!(reader.read_latest(&mut out).unwrap(), Some(u64::from(tick)));
        assert!(out == frame, "reader copy differs at tick {tick}");
        if tick < frames as u32 {
            assert_eq!(outcome, PaintOutcome::Continue);
        }
    }
    assert_eq!(outcome, PaintOutcome::Done);
    // late paints are ignored, ring untouched
    assert_eq!(sink.on_paint(&fake_frame(999), W, H, t), PaintOutcome::Ignored);
    let header = reader.header().unwrap();
    assert_eq!(header.seq, frames);
    // writer protocol: each slot was written seq/3 times, 2 generation steps each
    assert_eq!(header.slot_generations, [80, 80, 80]);
    assert_eq!(header.latest_slot(), Some(((frames - 1) % 3) as usize));

    let png = std::env::temp_dir().join(format!("huedeck-renderer-test-{}.png", std::process::id()));
    let (report, errors) = sink.finish(Some(&png));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(report.frames, frames);
    assert_eq!(report.shm_seq, frames);
    assert_eq!(report.paints_after_done, 1);
    let iv = report.paint_interval.expect("interval stats");
    assert_eq!(iv.summary.count, frames as usize - 1);
    assert!((iv.summary.p50_ms - 33.333).abs() < 2.1, "{iv:?}");
    assert!(iv.summary.p99_ms < 40.0, "{iv:?}");
    assert!(iv.summary.max_ms > 89.0, "{iv:?}");
    assert_eq!(iv.dropped, 1, "{iv:?}");
    assert!(report.publish_cost.is_some());
    let raw = report.intervals_ms.as_ref().expect("raw intervals requested");
    assert_eq!(raw.len(), frames as usize - 1);
    assert_eq!(raw[59], 90.0);

    // snapshots: not black, and frame 10 != frame 100
    assert_eq!(report.snapshots.len(), 2);
    let (s10, s100) = (&report.snapshots[0], &report.snapshots[1]);
    assert_eq!((s10.frame, s100.frame), (10, 100));
    assert!(s10.nonblack_fraction > 0.9 && s100.nonblack_fraction > 0.9);
    assert_ne!(s10.sha256, s100.sha256);
    for s in [s10, s100] {
        let path = PathBuf::from(s.png.as_ref().expect("png written"));
        let img = image::open(&path).expect("png decodes").to_rgb8();
        assert_eq!(img.dimensions(), (W, H));
        // BGRA -> RGB conversion: pixel (0,0) of tick 10 has v=30 → B=30,G=0,R=225
        if s.frame == 10 {
            assert_eq!(img.get_pixel(0, 0).0, [225, 0, 30]);
        }
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn size_mismatch_is_rejected_not_published() {
    let mut sink = FrameSink::new(
        RingWriter::anonymous(W, H).unwrap(),
        SinkConfig {
            frames: 2,
            fps: 30,
            snapshot_frames: vec![],
            raw_intervals: false,
            serve: false,
        },
    );
    let now = Instant::now();
    // a 640x400 buffer (device scale factor 2) must not be published into a 320x200 ring
    let big = vec![0u8; 640 * 400 * 4];
    assert_eq!(sink.on_paint(&big, 640, 400, now), PaintOutcome::Ignored);
    assert_eq!(sink.on_paint(&big[..10], W, H, now), PaintOutcome::Ignored);
    assert_eq!(sink.ring().seq(), 0);
    assert_eq!(sink.on_paint(&fake_frame(1), W, H, now), PaintOutcome::Continue);
    assert_eq!(sink.on_paint(&fake_frame(2), W, H, now), PaintOutcome::Done);
    let (report, _) = sink.finish(None);
    assert_eq!(report.size_mismatches, 2);
    assert_eq!(report.frames, 2);
    assert!(matches!(
        RingWriter::anonymous(0, 10),
        Err(RingError::InvalidSize(0, 10))
    ));
}

/// Writer thread publishes uniform frames (every byte = seq & 0xff) as fast as it can while a
/// reader thread on a separate read-only mapping checks it never sees a torn frame, and that the
/// `seq` it gets back labels the pixels it copied (also when the writer laps the reader).
#[test]
fn concurrent_reader_never_sees_torn_frames() {
    let tmp = TempFile::new("seqlock");
    let mut writer = RingWriter::create_file(&tmp.0, W, H).unwrap();
    let reader = RingReader::open_file(&tmp.0).unwrap();
    let stop = Arc::new(AtomicBool::new(false));

    let stop_r = stop.clone();
    let reader_thread = std::thread::spawn(move || {
        let mut out = vec![0u8; (W * H * 4) as usize];
        let (mut reads, mut contended, mut last_seq) = (0u64, 0u64, 0u64);
        while !stop_r.load(Ordering::Relaxed) {
            match reader.read_latest(&mut out) {
                Ok(Some(seq)) => {
                    let first = out[0];
                    assert!(out.iter().all(|&b| b == first), "torn frame near seq {seq}");
                    assert_eq!(
                        first,
                        (seq & 0xff) as u8,
                        "seq {seq} labels another frame's pixels"
                    );
                    assert!(seq >= last_seq, "seq went backwards");
                    last_seq = seq;
                    reads += 1;
                }
                Ok(None) => {}
                Err(RingError::Contended) => contended += 1,
                Err(e) => panic!("{e}"),
            }
        }
        (reads, contended)
    });

    let mut frame = vec![0u8; (W * H * 4) as usize];
    for seq in 1..=3000u64 {
        frame.fill((seq & 0xff) as u8);
        assert_eq!(writer.publish(&frame, W, H).unwrap(), seq);
    }
    // give the reader a moment on the final frame, then stop
    std::thread::sleep(Duration::from_millis(20));
    stop.store(true, Ordering::Relaxed);
    let (reads, _contended) = reader_thread.join().unwrap();
    assert!(reads > 0, "reader never completed a read");
    assert_eq!(writer.seq(), 3000);
    writer.flush().unwrap();
}
