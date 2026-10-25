// SPDX-License-Identifier: GPL-3.0-or-later
//! Where an LCD device's frames come from.
//!
//! A device session calls [`LcdProvider::attach`] when its plugin runs
//! `LCD.initialize`, and reads every `getFrame` from the returned store; it calls
//! [`LcdProvider::detach`] when the plugin initializes again with another size or the
//! session ends. The daemon's provider renders the device's chosen face in a CEF browser of
//! that size (crate `lightloom-daemon`, `lcd.rs`); [`MockLcdProvider`] needs no renderer.

use crate::spec::LcdSpec;
use lightloom_engine::{Frame, FrameStore, Rgb};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A device's link to its face renderer.
#[derive(Clone)]
pub struct LcdAttachment {
    /// Identifies this attachment in [`LcdProvider::detach`] (a restarted session may
    /// attach again before the old one detached).
    pub id: u64,
    /// The face's frames, at the attached size.
    pub frames: Arc<FrameStore>,
}

impl std::fmt::Debug for LcdAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LcdAttachment")
            .field("id", &self.id)
            .field("size", &(self.frames.width(), self.frames.height()))
            .finish()
    }
}

/// Renders LCD faces for device sessions.
pub trait LcdProvider: Send + Sync {
    /// The plugin of device `key` (shown as `name`) initialized its LCD at `spec`: start
    /// (or resize) its face renderer and return where the frames land.
    fn attach(&self, key: &str, name: &str, spec: LcdSpec) -> LcdAttachment;
    /// Attachment `id` of `key` is over; a newer attachment of the same key stays.
    fn detach(&self, key: &str, id: u64);
}

/// Process-wide attachment ids.
pub fn next_attachment_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The mock face: a horizontal red ramp over a vertical blue ramp, green 0x40, with a white
/// one-pixel border, so tests can tell orientation, size and padding apart.
pub fn mock_face_frame(width: u32, height: u32) -> Frame {
    let mut f = Frame::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let edge = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
            let c = if edge {
                Rgb::WHITE
            } else {
                Rgb::new(
                    (x * 255 / width.max(2).saturating_sub(1).max(1)) as u8,
                    0x40,
                    (y * 255 / height.max(2).saturating_sub(1).max(1)) as u8,
                )
            };
            f.set_pixel(x, y, c);
        }
    }
    f
}

/// One recorded provider call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MockEvent {
    /// `attach(key, _, spec)` answered with this id.
    Attach {
        /// Device key.
        key: String,
        /// Requested size.
        spec: LcdSpec,
        /// Attachment id.
        id: u64,
    },
    /// `detach(key, id)`.
    Detach {
        /// Device key.
        key: String,
        /// Attachment id.
        id: u64,
    },
}

/// A provider without a renderer: every attachment gets a store holding
/// [`mock_face_frame`] at the requested size. Records its calls.
#[derive(Default)]
pub struct MockLcdProvider {
    events: Mutex<Vec<MockEvent>>,
}

impl MockLcdProvider {
    /// A new mock.
    pub fn new() -> Self {
        Self::default()
    }

    /// The calls so far.
    pub fn events(&self) -> Vec<MockEvent> {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl LcdProvider for MockLcdProvider {
    fn attach(&self, key: &str, _name: &str, spec: LcdSpec) -> LcdAttachment {
        let frames = Arc::new(FrameStore::new(spec.width, spec.height));
        frames.publish(mock_face_frame(spec.width, spec.height));
        let id = next_attachment_id();
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(MockEvent::Attach {
                key: key.to_string(),
                spec,
                id,
            });
        LcdAttachment { id, frames }
    }

    fn detach(&self, key: &str, id: u64) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(MockEvent::Detach {
                key: key.to_string(),
                id,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_face_is_sized_and_bordered() {
        let f = mock_face_frame(5, 3);
        assert_eq!((f.width(), f.height()), (5, 3));
        assert_eq!(f.pixel(0, 0), Rgb::WHITE);
        assert_eq!(f.pixel(4, 2), Rgb::WHITE);
        assert_eq!(f.pixel(1, 1), Rgb::new(63, 0x40, 127));
        assert_eq!(f.pixel(3, 1), Rgb::new(191, 0x40, 127));
        // degenerate sizes do not divide by zero
        let _ = mock_face_frame(1, 1);
    }

    #[test]
    fn mock_provider_records_and_serves_frames() {
        let m = MockLcdProvider::new();
        let spec = LcdSpec::new(4.0, 2.0, false).unwrap();
        let a = m.attach("k", "Kraken", spec);
        let b = m.attach("k", "Kraken", spec);
        assert_ne!(a.id, b.id);
        assert_eq!((a.frames.width(), a.frames.height()), (4, 2));
        assert_eq!(*a.frames.latest().1, mock_face_frame(4, 2));
        m.detach("k", a.id);
        assert_eq!(m.events().len(), 3);
        assert_eq!(
            m.events()[2],
            MockEvent::Detach {
                key: "k".into(),
                id: a.id
            }
        );
    }
}
