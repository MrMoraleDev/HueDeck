// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `lightloom` — command-line client.
//!
//! The subcommand tree is final; handlers not implemented yet return
//! [`NotImplemented`] naming the milestone that fills them in.
//! `devices` is live (read-only HID enumeration).

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};

mod compat;
mod devices;

/// Placeholder error for commands that land in a later milestone.
#[derive(Debug, thiserror::Error)]
#[error("`lightloom {command}` is not implemented yet (planned for milestone {milestone})")]
pub struct NotImplemented {
    /// Subcommand name.
    pub command: &'static str,
    /// Milestone.
    pub milestone: &'static str,
}

#[derive(Debug, Parser)]
#[command(
    name = "lightloom",
    version,
    about = "Lightloom command-line client",
    propagate_version = true
)]
struct Cli {
    /// Daemon address (host:port) when not the default loopback port.
    #[arg(long, env = "LIGHTLOOM_ADDR", default_value = "127.0.0.1:16138")]
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
        /// Only show devices matching `vid[:pid]` (hex; repeatable).
        #[arg(long = "filter", value_name = "VID[:PID]")]
        filters: Vec<devices::Filter>,
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
        command: compat::CompatCommand,
    },
}

fn run_devices(json: bool, filters: &[devices::Filter]) -> anyhow::Result<()> {
    let groups = devices::apply_filters(devices::enumerate()?, filters);
    if json {
        println!("{}", serde_json::to_string_pretty(&devices::to_json(&groups))?);
    } else {
        println!("{}", devices::render_table(&groups));
    }
    Ok(())
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let (command, milestone) = match cli.command {
        Command::Devices { json, filters } => return run_devices(json, &filters),
        Command::Light { .. } => ("light", "M1"),
        Command::Rainbow { .. } => ("rainbow", "M1"),
        Command::Bench { .. } => ("bench", "M1"),
        Command::Compat { command } => return compat::run(command),
    };
    Err(NotImplemented { command, milestone }.into())
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
            (vec!["lightloom", "light", "#ff0000"], "M1"),
            (vec!["lightloom", "rainbow", "--fps", "60"], "M1"),
            (vec!["lightloom", "bench"], "M1"),
        ];
        for (argv, milestone) in cases {
            let cli = Cli::try_parse_from(argv.iter().copied()).unwrap();
            let err = run(cli).unwrap_err();
            let err = err.downcast_ref::<NotImplemented>().expect("NotImplemented");
            assert_eq!(err.milestone, milestone, "{argv:?}");
            assert!(err.to_string().contains(milestone));
        }
    }

    #[test]
    fn devices_flags_parse() {
        let cli = Cli::try_parse_from([
            "lightloom",
            "devices",
            "--json",
            "--filter",
            "1462:7c91",
            "--filter",
            "1532",
        ])
        .unwrap();
        match cli.command {
            Command::Devices { json, filters } => {
                assert!(json);
                assert_eq!(filters.len(), 2);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(Cli::try_parse_from(["lightloom", "devices", "--filter", "nothex"]).is_err());
    }
}
