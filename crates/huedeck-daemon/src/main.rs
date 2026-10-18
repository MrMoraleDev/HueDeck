// SPDX-License-Identifier: GPL-3.0-or-later
//! `huedeckd` — the HueDeck per-user daemon (unprivileged): devices, effects,
//! the local API and the dashboard. See `lib.rs`.

#![forbid(unsafe_code)]

use clap::Parser;
use huedeck_daemon::config::Config;
use huedeck_daemon::engine::HardwareMode;
use huedeck_daemon::paths::Paths;
use std::path::PathBuf;

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(
    name = "huedeckd",
    version,
    about = "HueDeck daemon (open source RGB manager)"
)]
struct Args {
    /// Configuration file (default: <config dir>/config.toml).
    #[arg(long, env = "HUEDECK_CONFIG")]
    config: Option<PathBuf>,

    /// Put data and configuration under this directory (also $HUEDECK_HOME).
    #[arg(long, value_name = "DIR")]
    home: Option<PathBuf>,

    /// Log filter (tracing EnvFilter syntax), overridden by RUST_LOG.
    #[arg(long, default_value = "info")]
    log: String,

    /// Never write to devices; log what would be sent.
    #[arg(long)]
    dry_run: bool,

    /// Do not touch any device (API, dashboard and effects only).
    #[arg(long)]
    no_hardware: bool,

    /// Keep settings in memory only (nothing is saved).
    #[arg(long)]
    ephemeral: bool,

    /// Open the dashboard in the default browser once the daemon listens.
    #[arg(long)]
    open: bool,
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
    let paths = match &args.home {
        Some(home) => Paths::under(home),
        None => Paths::from_env(),
    };
    let config_file = args.config.clone().unwrap_or_else(|| paths.config_file());
    let config = Config::load(&config_file)?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), config = %config_file.display(), dry_run = args.dry_run, "huedeckd");
    huedeck_daemon::run(huedeck_daemon::RunOptions {
        paths,
        config,
        dry_run: args.dry_run,
        hardware: if args.no_hardware {
            HardwareMode::None
        } else {
            HardwareMode::Real
        },
        ephemeral: args.ephemeral,
        open_dashboard: args.open,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
