// SPDX-License-Identifier: GPL-3.0-or-later
//! HID arrival/removal notifications.

use std::time::Duration;

/// How long hotplug events must be quiet before enumerating again (one plug
/// produces a burst of events, one per HID collection).
pub const HOTPLUG_SETTLE: Duration = Duration::from_millis(250);

/// HID arrival/removal notifications; inactive for mock runs and where the
/// platform has none yet.
pub struct Hotplug {
    #[cfg(windows)]
    _source: Option<lightloom_hal_windows::hotplug::HidHotplug>,
    #[cfg(target_os = "linux")]
    _source: Option<lightloom_hal_linux::HidrawHotplug>,
    events: Option<std::sync::mpsc::Receiver<lightloom_hal::DeviceEvent>>,
}

impl Hotplug {
    /// No notifications.
    pub fn inactive() -> Self {
        Self {
            #[cfg(any(windows, target_os = "linux"))]
            _source: None,
            events: None,
        }
    }

    /// Subscribe; falls back to [`Hotplug::inactive`] (with a warning on stderr)
    /// when the OS refuses.
    #[cfg(any(windows, target_os = "linux"))]
    pub fn start() -> Self {
        use lightloom_hal::HotplugSource;
        #[cfg(windows)]
        let mut source = lightloom_hal_windows::hotplug::HidHotplug::new();
        #[cfg(target_os = "linux")]
        let mut source = lightloom_hal_linux::HidrawHotplug::new();
        match source.subscribe() {
            Ok(events) => Self {
                _source: Some(source),
                events: Some(events),
            },
            Err(e) => {
                eprintln!("lightloom: hotplug notifications unavailable ({e}); devices plugged in later are not driven");
                Self::inactive()
            }
        }
    }

    /// No notifications on this OS.
    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn start() -> Self {
        Self::inactive()
    }

    /// Whether notifications arrive.
    pub fn active(&self) -> bool {
        self.events.is_some()
    }

    /// Whether any device arrived or left since the last call.
    pub fn changed(&self) -> bool {
        let Some(rx) = &self.events else {
            return false;
        };
        let mut any = false;
        while rx.try_recv().is_ok() {
            any = true;
        }
        any
    }
}
