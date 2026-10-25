// SPDX-License-Identifier: GPL-3.0-or-later
//! System sleep/resume, lock/unlock and display off/on notifications
//! (`lightloom_hal::power`): a hidden window on Windows, systemd-logind over
//! D-Bus on Linux (`PrepareForSleep`, session `Lock`/`Unlock`); inactive for
//! mock runs and on other platforms.

use lightloom_hal::power::PowerSink;

/// The platform's power events, delivered to a sink while this lives.
pub struct Power {
    #[cfg(windows)]
    _source: Option<lightloom_hal_windows::power::WindowsPower>,
    #[cfg(target_os = "linux")]
    _source: Option<lightloom_hal_linux::LogindPower>,
}

impl Power {
    /// No notifications.
    pub fn inactive() -> Self {
        Self {
            #[cfg(any(windows, target_os = "linux"))]
            _source: None,
        }
    }

    /// Subscribe `sink`; falls back to [`Power::inactive`] (with a warning)
    /// when the OS refuses.
    #[cfg(any(windows, target_os = "linux"))]
    pub fn start(sink: PowerSink) -> Self {
        use lightloom_hal::PowerSource;
        #[cfg(windows)]
        let mut source = lightloom_hal_windows::power::WindowsPower::new();
        #[cfg(target_os = "linux")]
        let mut source = lightloom_hal_linux::LogindPower::new();
        match source.subscribe(sink) {
            Ok(()) => Self {
                _source: Some(source),
            },
            Err(e) => {
                tracing::warn!(
                    "sleep/resume notifications unavailable ({e}); devices are not put away before sleep"
                );
                Self::inactive()
            }
        }
    }

    /// No notifications on this OS.
    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn start(_sink: PowerSink) -> Self {
        Self::inactive()
    }

    /// Whether notifications arrive.
    pub fn active(&self) -> bool {
        #[cfg(any(windows, target_os = "linux"))]
        return self._source.is_some();
        #[cfg(not(any(windows, target_os = "linux")))]
        false
    }
}
