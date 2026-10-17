// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The in-process frame hand-off between the renderer client and plugin threads.
//!
//! Design (synthesis decision D9): the producer publishes into a
//! `triple_buffer` so it never blocks on readers, a monotonically increasing
//! `AtomicU64` sequence number tells readers whether anything new exists, and a
//! `Condvar` wakes every plugin thread once per published frame. Slow plugin
//! threads simply observe a later `seq` and skip straight to the latest frame;
//! nothing queues up.
//!
//! Frames are shared as `Arc<Frame>` so handing one to N readers is N
//! refcount bumps, not N copies of 256 KB.

use crate::frame::Frame;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use triple_buffer::{Input, Output, TripleBuffer};

/// Latest-frame store shared by one producer and many consumers.
pub struct FrameStore {
    input: Mutex<Input<Arc<Frame>>>,
    output: Mutex<Output<Arc<Frame>>>,
    seq: AtomicU64,
    gate: Mutex<()>,
    cv: Condvar,
    width: u32,
    height: u32,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl FrameStore {
    /// Store pre-filled with a black frame of the given size (`seq == 0`).
    pub fn new(width: u32, height: u32) -> Self {
        let initial = Arc::new(Frame::new(width, height));
        // TODO(compile-check): triple_buffer 9 constructor is `TripleBuffer::new(&T)` (T: Clone).
        let (input, output) = TripleBuffer::new(&initial).split();
        Self {
            input: Mutex::new(input),
            output: Mutex::new(output),
            seq: AtomicU64::new(0),
            gate: Mutex::new(()),
            cv: Condvar::new(),
            width,
            height,
        }
    }

    /// Canvas width this store was created for.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Canvas height this store was created for.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Number of frames published so far.
    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::Acquire)
    }

    /// Publish a frame and wake every waiter. Returns the new sequence number.
    pub fn publish(&self, frame: Frame) -> u64 {
        self.publish_arc(Arc::new(frame))
    }

    /// Publish an already shared frame.
    pub fn publish_arc(&self, frame: Arc<Frame>) -> u64 {
        lock(&self.input).write(frame);
        // Bump and notify under the gate so a waiter that checked `seq` just
        // before blocking cannot miss this wake-up.
        let _g = lock(&self.gate);
        let seq = self.seq.fetch_add(1, Ordering::AcqRel) + 1;
        self.cv.notify_all();
        seq
    }

    /// The most recently published frame with its sequence number.
    pub fn latest(&self) -> (u64, Arc<Frame>) {
        let mut out = lock(&self.output);
        let frame = out.read().clone();
        (self.seq(), frame)
    }

    /// Block until `seq() > last_seen` or `timeout` elapses. Returns the new
    /// sequence number, or `None` on timeout. Pass `last_seen = seq()` to wait
    /// for the *next* frame.
    pub fn wait_for_newer(&self, last_seen: u64, timeout: Duration) -> Option<u64> {
        let deadline = Instant::now() + timeout;
        let mut guard = lock(&self.gate);
        loop {
            let seq = self.seq();
            if seq > last_seen {
                return Some(seq);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (g, _res) = self.cv.wait_timeout(guard, deadline - now).unwrap_or_else(|e| e.into_inner());
            guard = g;
        }
    }
}

impl std::fmt::Debug for FrameStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameStore")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("seq", &self.seq())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;
    use std::thread;

    #[test]
    fn publish_bumps_seq_and_latest_sees_it() {
        let store = FrameStore::new(4, 4);
        assert_eq!(store.seq(), 0);
        let (seq, f) = store.latest();
        assert_eq!(seq, 0);
        assert_eq!(f.pixel(0, 0), Rgb::BLACK);

        assert_eq!(store.publish(Frame::solid(4, 4, Rgb::WHITE)), 1);
        let (seq, f) = store.latest();
        assert_eq!(seq, 1);
        assert_eq!(f.pixel(3, 3), Rgb::WHITE);
    }

    #[test]
    fn readers_skip_to_latest() {
        let store = FrameStore::new(2, 2);
        store.publish(Frame::solid(2, 2, Rgb::new(1, 1, 1)));
        store.publish(Frame::solid(2, 2, Rgb::new(2, 2, 2)));
        store.publish(Frame::solid(2, 2, Rgb::new(3, 3, 3)));
        let (seq, f) = store.latest();
        assert_eq!(seq, 3);
        assert_eq!(f.pixel(0, 0), Rgb::new(3, 3, 3));
    }

    #[test]
    fn wait_for_newer_wakes_on_publish_and_times_out() {
        let store = Arc::new(FrameStore::new(2, 2));
        assert_eq!(store.wait_for_newer(0, Duration::from_millis(20)), None);

        let producer = {
            let store = Arc::clone(&store);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(30));
                store.publish(Frame::new(2, 2))
            })
        };
        let seen = store.wait_for_newer(0, Duration::from_secs(5));
        assert_eq!(seen, Some(1));
        assert_eq!(producer.join().unwrap(), 1);
        // Already newer than last_seen: returns immediately.
        assert_eq!(store.wait_for_newer(0, Duration::ZERO), Some(1));
    }
}
