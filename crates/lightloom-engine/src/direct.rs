// SPDX-License-Identifier: GPL-3.0-or-later
//! Direct LED colours: per-endpoint colours set from outside the canvas (the
//! OpenRGB SDK server) that win over canvas sampling and the forced colour
//! while their writer is alive.
//!
//! [`DirectLeds`] is shared by the writer (API side) and every device session
//! (which reads it once per frame). Entries are keyed by endpoint uid (the
//! device key, `<device key>:<subdevice id>` or a component instance id) and
//! hold one colour per LED in the endpoint's LED order (subdevices: cells,
//! row-major). Each entry belongs to one writer (`owner`) and lives until
//! its deadline: the writer extends it ([`DirectLeds::touch`]) while it is
//! connected and drops its entries when it goes away
//! ([`DirectLeds::release`]); a writer that hangs simply lets them expire,
//! and the endpoint returns to the canvas.

use crate::color::Rgb;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Most LEDs kept for one endpoint.
pub const MAX_DIRECT_LEDS: usize = 65_535;

#[derive(Clone, Debug)]
struct Entry {
    owner: u64,
    colors: Arc<[Rgb]>,
    deadline: Instant,
}

/// The shared direct-colour table; see the module documentation.
#[derive(Debug, Default)]
pub struct DirectLeds {
    entries: Mutex<HashMap<String, Entry>>,
    /// Bumped on every change, so readers can tell nothing changed.
    generation: AtomicU64,
    next_owner: AtomicU64,
}

impl DirectLeds {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// A fresh writer id (never 0).
    pub fn new_owner(&self) -> u64 {
        self.next_owner.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Changes so far (any set, update, release or expiry seen by a reader).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Write `colors` into endpoint `uid` starting at LED `start`, for an
    /// endpoint of `len` LEDs (colours past `len` are dropped). LEDs not
    /// written yet keep their previous direct colour, or black when the
    /// endpoint was not driven directly (or by another writer) before.
    pub fn write(&self, owner: u64, uid: &str, len: usize, start: usize, colors: &[Rgb], ttl: Duration) {
        let len = len.min(MAX_DIRECT_LEDS);
        let now = Instant::now();
        let mut entries = self.lock();
        let mut buf: Vec<Rgb> = match entries.get(uid) {
            Some(e) if e.owner == owner && e.deadline > now && e.colors.len() == len => e.colors.to_vec(),
            _ => vec![Rgb::BLACK; len],
        };
        for (slot, c) in buf.iter_mut().skip(start).zip(colors) {
            *slot = *c;
        }
        entries.insert(
            uid.to_string(),
            Entry {
                owner,
                colors: buf.into(),
                deadline: now + ttl,
            },
        );
        drop(entries);
        self.bump();
    }

    /// Extend every entry of `owner` to `ttl` from now (the writer is alive).
    pub fn touch(&self, owner: u64, ttl: Duration) {
        let deadline = Instant::now() + ttl;
        for e in self.lock().values_mut().filter(|e| e.owner == owner) {
            e.deadline = e.deadline.max(deadline);
        }
    }

    /// Drop every entry of `owner`: its endpoints return to the canvas.
    pub fn release(&self, owner: u64) {
        let mut entries = self.lock();
        let before = entries.len();
        entries.retain(|_, e| e.owner != owner);
        let changed = entries.len() != before;
        drop(entries);
        if changed {
            self.bump();
        }
    }

    /// The direct colours of `uid`, unless absent or expired (an expired
    /// entry is removed).
    pub fn get(&self, uid: &str) -> Option<Arc<[Rgb]>> {
        let mut entries = self.lock();
        let e = entries.get(uid)?;
        if e.deadline > Instant::now() {
            return Some(Arc::clone(&e.colors));
        }
        entries.remove(uid);
        drop(entries);
        self.bump();
        None
    }

    /// Whether any endpoint is driven directly now (expired entries count
    /// until a reader looks them up).
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Uids driven directly now.
    pub fn uids(&self) -> Vec<String> {
        let now = Instant::now();
        let mut v: Vec<String> = self
            .lock()
            .iter()
            .filter(|(_, e)| e.deadline > now)
            .map(|(k, _)| k.clone())
            .collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
    const GREEN: Rgb = Rgb { r: 0, g: 255, b: 0 };
    const LONG: Duration = Duration::from_secs(60);

    #[test]
    fn partial_writes_merge_and_release_clears() {
        let d = DirectLeds::new();
        let a = d.new_owner();
        assert!(d.get("dev").is_none());
        d.write(a, "dev", 3, 1, &[RED], LONG);
        assert_eq!(&*d.get("dev").unwrap(), &[Rgb::BLACK, RED, Rgb::BLACK]);
        d.write(a, "dev", 3, 2, &[GREEN, GREEN], LONG);
        assert_eq!(&*d.get("dev").unwrap(), &[Rgb::BLACK, RED, GREEN]);
        // Another writer starts from black.
        let b = d.new_owner();
        d.write(b, "dev", 3, 0, &[GREEN], LONG);
        assert_eq!(&*d.get("dev").unwrap(), &[GREEN, Rgb::BLACK, Rgb::BLACK]);
        let g = d.generation();
        d.release(a);
        assert_eq!(d.generation(), g, "a no longer owns anything");
        d.release(b);
        assert!(d.get("dev").is_none() && d.is_empty());
        assert!(d.generation() > g);
    }

    #[test]
    fn entries_expire_unless_touched() {
        let d = DirectLeds::new();
        let a = d.new_owner();
        d.write(a, "dev", 1, 0, &[RED], Duration::ZERO);
        assert!(d.get("dev").is_none());
        assert!(d.is_empty(), "the expired entry was dropped");
        d.write(a, "dev", 1, 0, &[RED], Duration::ZERO);
        d.touch(a, LONG);
        assert_eq!(&*d.get("dev").unwrap(), &[RED]);
        assert_eq!(d.uids(), vec!["dev".to_string()]);
    }
}
