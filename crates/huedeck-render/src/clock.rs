// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Fixed-rate frame clock.
//!
//! `std::thread::sleep` plus a short spin window keeps jitter well under a
//! millisecond on current Windows and Linux builds. The high-resolution
//! waitable timer / timerfd variant from the architecture notes replaces the
//! sleep in a later milestone without changing this API.

use std::thread;
use std::time::{Duration, Instant};

/// Lowest accepted frame rate.
pub const MIN_FPS: f64 = 1.0;
/// Highest accepted frame rate.
pub const MAX_FPS: f64 = 240.0;

/// Ticks at a fixed rate, correcting for sleep overshoot.
#[derive(Debug)]
pub struct FrameClock {
    period: Duration,
    next: Instant,
    tick: u64,
    spin_window: Duration,
}

impl FrameClock {
    /// Clock at `fps` (clamped to `MIN_FPS..=MAX_FPS`), first tick one period from now.
    pub fn new(fps: f64) -> Self {
        let fps = if fps.is_finite() { fps.clamp(MIN_FPS, MAX_FPS) } else { 30.0 };
        let period = Duration::from_secs_f64(1.0 / fps);
        Self { period, next: Instant::now() + period, tick: 0, spin_window: Duration::from_micros(1500) }
    }

    /// Tick period.
    pub fn period(&self) -> Duration {
        self.period
    }

    /// Frames per second.
    pub fn fps(&self) -> f64 {
        1.0 / self.period.as_secs_f64()
    }

    /// Ticks delivered so far.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Block until the next tick; returns the tick number (starting at 1).
    /// If the caller fell more than one period behind, the schedule resyncs
    /// instead of firing a burst of catch-up ticks.
    pub fn wait_tick(&mut self) -> u64 {
        loop {
            let now = Instant::now();
            if now >= self.next {
                break;
            }
            let remaining = self.next - now;
            if remaining > self.spin_window {
                thread::sleep(remaining - self.spin_window);
            } else {
                thread::yield_now();
            }
        }
        self.tick += 1;
        self.next += self.period;
        let now = Instant::now();
        if self.next + self.period < now {
            self.next = now + self.period;
        }
        self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_fps() {
        assert_eq!(FrameClock::new(0.0).fps(), MIN_FPS);
        assert_eq!(FrameClock::new(1e9).fps(), MAX_FPS);
        assert!((FrameClock::new(f64::NAN).fps() - 30.0).abs() < 1e-9);
        assert!((FrameClock::new(30.0).period().as_secs_f64() - 1.0 / 30.0).abs() < 1e-9);
    }

    #[test]
    fn ticks_at_roughly_the_requested_rate() {
        let mut clock = FrameClock::new(100.0);
        let start = Instant::now();
        for expected in 1..=10 {
            assert_eq!(clock.wait_tick(), expected);
        }
        let elapsed = start.elapsed();
        // 10 ticks at 100 fps ≈ 100 ms; allow generous slack for loaded CI boxes.
        assert!(elapsed >= Duration::from_millis(90), "too fast: {elapsed:?}");
        assert!(elapsed <= Duration::from_millis(400), "too slow: {elapsed:?}");
    }

    #[test]
    fn resyncs_after_falling_behind() {
        let mut clock = FrameClock::new(200.0);
        thread::sleep(Duration::from_millis(50));
        let t = Instant::now();
        clock.wait_tick(); // immediate
        clock.wait_tick(); // must not fire instantly again: schedule was resynced
        assert!(t.elapsed() >= Duration::from_millis(3));
    }
}
