// SPDX-License-Identifier: GPL-3.0-or-later
//! System notifications (the desktop's own pop-ups, outside the dashboard).
//!
//! Each OS backend implements [`Notifier`]: Windows toasts in
//! `lightloom-hal-windows` (`toast`), libnotify / the freedesktop
//! `org.freedesktop.Notifications` D-Bus service on Linux later.
//! [`NoNotifier`] stands in where there is none.

use crate::error::{HalError, HalResult};

/// One notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    /// First line, in bold.
    pub title: String,
    /// The text under it.
    pub body: String,
}

/// Shows system notifications. `notify` must not block for long: backends
/// that wait on the OS do so on their own thread.
pub trait Notifier: Send + Sync {
    /// Show `notification`.
    fn notify(&self, notification: &Notification) -> HalResult<()>;
}

/// A platform without system notifications.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoNotifier;

impl Notifier for NoNotifier {
    fn notify(&self, _notification: &Notification) -> HalResult<()> {
        Err(HalError::Unsupported("system notifications"))
    }
}
