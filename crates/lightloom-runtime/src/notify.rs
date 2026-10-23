// SPDX-License-Identifier: GPL-3.0-or-later
//! This OS's system notifications: Windows toasts
//! (`lightloom_hal_windows::toast`); elsewhere none yet ([`NoNotifier`]; Linux
//! gets libnotify / the freedesktop D-Bus service behind the same trait).

use lightloom_hal::notify::{NoNotifier, Notifier};
use std::sync::Arc;

/// Lightloom's system notifications on this OS.
pub fn system_notifier() -> Arc<dyn Notifier> {
    #[cfg(windows)]
    {
        Arc::new(lightloom_hal_windows::toast::ToastNotifier::lightloom())
    }
    #[cfg(not(windows))]
    {
        Arc::new(NoNotifier)
    }
}

/// No system notifications (tests, `--no-hardware` runs that must stay quiet).
pub fn no_notifier() -> Arc<dyn Notifier> {
    Arc::new(NoNotifier)
}
