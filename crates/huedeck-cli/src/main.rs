// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `huedeck` — command-line client.
//!
//! Headless commands (no daemon yet): `devices` (read-only HID enumeration),
//! `light` / `rainbow` / `bench` (null renderer → plugin sessions → hardware,
//! see [`run`]) and `compat dry-run` (corpus harness).
//!
//! Exit codes: `0` success; `1` a device session failed, faulted or saw HID
//! errors; `2` a setup error (no plugin or device, conflicting software, bad
//! arguments).

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use std::process::ExitCode;

mod compat;
mod devices;
mod run;

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
        /// Only show devices matching `vid[:pid]` (hex; repeatable).
        #[arg(long = "filter", value_name = "VID[:PID]")]
        filters: Vec<devices::Filter>,
    },
    /// Set every supported device (or the --device ones) to a solid colour until Ctrl-C.
    Light {
        /// Colour as #RRGGBB or a basic name (red, green, blue, white, ...).
        #[arg(value_parser = run::parse_color)]
        color: huedeck_engine::Rgb,
        #[command(flatten)]
        run: run::RunArgs,
    },
    /// Run the built-in rainbow pattern through the null renderer until Ctrl-C.
    Rainbow {
        /// Frames per second (1-240).
        #[arg(long, default_value_t = 30.0, value_parser = run::parse_fps)]
        fps: f64,
        #[command(flatten)]
        run: run::RunArgs,
    },
    /// Drive the rainbow pattern for a fixed time and report latency percentiles.
    Bench {
        /// Seconds to run.
        #[arg(long, default_value_t = 10.0, value_parser = run::parse_secs)]
        seconds: f64,
        /// Frames per second to drive (1-240).
        #[arg(long, default_value_t = 30.0, value_parser = run::parse_fps)]
        fps: f64,
        /// Use a scripted in-memory device with this VID:PID instead of hardware (CI, no device needed).
        #[arg(long, value_name = "VID:PID", value_parser = run::parse_vid_pid)]
        mock: Option<(u16, u16)>,
        #[command(flatten)]
        run: run::RunArgs,
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

fn dispatch(cli: Cli) -> anyhow::Result<run::Outcome> {
    match cli.command {
        Command::Devices { json, filters } => run_devices(json, &filters).map(|_| run::Outcome::Clean),
        Command::Light { color, run } => run::run(run::Mode::Light(color), &run),
        Command::Rainbow { fps, run } => run::run(run::Mode::Rainbow { fps }, &run),
        Command::Bench {
            seconds,
            fps,
            mock,
            run,
        } => run::run(run::Mode::Bench { fps, seconds, mock }, &run),
        Command::Compat { command } => compat::run(command).map(|_| run::Outcome::Clean),
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("HUEDECK_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(run::Outcome::Clean) => ExitCode::SUCCESS,
        Ok(run::Outcome::Failed) => ExitCode::from(1),
        Err(e) => {
            eprintln!("huedeck: {e:#}");
            ExitCode::from(2)
        }
    }
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
    fn run_commands_parse() {
        let cli = Cli::try_parse_from([
            "huedeck",
            "light",
            "#ff0000",
            "--dry-run",
            "--device",
            "1462:7c91",
            "--restore-check=0x52:185",
            "--param",
            "LightingMode=Forced",
        ])
        .unwrap();
        match cli.command {
            Command::Light { color, run } => {
                assert_eq!(color, huedeck_engine::Rgb::new(255, 0, 0));
                assert!(run.dry_run);
                assert_eq!(run.devices, vec!["1462:7c91".to_string()]);
                assert_eq!(
                    run.restore_check,
                    vec![run::RestoreArg::Report(
                        huedeck_plugin_host::session::RestoreProbe {
                            report_id: 0x52,
                            len: 185
                        }
                    )]
                );
                assert_eq!(run.params.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }
        let cli = Cli::try_parse_from(["huedeck", "rainbow", "--fps", "60", "--duration", "600"]).unwrap();
        assert!(
            matches!(cli.command, Command::Rainbow { fps, ref run } if fps == 60.0 && run.duration == Some(600.0))
        );
        let cli = Cli::try_parse_from(["huedeck", "bench", "--mock", "1462:7c91", "--seconds", "2"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Bench {
                mock: Some((0x1462, 0x7c91)),
                ..
            }
        ));
        // `--restore-check` alone means every declared feature report; it never eats the next token.
        let cli = Cli::try_parse_from(["huedeck", "rainbow", "--restore-check", "--fps", "60"]).unwrap();
        assert!(
            matches!(cli.command, Command::Rainbow { ref run, .. } if run.restore_check == vec![run::RestoreArg::Auto])
        );
        assert!(Cli::try_parse_from(["huedeck", "light", "notacolour"]).is_err());
        // Out-of-range times and rates are rejected before anything starts.
        assert!(Cli::try_parse_from(["huedeck", "light", "red", "--duration", "1e30"]).is_err());
        assert!(Cli::try_parse_from(["huedeck", "light", "red", "--duration", "0"]).is_err());
        assert!(Cli::try_parse_from(["huedeck", "light", "red", "--stats-every", "NaN"]).is_err());
        assert!(Cli::try_parse_from(["huedeck", "bench", "--seconds", "inf"]).is_err());
        assert!(Cli::try_parse_from(["huedeck", "rainbow", "--fps", "0"]).is_err());
        assert!(Cli::try_parse_from(["huedeck", "rainbow", "--fps", "1000"]).is_err());
        assert!(Cli::try_parse_from(["huedeck", "light", "red", "--brightness", "101"]).is_err());
    }

    #[test]
    fn devices_flags_parse() {
        let cli = Cli::try_parse_from([
            "huedeck",
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
        assert!(Cli::try_parse_from(["huedeck", "devices", "--filter", "nothex"]).is_err());
    }
}
