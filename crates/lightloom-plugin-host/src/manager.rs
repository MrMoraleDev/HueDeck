// SPDX-License-Identifier: GPL-3.0-or-later
//! Device manager: one session per present device, following hotplug.
//!
//! The caller enumerates (at start, and after hotplug notifications once they
//! settle) and hands [`DeviceManager::reconcile`] the devices present now,
//! each with the plugin that drives it. The manager then
//!
//! * stops the sessions of devices that are gone (the report of a session
//!   that was still running is marked [`SessionReport::device_removed`]);
//! * restarts sessions whose device is present but whose latest I/O failed
//!   with "disconnected": a quick unplug/replug the enumeration never saw
//!   missing leaves the session holding a stale handle. Restarts of one device
//!   back off exponentially (1 s up to [`MAX_RESTART_DELAY`]);
//! * starts sessions for devices that have none, but never while an earlier
//!   session of the same device is still shutting down.
//!
//! A session that ended on its own (a fault) is not restarted while its
//! device stays plugged in, so a broken plugin does not loop; unplugging and
//! replugging the device starts it afresh.
//!
//! [`DeviceManager::save`] hands a `SaveToDevice(request)` to a device's session,
//! at most once per [`crate::save::SAVE_INTERVAL`] per device (previews are free).

use crate::catalog::CatalogEntry;
use crate::save::{SaveAction, SaveChoice, SaveLimiter, SaveOutcome, SaveRefused};
use crate::session::{self, Control, SaveCall, SessionDevice, SessionHandle, SessionOptions, SessionReport};
use lightloom_engine::FrameStore;
use lightloom_hal::hid::HidBackend;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long stopped sessions get to run `Shutdown()` before the manager stops waiting
/// for them (they keep their device until they end; no new session starts meanwhile).
pub const STOP_TIMEOUT: Duration = Duration::from_secs(10);
/// A session younger than this is not restarted for a lost device.
pub const RESTART_AFTER: Duration = Duration::from_secs(1);
/// Longest wait between two restarts of a device that keeps losing its handle.
pub const MAX_RESTART_DELAY: Duration = Duration::from_secs(60);
/// A session this old without a lost device resets its device's restart backoff.
pub const HEALTHY_AFTER: Duration = Duration::from_secs(30);

/// A device present now, with the plugin that drives it.
#[derive(Clone, Debug)]
pub struct Present {
    /// The device.
    pub device: SessionDevice,
    /// Its plugin.
    pub plugin: CatalogEntry,
}

/// What [`DeviceManager::reconcile`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A session started for a device that had none.
    Started {
        /// Device key.
        key: String,
        /// Plugin `Name()`.
        plugin: String,
    },
    /// The device went away; its session was stopped.
    Removed {
        /// Device key.
        key: String,
    },
    /// The device came back behind a stale handle; its session was restarted.
    Restarted {
        /// Device key.
        key: String,
    },
    /// A session could not be started (thread creation failed).
    Failed {
        /// Device key.
        key: String,
        /// Why.
        error: String,
    },
}

/// Builds the options of each new session (per-device channel setup, log sink, ...).
pub type OptionsFn = Box<dyn FnMut(&SessionDevice, &CatalogEntry) -> SessionOptions + Send>;

#[derive(Clone, Copy, Debug)]
struct Backoff {
    /// Wait after the next restart.
    delay: Duration,
    /// No restart before this.
    not_before: Instant,
}

/// Sessions for the devices that are present.
pub struct DeviceManager {
    backend: Arc<dyn HidBackend>,
    frames: Arc<FrameStore>,
    options: OptionsFn,
    sessions: BTreeMap<String, SessionHandle>,
    /// Stopped sessions that had not ended within the stop timeout, with
    /// whether their device was removed.
    stopping: Vec<(SessionHandle, bool)>,
    backoff: BTreeMap<String, Backoff>,
    reports: Vec<SessionReport>,
    failed: Vec<String>,
    stop_timeout: Duration,
    restart_after: Duration,
    saves: SaveLimiter,
}

impl std::fmt::Debug for DeviceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceManager")
            .field("sessions", &self.sessions.keys().collect::<Vec<_>>())
            .field("stopping", &self.stopping.len())
            .field("reports", &self.reports.len())
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

impl DeviceManager {
    /// A manager whose sessions render `frames` through `backend`.
    pub fn new(backend: Arc<dyn HidBackend>, frames: Arc<FrameStore>, options: OptionsFn) -> Self {
        Self {
            backend,
            frames,
            options,
            sessions: BTreeMap::new(),
            stopping: Vec::new(),
            backoff: BTreeMap::new(),
            reports: Vec::new(),
            failed: Vec::new(),
            stop_timeout: STOP_TIMEOUT,
            restart_after: RESTART_AFTER,
            saves: SaveLimiter::default(),
        }
    }

    /// Shortest time between two saves to one device (default
    /// [`crate::save::SAVE_INTERVAL`]; tests shorten it).
    pub fn with_save_interval(mut self, interval: Duration) -> Self {
        self.saves = SaveLimiter::new(interval);
        self
    }

    /// Hand `SaveToDevice(request)` to the running session of `key`; the outcome
    /// arrives on the receiver once the session ran it (between two frames). A
    /// save (not a preview) counts against the device's rate limit as soon as it
    /// is handed over, whatever its outcome.
    pub fn save(
        &mut self,
        key: &str,
        action: SaveAction,
        choice: SaveChoice,
    ) -> Result<Receiver<SaveOutcome>, SaveRefused> {
        let Some(h) = self.sessions.get(key).filter(|h| !h.is_finished()) else {
            return Err(SaveRefused::NotRunning(key.to_string()));
        };
        if action == SaveAction::Save {
            self.saves.check(key)?;
        }
        let (reply, rx) = channel();
        if !h.control(Control::Save(SaveCall {
            action,
            choice,
            reply,
        })) {
            return Err(SaveRefused::NotRunning(key.to_string()));
        }
        if action == SaveAction::Save {
            self.saves.take(key)?;
        }
        Ok(rx)
    }

    /// Seconds until `key` may be saved again (0: now).
    pub fn save_wait_secs(&self, key: &str) -> u64 {
        self.saves.wait_secs(key)
    }

    /// Minimum age of a session restarted for a lost device (default
    /// [`RESTART_AFTER`]); the backoff after a restart starts at the larger of
    /// this and one second.
    pub fn with_restart_after(mut self, age: Duration) -> Self {
        self.restart_after = age;
        self
    }

    /// How long stopped sessions get to finish (default [`STOP_TIMEOUT`]).
    pub fn with_stop_timeout(mut self, timeout: Duration) -> Self {
        self.stop_timeout = timeout;
        self
    }

    /// Whether the session of `key` lost its device and may be restarted now.
    fn restartable(&self, key: &str, h: &SessionHandle) -> bool {
        !h.is_finished()
            && h.live().disconnected
            && h.started().elapsed() >= self.restart_after
            && self
                .backoff
                .get(key)
                .is_none_or(|b| Instant::now() >= b.not_before)
    }

    /// Make the sessions match the devices `present` now (see the module docs).
    pub fn reconcile(&mut self, present: Vec<Present>) -> Vec<Change> {
        self.reap_stopping();
        let mut changes = Vec::new();
        let keys: BTreeSet<&str> = present.iter().map(|p| p.device.key.as_str()).collect();

        // Gone: stop. Only a session still running was cut off by the removal.
        let gone: Vec<String> = self
            .sessions
            .keys()
            .filter(|k| !keys.contains(k.as_str()))
            .cloned()
            .collect();
        for key in &gone {
            if let Some(h) = self.sessions.remove(key) {
                let running = !h.is_finished();
                self.stop(vec![h], running);
            }
            self.backoff.remove(key);
            changes.push(Change::Removed { key: key.clone() });
        }

        // Healthy for a while: the device's backoff starts over.
        let healthy: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, h)| !h.live().disconnected && h.started().elapsed() >= HEALTHY_AFTER)
            .map(|(k, _)| k.clone())
            .collect();
        for key in healthy {
            self.backoff.remove(&key);
        }

        // Stale handles: restart, backing off.
        let stale: Vec<String> = self
            .sessions
            .iter()
            .filter(|(k, h)| keys.contains(k.as_str()) && self.restartable(k, h))
            .map(|(k, _)| k.clone())
            .collect();
        for key in &stale {
            if let Some(h) = self.sessions.remove(key) {
                self.stop(vec![h], true);
            }
            let first = self.restart_after.max(Duration::from_secs(1));
            let delay = self
                .backoff
                .get(key)
                .map_or(first, |b| (b.delay * 2).min(MAX_RESTART_DELAY));
            self.backoff.insert(
                key.clone(),
                Backoff {
                    delay,
                    not_before: Instant::now() + delay,
                },
            );
        }

        for p in present {
            let key = p.device.key.clone();
            if self.sessions.contains_key(&key) || self.stopping.iter().any(|(h, _)| h.key() == key) {
                continue;
            }
            let name = p.plugin.meta.name.clone();
            let opts = (self.options)(&p.device, &p.plugin);
            match session::spawn(
                p.device,
                p.plugin,
                Arc::clone(&self.backend),
                Arc::clone(&self.frames),
                opts,
            ) {
                Ok(handle) => {
                    self.sessions.insert(key.clone(), handle);
                    changes.push(if stale.contains(&key) {
                        Change::Restarted { key }
                    } else {
                        Change::Started { key, plugin: name }
                    });
                }
                Err(e) => changes.push(Change::Failed {
                    key,
                    error: e.to_string(),
                }),
            }
        }
        changes
    }

    /// Sessions running or ended but not collected yet, by device key.
    pub fn sessions(&self) -> impl Iterator<Item = &SessionHandle> {
        self.sessions.values()
    }

    /// Whether every session has ended (true without sessions).
    pub fn all_finished(&self) -> bool {
        self.sessions.values().all(SessionHandle::is_finished)
    }

    /// Whether a session lost its device and may be restarted now (a reconcile is due).
    pub fn has_disconnected(&self) -> bool {
        self.sessions.iter().any(|(k, h)| self.restartable(k, h))
    }

    /// The reports of sessions that ended so far, and the sessions that panicked
    /// or did not stop in time, handing them over (a long-running caller drains
    /// them so they do not pile up).
    pub fn drain_reports(&mut self) -> (Vec<SessionReport>, Vec<String>) {
        self.reap_stopping();
        (
            std::mem::take(&mut self.reports),
            std::mem::take(&mut self.failed),
        )
    }

    /// Stop every session and return all reports (including those of sessions
    /// stopped earlier) and the sessions that panicked or did not stop in time.
    pub fn finish(mut self) -> (Vec<SessionReport>, Vec<String>) {
        let all = std::mem::take(&mut self.sessions).into_values().collect();
        self.stop(all, false);
        // Sessions still shutting down get the stop timeout once more, then are given up on.
        let stuck = std::mem::take(&mut self.stopping);
        let deadline = Instant::now() + self.stop_timeout;
        while stuck.iter().any(|(h, _)| !h.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        for (h, removed) in stuck {
            self.collect(h, removed);
        }
        (self.reports, self.failed)
    }

    /// Collect the earlier sessions that have ended since they were asked to stop.
    fn reap_stopping(&mut self) {
        let (done, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.stopping)
            .into_iter()
            .partition(|(h, _)| h.is_finished());
        self.stopping = waiting;
        for (h, removed) in done {
            self.collect(h, removed);
        }
    }

    fn stop(&mut self, handles: Vec<SessionHandle>, removed: bool) {
        for h in &handles {
            h.stop();
        }
        let deadline = Instant::now() + self.stop_timeout;
        while handles.iter().any(|h| !h.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        for h in handles {
            if h.is_finished() {
                self.collect(h, removed);
            } else {
                // Still holding its device: no new session for it until it ends.
                self.stopping.push((h, removed));
            }
        }
    }

    /// Join an ended session, or record that it never ended.
    fn collect(&mut self, h: SessionHandle, removed: bool) {
        let key = h.key().to_string();
        if !h.is_finished() {
            self.failed.push(format!(
                "{key}: did not stop within {} s (stuck in device I/O?)",
                self.stop_timeout.as_secs()
            ));
            return;
        }
        match h.join() {
            Ok(mut r) => {
                r.device_removed |= removed;
                self.reports.push(r);
            }
            Err(panic) => self.failed.push(format!("{key}: panicked: {panic}")),
        }
    }
}

/// Stop `handles` and collect the reports of those that end within `timeout`.
/// Sessions that panicked or are stuck (in device I/O) come back as
/// `key: reason`; stuck threads are left behind and end with the process.
pub fn stop_sessions(handles: Vec<SessionHandle>, timeout: Duration) -> (Vec<SessionReport>, Vec<String>) {
    for h in &handles {
        h.stop();
    }
    let deadline = Instant::now() + timeout;
    while handles.iter().any(|h| !h.is_finished()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut reports = Vec::new();
    let mut failed = Vec::new();
    for h in handles {
        let key = h.key().to_string();
        if !h.is_finished() {
            failed.push(format!(
                "{key}: did not stop within {} s (stuck in device I/O?)",
                timeout.as_secs()
            ));
            continue;
        }
        match h.join() {
            Ok(r) => reports.push(r),
            Err(panic) => failed.push(format!("{key}: panicked: {panic}")),
        }
    }
    (reports, failed)
}
