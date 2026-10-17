// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `huedeck` — command-line client.
//!
//! M0 skeleton: the subcommand tree is final, every handler returns
//! [`NotImplemented`] naming the milestone that fills it in.

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Placeholder error for commands that land in a later milestone.
#[derive(Debug, thiserror::Error)]
#[error("`huedeck {command}` is not implemented yet (planned for milestone {milestone})")]
pub struct NotImplemented {
    /// Subcommand name.
    pub command: &'static str,
    /// Milestone.
    pub milestone: &'static str,
}

#[derive(Debug, Parser)]
#[command(
    name = "huedeck",
    version,
    about = "HueDeck command-line client",
    propagate_version = true
)]
struct Cli {
    /// Daemon address (host:port) when not the default loopback port.
    #[arg(long, env = "HUEDECK_ADDR", default_value = "127.0.0.1:16138")]
    addr: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List detected devices and their endpoints.
    Devices {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Set every device (or one) to a solid colour.
    Light {
        /// Colour as #RRGGBB or a CSS name.
        color: String,
        /// Restrict to one device uid (vid:pid:serial).
        #[arg(long)]
        device: Option<String>,
        /// Do not write to hardware; print the packets instead.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the built-in rainbow pattern through the null renderer.
    Rainbow {
        /// Frames per second.
        #[arg(long, default_value_t = 30.0)]
        fps: f64,
        /// Stop after this many seconds (default: until Ctrl-C).
        #[arg(long)]
        duration: Option<u64>,
    },
    /// Measure frame / sample / write latency histograms.
    Bench {
        /// Seconds to run.
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        /// Frames per second to drive.
        #[arg(long, default_value_t = 30.0)]
        fps: f64,
    },
    /// SignalRGB-compatibility tooling.
    Compat {
        #[command(subcommand)]
        command: CompatCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CompatCommand {
    /// Load every plugin in a directory with stubbed host objects and report API gaps.
    DryRun {
        /// Plugin directory (e.g. the user's SignalRGB plugin cache).
        dir: PathBuf,
        /// Also run Initialize() under the harness.
        #[arg(long)]
        initialize: bool,
    },
}

fn run(cli: Cli) -> Result<(), NotImplemented> {
    let (command, milestone) = match cli.command {
        Command::Devices { .. } => ("devices", "M1"),
        Command::Light { .. } => ("light", "M1"),
        Command::Rainbow { .. } => ("rainbow", "M1"),
        Command::Bench { .. } => ("bench", "M1"),
        Command::Compat {
            command: CompatCommand::DryRun { .. },
        } => ("compat dry-run", "M2"),
    };
    Err(NotImplemented { command, milestone })
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run(cli)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_subcommand_names_its_milestone() {
        let cases = [
            (vec!["huedeck", "devices"], "M1"),
            (vec!["huedeck", "light", "#ff0000"], "M1"),
            (vec!["huedeck", "rainbow", "--fps", "60"], "M1"),
            (vec!["huedeck", "bench"], "M1"),
            (vec!["huedeck", "compat", "dry-run", "C:/plugins"], "M2"),
        ];
        for (argv, milestone) in cases {
            let cli = Cli::try_parse_from(argv.iter().copied()).unwrap();
            let err = run(cli).unwrap_err();
            assert_eq!(err.milestone, milestone, "{argv:?}");
            assert!(err.to_string().contains(milestone));
        }
    }
}
