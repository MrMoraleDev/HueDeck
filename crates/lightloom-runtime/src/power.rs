// SPDX-License-Identifier: GPL-3.0-or-later
//! System sleep/resume, lock/unlock and display off/on notifications
//! (`lightloom_hal::power`); inactive for mock runs and where the platform has
//! none yet (Linux: logind `PrepareForSleep` over D-Bus, not implemented).

use lightloom_hal::power::PowerSink;

/// The platform's power events, delivered to a sink while this lives.
pub struct Power {
    #[cfg(windows)]
    _source: Option<lightloom_hal_windows::power::WindowsPower>,
}

impl Power {
    /// No notifications.
    pub fn inactive() -> Self {
        Self {
            #[cfg(windows)]
            _source: None,
        }
    }

    /// Subscribe `sink`; falls back to [`Power::inactive`] (with a warning)
    /// when the OS refuses.
    #[cfg(windows)]
    pub fn start(sink: PowerSink) -> Self {
        use lightloom_hal::PowerSource;
        let mut source = lightloom_hal_windows::power::WindowsPower::new();
        match source.subscribe(sink) {
            Ok(()) => Self {
                _source: Some(source),
            },
            Err(e) => {
                tracing::warn!("sleep/resume notifications unavailable ({e}); devices are not put away before sleep");
                Self::inactive()
            }
        }
    }

    /// No notifications on this OS yet.
    #[cfg(not(windows))]
    pub fn start(_sink: PowerSink) -> Self {
        Self::inactive()
    }

    /// Whether notifications arrive.
    pub fn active(&self) -> bool {
        #[cfg(windows)]
        return self._source.is_some();
        #[cfg(not(windows))]
        false
    }
}
