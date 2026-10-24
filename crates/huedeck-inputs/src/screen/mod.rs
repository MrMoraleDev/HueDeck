// SPDX-License-Identifier: GPL-3.0-or-later
//! `engine.zone` and `engine.vision`: a capture of one display, reduced per
//! frame to the 160x100 thumbnail + 28x20 HSL grid and to the active effect's
//! meters, handed to a sink (the daemon forwards it to the renderer). See
//! docs/screen.md.
//!
//! * [`sample`]: thumbnail and zones (CPU).
//! * [`meters`]: area / linear / colormean meters.
//! * [`ScreenInput`]: a thread that waits for desktop images from a
//!   [`FrameSource`] at most `fps` times per second, reduces them and calls
//!   the sink. A source that fails is reopened with a backoff of 250 ms
//!   doubling to 2 s; after 2 s without a frame the zones go black. Whatever
//!   the state, the last snapshot is pushed again once a second while nothing
//!   new arrives (a restarted renderer gets it).
//! * On Windows, [`desktop`] opens DXGI Desktop Duplication of a display
//!   (`dxgi.rs`). Frames stay in memory in this process; nothing is stored.

#[cfg(windows)]
mod dxgi;
pub mod meters;
pub mod sample;

use crate::ZoneSnapshot;
pub use meters::{HslWindow, Meter, MeterKind, Rect, RectOverride, VisionValue};
pub use sample::{Frame, Rotation};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A display that can be captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayInfo {
    /// Device name (`\\.\DISPLAY1`), what `[screen] display` names.
    pub name: String,
    /// Width in pixels as shown.
    pub width: u32,
    /// Height in pixels as shown.
    pub height: u32,
    /// The primary display (desktop origin).
    pub primary: bool,
    /// Graphics adapter it is connected to.
    pub adapter: String,
}

/// Whether a configured display name means `device` (`\\.\DISPLAY2`,
/// `DISPLAY2` and `display2` all name the same one).
pub fn display_matches(wanted: &str, device: &str) -> bool {
    let strip = |s: &str| s.trim().trim_start_matches(r"\\.\").to_ascii_uppercase();
    strip(wanted) == strip(device)
}

/// Desktop images from somewhere (a duplication, a test pattern).
pub trait FrameSource: Send {
    /// What is captured, for the log.
    fn describe(&self) -> String;
    /// Wait up to `timeout` for a desktop image newer than the last one and
    /// call `f` with it: `Ok(true)`. `Ok(false)`: nothing changed. `Err`: the
    /// source is gone and must be reopened.
    fn capture(&mut self, timeout: Duration, f: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, String>;
}

/// Opens a source (called again after it fails).
pub type OpenSource = Box<dyn FnMut() -> Result<Box<dyn FrameSource>, String> + Send>;

/// One push: the zones (when wanted) and every meter's value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenSnapshot {
    /// `engine.zone`, when the effect reads it.
    pub zone: Option<ZoneSnapshot>,
    /// `engine.vision.<name>` values, in meter order.
    pub vision: Vec<(String, VisionValue)>,
}

/// Receives each snapshot.
pub type Sink = Box<dyn FnMut(&ScreenSnapshot) + Send>;

/// What to compute from each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenSettings {
    /// Frames per second, clamped to 10..=60.
    pub fps: u32,
    /// Compute `engine.zone`.
    pub zone: bool,
    /// Meters to evaluate.
    pub meters: Vec<Meter>,
}

/// Wait between failed opens.
const RETRY_OPEN: Duration = Duration::from_secs(2);
/// First wait after a capture error (doubles up to [`RETRY_OPEN`]).
const FIRST_BACKOFF: Duration = Duration::from_millis(250);
/// Without a frame this long (capture failing), the zones go black.
const BLANK_AFTER: Duration = Duration::from_secs(2);
/// Unchanged snapshots are pushed again this often.
const KEEPALIVE: Duration = Duration::from_secs(1);

/// The displays [`desktop`] can capture (Windows; empty elsewhere).
pub fn displays() -> Vec<DisplayInfo> {
    #[cfg(windows)]
    {
        dxgi::displays()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// An [`OpenSource`] duplicating `display` (a name as [`displays`] lists
/// it; `None` or empty: the primary display).
pub fn desktop(display: Option<String>) -> OpenSource {
    #[cfg(windows)]
    {
        Box::new(move || {
            dxgi::Duplication::open(display.as_deref()).map(|s| Box::new(s) as Box<dyn FrameSource>)
        })
    }
    #[cfg(not(windows))]
    {
        let _ = display;
        Box::new(|| Err("screen capture is not available on this platform yet".into()))
    }
}

/// A running screen input (see the module docs); stops when dropped.
pub struct ScreenInput {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ScreenInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScreenInput")
            .field("running", &self.thread.is_some())
            .finish()
    }
}

impl ScreenInput {
    /// Start reducing `open`'s frames per `settings` and calling `sink`.
    pub fn start(settings: ScreenSettings, open: OpenSource, sink: Sink) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("huedeck-screen".into())
            .spawn(move || run(&flag, &settings, open, sink))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    /// Stop and join the thread (the capture closes with it).
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ScreenInput {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Reduce one frame into `snap`.
pub fn reduce(frame: &Frame<'_>, settings: &ScreenSettings, snap: &mut ScreenSnapshot) {
    if let Some(zone) = snap.zone.as_mut() {
        sample::thumbnail(frame, &mut zone.imagedata);
        sample::zones(zone);
    }
    for (slot, m) in snap.vision.iter_mut().zip(&settings.meters) {
        slot.1 = m.evaluate(frame);
    }
}

/// The snapshot shown while nothing is captured: black zones, meters at 0.
fn blank(settings: &ScreenSettings) -> ScreenSnapshot {
    ScreenSnapshot {
        zone: settings.zone.then(|| {
            let mut z = ZoneSnapshot::default();
            for px in z.imagedata.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            z
        }),
        vision: settings
            .meters
            .iter()
            .map(|m| {
                let v = match m.kind {
                    MeterKind::ColorMean => VisionValue::Triple([0.0; 3]),
                    _ => VisionValue::Number(0.0),
                };
                (m.name.clone(), v)
            })
            .collect(),
    }
}

fn run(stop: &AtomicBool, settings: &ScreenSettings, mut open: OpenSource, mut sink: Sink) {
    let period = Duration::from_secs_f64(1.0 / f64::from(settings.fps.clamp(10, 60)));
    let mut snap = blank(settings);
    let mut source: Option<Box<dyn FrameSource>> = None;
    let mut open_at = Instant::now();
    let mut open_failed = false;
    let mut backoff = FIRST_BACKOFF;
    let mut last_frame = Instant::now();
    let mut last_push: Option<Instant> = None;
    let mut blanked = false;
    while !stop.load(Ordering::Acquire) {
        let tick = Instant::now();
        if source.is_none() && tick >= open_at {
            match open() {
                Ok(s) => {
                    tracing::info!("screen: capturing {}", s.describe());
                    source = Some(s);
                    open_failed = false;
                }
                Err(e) => {
                    if !open_failed {
                        tracing::warn!("screen: cannot start the capture: {e}");
                    }
                    open_failed = true;
                    open_at = tick + RETRY_OPEN;
                }
            }
        }
        let mut fresh = false;
        if let Some(s) = source.as_mut() {
            match s.capture(period, &mut |f| reduce(f, settings, &mut snap)) {
                Ok(true) => fresh = true,
                Ok(false) => {}
                Err(e) => {
                    tracing::info!("screen: capture stopped ({e}); reopening in {backoff:?}");
                    source = None;
                    open_at = Instant::now() + backoff;
                    backoff = (backoff * 2).min(RETRY_OPEN);
                }
            }
        }
        let now = Instant::now();
        if fresh {
            last_frame = now;
            backoff = FIRST_BACKOFF;
            blanked = false;
            sink(&snap);
            last_push = Some(now);
        } else if source.is_none() && !blanked && now.duration_since(last_frame) >= BLANK_AFTER {
            snap = blank(settings);
            sink(&snap);
            blanked = true;
            last_push = Some(now);
        } else if last_push.is_some_and(|t| now.duration_since(t) >= KEEPALIVE) {
            sink(&snap);
            last_push = Some(now);
        }
        // At most `fps` frames: a busy desktop returns new images at once.
        let next = tick + period;
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sample::tests::surface;
    use super::*;
    use std::sync::mpsc;

    /// A source producing a solid colour that changes on every frame.
    struct Solid {
        n: u8,
        data: Vec<u8>,
        fail_after: Option<u8>,
    }

    impl FrameSource for Solid {
        fn describe(&self) -> String {
            "a test pattern".into()
        }
        fn capture(&mut self, _: Duration, f: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, String> {
            if self.fail_after.is_some_and(|n| self.n >= n) {
                return Err("gone".into());
            }
            self.n = self.n.wrapping_add(1);
            self.data = surface(64, 40, |_, _| [self.n, 0, 0]);
            f(&Frame::new(&self.data, 64, 40, 256, Rotation::Identity).unwrap());
            Ok(true)
        }
    }

    fn settings(meters: Vec<Meter>) -> ScreenSettings {
        ScreenSettings {
            fps: 60,
            zone: true,
            meters,
        }
    }

    fn collect(settings: ScreenSettings, open: OpenSource, n: usize, wait: Duration) -> Vec<ScreenSnapshot> {
        let (tx, rx) = mpsc::channel();
        let mut input = ScreenInput::start(
            settings,
            open,
            Box::new(move |s| {
                let _ = tx.send(s.clone());
            }),
        )
        .unwrap();
        let snaps = (0..n).map_while(|_| rx.recv_timeout(wait).ok()).collect();
        input.stop();
        snaps
    }

    #[test]
    fn pushes_zones_and_meters_of_a_fake_source() {
        let m = Meter {
            name: "redness".into(),
            kind: MeterKind::Area,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: Some(1.0),
            },
            color: HslWindow {
                h: Some((350.0, 10.0)),
                ..HslWindow::default()
            },
            overrides: Vec::new(),
        };
        let open: OpenSource = Box::new(|| {
            Ok(Box::new(Solid {
                n: 100,
                data: Vec::new(),
                fail_after: None,
            }) as Box<dyn FrameSource>)
        });
        let snaps = collect(settings(vec![m]), open, 5, Duration::from_secs(5));
        assert_eq!(snaps.len(), 5);
        let last = snaps.last().unwrap();
        let z = last.zone.as_ref().unwrap();
        assert_eq!(&z.imagedata[..4], &[105, 0, 0, 255]);
        assert!(z.hue.iter().all(|&h| h == 0) && z.saturation.iter().all(|&s| s == 100));
        assert_eq!(last.vision, vec![("redness".into(), VisionValue::Number(1.0))]);
    }

    #[test]
    fn reopens_a_source_that_fails() {
        let opens = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&opens);
        let open: OpenSource = Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Solid {
                n: 0,
                data: Vec::new(),
                fail_after: Some(3),
            }) as Box<dyn FrameSource>)
        });
        // 3 frames, error, 250 ms, 3 frames, error, 500 ms, 3 frames.
        let snaps = collect(settings(Vec::new()), open, 9, Duration::from_secs(5));
        assert_eq!(snaps.len(), 9);
        assert!(opens.load(Ordering::SeqCst) >= 3);
    }

    #[test]
    fn a_source_that_cannot_open_goes_black() {
        let s = ScreenSettings {
            meters: vec![Meter {
                name: "hp".into(),
                kind: MeterKind::Linear,
                rect: Rect::default(),
                color: HslWindow::default(),
                overrides: Vec::new(),
            }],
            ..settings(Vec::new())
        };
        let started = Instant::now();
        let snaps = collect(
            s,
            Box::new(|| Err("no display".into())),
            2,
            Duration::from_secs(4),
        );
        // Black after 2 s, then again each second for a restarted renderer.
        assert_eq!(snaps.len(), 2);
        assert!(started.elapsed() >= BLANK_AFTER + KEEPALIVE - Duration::from_millis(50));
        for snap in &snaps {
            let z = snap.zone.as_ref().unwrap();
            assert!(z.lightness.iter().all(|&l| l == 0));
            assert_eq!(&z.imagedata[..4], &[0, 0, 0, 255]);
            assert_eq!(snap.vision, vec![("hp".into(), VisionValue::Number(0.0))]);
        }
    }

    #[test]
    fn an_unchanged_desktop_is_pushed_again_each_second() {
        struct Still(bool);
        impl FrameSource for Still {
            fn describe(&self) -> String {
                "still".into()
            }
            fn capture(&mut self, t: Duration, f: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, String> {
                if std::mem::replace(&mut self.0, true) {
                    thread::sleep(t);
                    return Ok(false);
                }
                let data = surface(4, 4, |_, _| [0, 0, 255]);
                f(&Frame::new(&data, 4, 4, 16, Rotation::Identity).unwrap());
                Ok(true)
            }
        }
        let started = Instant::now();
        let snaps = collect(
            ScreenSettings {
                zone: false,
                ..settings(Vec::new())
            },
            Box::new(|| Ok(Box::new(Still(false)) as Box<dyn FrameSource>)),
            2,
            Duration::from_secs(3),
        );
        assert_eq!(snaps.len(), 2);
        assert!(started.elapsed() >= Duration::from_millis(900));
        assert!(snaps[1].zone.is_none());
    }

    #[test]
    fn display_names_match_loosely() {
        assert!(display_matches("DISPLAY2", r"\\.\DISPLAY2"));
        assert!(display_matches(r" \\.\display2 ", r"\\.\DISPLAY2"));
        assert!(!display_matches("DISPLAY1", r"\\.\DISPLAY12"));
    }
}
