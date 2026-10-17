// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `lightloomd` — the Lightloom per-user daemon (unprivileged).
//!
//! M0 skeleton: argument parsing, tracing, version banner and the lifecycle
//! state machine type. Hardware scanning, the plugin host, renderer client
//! and local API are wired in from M1 on.

#![forbid(unsafe_code)]

use clap::Parser;
use std::fmt;
use std::path::PathBuf;
use tracing::info;

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

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(name = "lightloomd", version, about = "Lightloom daemon (open source RGB manager)")]
struct Args {
    /// Path to the configuration file (default: platform config dir / lightloom / config.toml).
    #[arg(long, env = "LIGHTLOOM_CONFIG")]
    config: Option<PathBuf>,

    /// Log filter (tracing EnvFilter syntax), overridden by RUST_LOG.
    #[arg(long, default_value = "info")]
    log: String,

    /// Never write to hardware; log what would be sent.
    #[arg(long)]
    dry_run: bool,
}

fn default_config_path() -> PathBuf {
    let base = std::env::var_os("LIGHTLOOM_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Lightloom")))
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("lightloom")))
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config").join("lightloom")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("config.toml")
}

fn init_tracing(default_filter: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(default_filter))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_tracing(&args.log);

    let config = args.config.clone().unwrap_or_else(default_config_path);
    println!("lightloomd {}", env!("CARGO_PKG_VERSION"));
    info!(version = env!("CARGO_PKG_VERSION"), config = %config.display(), dry_run = args.dry_run, "starting");

    let mut state = LifecycleState::Booting;
    while let Some(next) = state.next_on_boot() {
        info!(from = %state, to = %next, "lifecycle");
        state = next;
        if state == LifecycleState::LoadingConfig && !config.exists() {
            info!(path = %config.display(), "no config file yet; using defaults");
        }
    }
    // TODO(M1): keep running (hotplug, frame loop, API); for now exit once the
    // boot path completes so the binary is useful for `--version` and smoke tests.
    info!(state = %state, "boot path complete; nothing to serve yet, exiting");
    Ok(())
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

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
