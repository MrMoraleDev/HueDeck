// SPDX-License-Identifier: GPL-3.0-or-later
//! This OS's keyboard, mouse and desktop helpers for the macro runtime
//! (`lightloom_hal::input`): Windows `SendInput`, clipboard, `ShellExecuteW`,
//! idle time and the opt-in low-level hooks. Elsewhere nothing is available
//! yet (Linux: uinput / libinput later), and runs without hardware (tests,
//! `--no-hardware`) use `lightloom_hal::input::RecordingDesktop` instead, so
//! they never inject input.

use lightloom_hal::input::{Desktop, InputHook};
use std::sync::Arc;

/// This OS's [`Desktop`].
pub fn desktop() -> Arc<dyn Desktop> {
    #[cfg(windows)]
    return Arc::new(lightloom_hal_windows::input::WindowsDesktop);
    #[cfg(not(windows))]
    Arc::new(lightloom_hal::input::NoDesktop)
}

/// This OS's low-level keyboard and mouse hooks; `None` where there are none.
pub fn hooks() -> Option<Box<dyn InputHook>> {
    #[cfg(windows)]
    return Some(Box::new(lightloom_hal_windows::input::LowLevelHooks::new()));
    #[cfg(not(windows))]
    None
}
