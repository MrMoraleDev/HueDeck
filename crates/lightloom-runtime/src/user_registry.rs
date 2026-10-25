// SPDX-License-Identifier: GPL-3.0-or-later
//! This OS's per-user registry for the keys Lightloom writes on request (the
//! opt-in `lightloom://` URL scheme): `HKEY_CURRENT_USER` on Windows
//! (`lightloom_hal_windows::registry::CurrentUserRegistry`); elsewhere none
//! (a URL scheme is a `.desktop` file there, not done yet).

use lightloom_hal::UserRegistry;
use std::sync::Arc;

/// The per-user registry of this OS, if it has one.
pub fn system_registry() -> Option<Arc<dyn UserRegistry>> {
    #[cfg(windows)]
    {
        Some(Arc::new(lightloom_hal_windows::registry::CurrentUserRegistry))
    }
    #[cfg(not(windows))]
    {
        None
    }
}
