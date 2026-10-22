// SPDX-License-Identifier: GPL-3.0-or-later
//! The daemon's lifecycle states (docs/architecture.md §3).

use std::fmt;

/// Lifecycle states (synthesis §3; the approved plan adds Suspending/Resuming).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LifecycleState {
    /// Process started, nothing loaded.
    Booting,
    /// Reading config and persistence.
    LoadingConfig,
    /// Checking whether the optional io-broker service is installed and healthy.
    BrokerProbe,
    /// Checking the CEF renderer binary.
    RendererProbe,
    /// Enumerating HID / USB / serial / SMBus / LampArray / sensors.
    ScanningHardware,
    /// Importing only the plugins whose VID:PID is present (index cache).
    LoadingPlugins,
    /// Parsing effects metadata.
    LoadingEffects,
    /// Steady state.
    Running,
    /// `Shutdown(true)` on every plugin (system sleep).
    Suspending,
    /// Re-`Initialize()` after resume.
    Resuming,
    /// Orderly exit: stop seq → `Shutdown(false)` → close handles → stop renderer.
    ShuttingDown,
}

impl LifecycleState {
    /// The state that normally follows this one on the boot path.
    pub fn next_on_boot(self) -> Option<LifecycleState> {
        use LifecycleState::*;
        Some(match self {
            Booting => LoadingConfig,
            LoadingConfig => BrokerProbe,
            BrokerProbe => RendererProbe,
            RendererProbe => ScanningHardware,
            ScanningHardware => LoadingPlugins,
            LoadingPlugins => LoadingEffects,
            LoadingEffects => Running,
            Running | Suspending | Resuming | ShuttingDown => return None,
        })
    }
}

impl fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_path_reaches_running() {
        let mut s = LifecycleState::Booting;
        let mut steps = 0;
        while let Some(n) = s.next_on_boot() {
            s = n;
            steps += 1;
        }
        assert_eq!(s, LifecycleState::Running);
        assert_eq!(steps, 7);
        assert_eq!(LifecycleState::Suspending.next_on_boot(), None);
    }
}
