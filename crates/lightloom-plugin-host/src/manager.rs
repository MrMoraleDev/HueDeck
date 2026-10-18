// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Device manager: one session per present device, following hotplug.
//!
//! The caller enumerates (at start, and after hotplug notifications once they
//! settle) and hands [`DeviceManager::reconcile`] the devices present now,
//! each with the plugin that drives it. The manager then
//!
//! * stops the sessions of devices that are gone (their reports are marked
//!   [`SessionReport::device_removed`]);
//! * restarts sessions whose device is present but whose I/O failed with
//!   "disconnected": a quick unplug/replug the enumeration never saw missing
//!   leaves the session holding a stale handle;
//! * starts sessions for devices that have none.
//!
//! A session that ended on its own (a fault) is not restarted while its
//! device stays plugged in, so a broken plugin does not loop; unplugging and
//! replugging the device starts it afresh.

use crate::catalog::CatalogEntry;
use crate::session::{self, SessionDevice, SessionHandle, SessionOptions, SessionReport};
use lightloom_engine::FrameStore;
use lightloom_hal::hid::HidBackend;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long stopped sessions get to run `Shutdown()` before the manager gives up on them.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(10);
/// A session younger than this is not restarted for a lost device, so a
/// device that keeps failing cannot make the manager restart it in a loop.
pub const RESTART_AFTER: Duration = Duration::from_secs(1);

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

/// Sessions for the devices that are present.
pub struct DeviceManager {
    backend: Arc<dyn HidBackend>,
    frames: Arc<FrameStore>,
    options: OptionsFn,
    sessions: BTreeMap<String, SessionHandle>,
    reports: Vec<SessionReport>,
    failed: Vec<String>,
    stop_timeout: Duration,
    restart_after: Duration,
}

impl std::fmt::Debug for DeviceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceManager")
            .field("sessions", &self.sessions.keys().collect::<Vec<_>>())
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
            reports: Vec::new(),
            failed: Vec::new(),
            stop_timeout: STOP_TIMEOUT,
            restart_after: RESTART_AFTER,
        }
    }

    /// Minimum age of a session restarted for a lost device (default [`RESTART_AFTER`]).
    pub fn with_restart_after(mut self, age: Duration) -> Self {
        self.restart_after = age;
        self
    }

    /// How long stopped sessions get to finish (default [`STOP_TIMEOUT`]).
    pub fn with_stop_timeout(mut self, timeout: Duration) -> Self {
        self.stop_timeout = timeout;
        self
    }

    /// Make the sessions match the devices `present` now (see the module docs).
    pub fn reconcile(&mut self, present: Vec<Present>) -> Vec<Change> {
        let mut changes = Vec::new();
        let keys: BTreeSet<&str> = present.iter().map(|p| p.device.key.as_str()).collect();
        let gone: Vec<String> = self
            .sessions
            .keys()
            .filter(|k| !keys.contains(k.as_str()))
            .cloned()
            .collect();
        let stale: Vec<String> = self
            .sessions
            .iter()
            .filter(|(k, h)| {
                keys.contains(k.as_str())
                    && !h.is_finished()
                    && h.live().disconnects > 0
                    && h.started().elapsed() >= self.restart_after
            })
            .map(|(k, _)| k.clone())
            .collect();
        let stopping: Vec<SessionHandle> = gone
            .iter()
            .chain(&stale)
            .filter_map(|k| self.sessions.remove(k))
            .collect();
        self.stop(stopping, true);
        changes.extend(gone.into_iter().map(|key| Change::Removed { key }));

        for p in present {
            let key = p.device.key.clone();
            if self.sessions.contains_key(&key) {
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

    /// Whether a running session has lost its device (a reconcile is due).
    pub fn has_disconnected(&self) -> bool {
        self.sessions
            .values()
            .any(|h| !h.is_finished() && h.live().disconnects > 0)
    }

    /// Stop every session and return all reports (including those of sessions
    /// stopped earlier) and the sessions that panicked or did not stop in time.
    pub fn finish(mut self) -> (Vec<SessionReport>, Vec<String>) {
        let all = std::mem::take(&mut self.sessions).into_values().collect();
        self.stop(all, false);
        (self.reports, self.failed)
    }

    fn stop(&mut self, handles: Vec<SessionHandle>, removed: bool) {
        let (reports, failed) = stop_sessions(handles, self.stop_timeout);
        self.reports.extend(reports.into_iter().map(|mut r| {
            r.device_removed |= removed;
            r
        }));
        self.failed.extend(failed);
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
