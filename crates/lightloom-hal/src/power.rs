// SPDX-License-Identifier: GPL-3.0-or-later
//! System power and session events: sleep and resume, lock and unlock, the
//! display turning off and on.
//!
//! A [`PowerSource`] delivers them to a [`PowerSink`] on its own thread. The
//! sink of [`PowerEvent::Suspend`] may block until the devices are put away:
//! the OS waits for the handler (a bounded time, a couple of seconds on
//! Windows), so the consumer bounds its own wait too. Sources may repeat an
//! event (Windows sends both `PBT_APMRESUMEAUTOMATIC` and
//! `PBT_APMRESUMESUSPEND`): consumers treat every event as idempotent.
//!
//! Backends: `lightloom_hal_windows::power` (a hidden window for
//! `WM_POWERBROADCAST` and `WM_WTSSESSION_CHANGE`) and
//! `lightloom_hal_linux::logind` (systemd-logind `PrepareForSleep` and
//! session lock/unlock over D-Bus).

use crate::error::HalResult;
use std::sync::Arc;

/// A power or session event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PowerEvent {
    /// The system is about to sleep or hibernate.
    Suspend,
    /// The system woke up.
    Resume,
    /// The user's session was locked.
    Lock,
    /// It was unlocked.
    Unlock,
    /// The display turned off.
    DisplayOff,
    /// The display turned on (or dimmed).
    DisplayOn,
}

/// Receives [`PowerEvent`]s (see the module docs: `Suspend` may block).
pub type PowerSink = Arc<dyn Fn(PowerEvent) + Send + Sync>;

/// Source of [`PowerEvent`]s.
pub trait PowerSource: Send {
    /// Start delivering events to `sink` (replacing an earlier subscription).
    fn subscribe(&mut self, sink: PowerSink) -> HalResult<()>;
    /// Stop delivering events.
    fn stop(&mut self);
}

/// A [`PowerSource`] driven by hand (tests, platforms without one).
#[derive(Default)]
pub struct ManualPower {
    sink: std::sync::Mutex<Option<PowerSink>>,
}

impl std::fmt::Debug for ManualPower {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManualPower")
            .field("subscribed", &self.subscribed())
            .finish()
    }
}

impl ManualPower {
    /// A source nobody listens to yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a sink is subscribed.
    pub fn subscribed(&self) -> bool {
        self.sink.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Deliver `event` as the OS would (blocking as long as the sink does);
    /// `false` without a subscriber.
    pub fn fire(&self, event: PowerEvent) -> bool {
        let sink = self.sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match sink {
            Some(sink) => {
                sink(event);
                true
            }
            None => false,
        }
    }
}

impl PowerSource for ManualPower {
    fn subscribe(&mut self, sink: PowerSink) -> HalResult<()> {
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
        Ok(())
    }

    fn stop(&mut self) {
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn manual_source_delivers_until_stopped() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut src = ManualPower::new();
        assert!(!src.fire(PowerEvent::Suspend), "nobody listens yet");
        let s = Arc::clone(&seen);
        src.subscribe(Arc::new(move |e| s.lock().unwrap().push(e)))
            .unwrap();
        assert!(src.fire(PowerEvent::Suspend));
        assert!(src.fire(PowerEvent::Resume));
        src.stop();
        assert!(!src.fire(PowerEvent::Lock));
        assert_eq!(*seen.lock().unwrap(), [PowerEvent::Suspend, PowerEvent::Resume]);
    }
}
