// SPDX-License-Identifier: Apache-2.0 OR MIT
//! In-process renderer producing synthetic frames.

use crate::clock::FrameClock;
use crate::{RenderError, RendererClient};
use huedeck_engine::{Frame, FrameStore, Rgb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// What the null renderer draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    /// Every pixel the same colour.
    Solid(Rgb),
    /// Horizontal hue sweep scrolling 2° per tick.
    Rainbow,
    /// Eight vertical colour bars with a white column marching one pixel per tick,
    /// so a device's placement and orientation can be checked by eye.
    TestPattern,
}

impl Pattern {
    /// Render one frame of this pattern at `tick`.
    pub fn render(&self, width: u32, height: u32, tick: u64) -> Frame {
        match *self {
            Pattern::Solid(c) => Frame::solid(width, height, c),
            Pattern::Rainbow => {
                let mut f = Frame::new(width, height);
                let w = width.max(1) as f32;
                let shift = (tick * 2 % 360) as f32;
                for x in 0..width {
                    let hue = x as f32 / w * 360.0 + shift;
                    let c = Rgb::from_hsv(hue, 1.0, 1.0);
                    for y in 0..height {
                        f.set_pixel(x, y, c);
                    }
                }
                f
            }
            Pattern::TestPattern => {
                const BARS: [Rgb; 8] = [
                    Rgb::new(255, 255, 255),
                    Rgb::new(255, 255, 0),
                    Rgb::new(0, 255, 255),
                    Rgb::new(0, 255, 0),
                    Rgb::new(255, 0, 255),
                    Rgb::new(255, 0, 0),
                    Rgb::new(0, 0, 255),
                    Rgb::new(0, 0, 0),
                ];
                let mut f = Frame::new(width, height);
                let bar_w = (width / BARS.len() as u32).max(1);
                let marker = if width == 0 {
                    0
                } else {
                    (tick % width as u64) as u32
                };
                for x in 0..width {
                    let c = if x == marker {
                        Rgb::WHITE
                    } else {
                        BARS[((x / bar_w) as usize).min(BARS.len() - 1)]
                    };
                    for y in 0..height {
                        f.set_pixel(x, y, c);
                    }
                }
                f
            }
        }
    }
}

/// Renders a [`Pattern`] into a [`FrameStore`] from a background thread.
#[derive(Debug)]
pub struct NullRenderer {
    pattern: Pattern,
    fps: f64,
    width: u32,
    height: u32,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NullRenderer {
    /// Renderer for a 320x200 canvas.
    pub fn new(pattern: Pattern, fps: f64) -> Self {
        Self::with_size(
            pattern,
            fps,
            huedeck_engine::CANVAS_WIDTH,
            huedeck_engine::CANVAS_HEIGHT,
        )
    }

    /// Renderer for an arbitrary canvas size (LCD faces, tests).
    pub fn with_size(pattern: Pattern, fps: f64, width: u32, height: u32) -> Self {
        Self {
            pattern,
            fps,
            width,
            height,
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    /// The pattern being drawn.
    pub fn pattern(&self) -> Pattern {
        self.pattern
    }
}

impl RendererClient for NullRenderer {
    fn name(&self) -> &str {
        "null"
    }

    fn start(&mut self, store: Arc<FrameStore>) -> Result<(), RenderError> {
        if self.thread.is_some() {
            return Err(RenderError::AlreadyRunning);
        }
        if (store.width(), store.height()) != (self.width, self.height) {
            return Err(RenderError::SizeMismatch {
                renderer: (self.width, self.height),
                store: (store.width(), store.height()),
            });
        }
        self.stop.store(false, Ordering::Release);
        let stop = Arc::clone(&self.stop);
        let (pattern, fps, width, height) = (self.pattern, self.fps, self.width, self.height);
        let handle = thread::Builder::new()
            .name("huedeck-null-renderer".into())
            .spawn(move || {
                let mut clock = FrameClock::new(fps);
                while !stop.load(Ordering::Acquire) {
                    let tick = clock.wait_tick();
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    store.publish(pattern.render(width, height, tick));
                }
            })?;
        self.thread = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }

    fn is_running(&self) -> bool {
        self.thread.is_some() && !self.stop.load(Ordering::Acquire)
    }
}

impl Drop for NullRenderer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn patterns_render_deterministically() {
        let solid = Pattern::Solid(Rgb::SIGNAL_BLUE).render(4, 2, 0);
        assert!(solid.as_bgra().chunks(4).all(|p| p == [0xDB, 0x9E, 0x00, 0xFF]));

        let r0 = Pattern::Rainbow.render(320, 1, 0);
        let r1 = Pattern::Rainbow.render(320, 1, 1);
        assert_eq!(r0.pixel(0, 0), Rgb::new(255, 0, 0));
        assert_ne!(r0, r1, "rainbow must scroll with the tick");
        assert_eq!(
            Pattern::Rainbow.render(320, 1, 180),
            r0,
            "scroll wraps every 180 ticks"
        );

        let t = Pattern::TestPattern.render(16, 1, 5);
        assert_eq!(t.pixel(5, 0), Rgb::WHITE);
        assert_eq!(t.pixel(2, 0), Rgb::new(255, 255, 0));
        assert_eq!(t.pixel(15, 0), Rgb::BLACK);
    }

    #[test]
    fn null_renderer_publishes_frames_then_stops() {
        let store = Arc::new(FrameStore::new(8, 4));
        let mut r = NullRenderer::with_size(Pattern::Rainbow, 60.0, 8, 4);
        assert!(!r.is_running());
        r.start(Arc::clone(&store)).unwrap();
        assert!(r.is_running());
        assert!(matches!(
            r.start(Arc::clone(&store)),
            Err(RenderError::AlreadyRunning)
        ));

        let seq = store
            .wait_for_newer(0, Duration::from_secs(5))
            .expect("first frame");
        let seq = store
            .wait_for_newer(seq, Duration::from_secs(5))
            .expect("second frame");
        assert!(seq >= 2);
        r.stop();
        assert!(!r.is_running());
        let after = store.seq();
        thread::sleep(Duration::from_millis(50));
        assert_eq!(store.seq(), after, "no frames after stop");
    }

    #[test]
    fn size_mismatch_is_rejected() {
        let store = Arc::new(FrameStore::new(2, 2));
        let mut r = NullRenderer::new(Pattern::TestPattern, 30.0);
        assert!(matches!(r.start(store), Err(RenderError::SizeMismatch { .. })));
    }
}
