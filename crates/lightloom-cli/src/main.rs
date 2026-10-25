// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom` — command-line client.
//!
//! Headless commands (no daemon yet): `devices` (read-only HID enumeration),
//! `light` / `rainbow` / `bench` (null renderer → plugin sessions → hardware,
//! see [`run`]) and `compat dry-run` (corpus harness). `url` prints where the
//! daemon serves (`--addr`, else the `api-port` file lightloomd writes, else
//! 127.0.0.1:53000). `action <url>` hands a `lightloom://` action URL to the
//! running daemon (`POST /v1/actions`). `udev generate` writes the Linux udev rules for the
//! devices the plugins claim.
//!
//! Exit codes: `0` success; `1` a device session failed, faulted or saw I/O
//! errors (HID or SMBus); `2` a setup error (no plugin or device, conflicting software, bad
//! arguments).

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use std::process::ExitCode;

mod action;
mod compat;
mod devices;
mod run;
mod udev;

#[derive(Debug, Parser)]
#[command(
    name = "lightloom",
    version,
    about = "Lightloom command-line client",
    propagate_version = true
)]
struct Cli {
    /// Daemon address (host:port). Default: the port lightloomd serves on (the
    /// `api-port` file in its data directory), else 127.0.0.1:53000.
    #[arg(long, env = "LIGHTLOOM_ADDR")]
    addr: Option<String>,

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
        color: lightloom_engine::Rgb,
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
    /// Render an effect (lightscript) with the CEF renderer onto every supported device until Ctrl-C.
    Effect {
        #[command(flatten)]
        effect: run::EffectArgs,
        #[command(flatten)]
        run: run::RunArgs,
    },
    /// List the effects in a directory and their controls.
    Effects {
        /// Effect directories (default: ./effects).
        dirs: Vec<std::path::PathBuf>,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
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
        #[arg(long, value_name = "VID:PID", value_parser = run::parse_vid_pid, conflicts_with = "mock_smbus")]
        mock: Option<(u16, u16)>,
        /// Run this SMBus plugin (its name or directory, e.g. msi-gpu or crucial-ballistix)
        /// on a scripted in-memory bus instead of hardware (CI, no device needed).
        #[arg(long, value_name = "PLUGIN")]
        mock_smbus: Option<String>,
        #[command(flatten)]
        run: run::RunArgs,
    },
    /// List the plugins found and which one would drive each connected device (HID: read-only).
    /// The SMBus buses are listed and scanned with the SMBus plugins too (a plugin's Scan()
    /// may write, e.g. to give a RAM module its address), unless --no-smbus; never while
    /// SignalRGB, OpenRGB or lightloomd runs.
    Plugins {
        #[command(flatten)]
        catalog: run::CatalogArgs,
        /// Leave the SMBus buses alone (no NvAPI, no ADL, no I/O broker, no Scan()).
        #[arg(long)]
        no_smbus: bool,
        /// Scan the SMBus buses without writing: the scans' writes are recorded, never sent
        /// (reads still reach the devices; a RAM module that needs a new address is reported).
        #[arg(long, conflicts_with = "no_smbus")]
        dry_run: bool,
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Print the address of the daemon's dashboard and API (--addr / LIGHTLOOM_ADDR, else the
    /// port lightloomd serves on, else the default 127.0.0.1:53000).
    Url,
    /// Run a lightloom:// action URL in the running daemon (e.g. lightloom://effect/apply/Rainbow,
    /// lightloom://brightness/50, lightloom://view/devices); see docs/integrations/actions.md.
    Action {
        /// The URL (or a bare action path such as layout/apply/Desk).
        url: String,
        /// Print the daemon's JSON answer.
        #[arg(long)]
        json: bool,
    },
    /// SignalRGB-compatibility tooling.
    Compat {
        #[command(subcommand)]
        command: compat::CompatCommand,
    },
    /// Linux udev rules for the devices the plugins claim.
    Udev {
        #[command(subcommand)]
        command: UdevCommand,
    },
}

#[derive(Debug, Subcommand)]
enum UdevCommand {
    /// Print (or write) 70-lightloom.rules from the plugin manifests.
    Generate {
        /// Plugin directories (a plugin's own directory or one holding plugins; repeatable).
        /// Default: LIGHTLOOM_PLUGINS, else ./plugins.
        #[arg(long = "plugins", value_name = "DIR")]
        plugins: Vec<std::path::PathBuf>,
        /// Write the rules to this file instead of standard output.
        #[arg(long, value_name = "FILE")]
        out: Option<std::path::PathBuf>,
        /// Write nothing; exit 1 when --out differs from what would be written.
        #[arg(long, requires = "out")]
        check: bool,
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

/// The daemon's `host:port`: `explicit` (`--addr` / `LIGHTLOOM_ADDR`), else the
/// port lightloomd wrote to `api-port` in `data_dir`, else the default port.
fn daemon_addr(explicit: Option<&str>, data_dir: &std::path::Path) -> String {
    if let Some(addr) = explicit.map(str::trim).filter(|a| !a.is_empty()) {
        return addr.to_string();
    }
    let port = lightloom_runtime::dirs::read_api_port(data_dir).unwrap_or(lightloom_api::NATIVE_PORT);
    format!("{}:{port}", lightloom_api::BIND_ADDRESS)
}

fn dispatch(cli: Cli) -> anyhow::Result<run::Outcome> {
    match cli.command {
        Command::Url => {
            let (data, _) = lightloom_runtime::dirs::from_env();
            println!("http://{}/", daemon_addr(cli.addr.as_deref(), &data));
            Ok(run::Outcome::Clean)
        }
        Command::Action { url, json } => {
            let (data, _) = lightloom_runtime::dirs::from_env();
            let addr = daemon_addr(cli.addr.as_deref(), &data);
            action::run(&addr, &data, &url, json).map(|ok| {
                if ok {
                    run::Outcome::Clean
                } else {
                    run::Outcome::Failed
                }
            })
        }
        Command::Devices { json, filters } => run_devices(json, &filters).map(|_| run::Outcome::Clean),
        Command::Light { color, run } => run::run(run::Mode::Light(color), &run),
        Command::Rainbow { fps, run } => run::run(run::Mode::Rainbow { fps }, &run),
        Command::Effect { effect, run } => run::run(run::Mode::Effect(effect), &run),
        Command::Effects { dirs, json } => run::list_effects(&dirs, json).map(|_| run::Outcome::Clean),
        Command::Bench {
            seconds,
            fps,
            mock,
            mock_smbus,
            run,
        } => run::run(
            run::Mode::Bench {
                fps,
                seconds,
                mock,
                mock_smbus,
            },
            &run,
        ),
        Command::Plugins {
            catalog,
            no_smbus,
            dry_run,
            json,
        } => run::list_plugins(&catalog, !no_smbus, dry_run, json).map(|_| run::Outcome::Clean),
        Command::Udev {
            command: UdevCommand::Generate { plugins, out, check },
        } => {
            let dirs = if plugins.is_empty() {
                run::default_plugin_dirs()
            } else {
                plugins
            };
            udev::generate(&dirs, out.as_deref(), check).map(|clean| {
                if clean {
                    run::Outcome::Clean
                } else {
                    run::Outcome::Failed
                }
            })
        }
        Command::Compat { command } => compat::run(command).map(|clean| {
            if clean {
                run::Outcome::Clean
            } else {
                run::Outcome::Failed
            }
        }),
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LIGHTLOOM_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(run::Outcome::Clean) => ExitCode::SUCCESS,
        Ok(run::Outcome::Failed) => ExitCode::from(1),
        Err(e) => {
            eprintln!("lightloom: {e:#}");
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
            "lightloom",
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
                assert_eq!(color, lightloom_engine::Rgb::new(255, 0, 0));
                assert!(run.dry_run);
                assert_eq!(run.devices, vec!["1462:7c91".to_string()]);
                assert!(!run.catalog.signalrgb);
                assert_eq!(
                    run.restore_check,
                    vec![run::RestoreArg::Report(
                        lightloom_plugin_host::session::RestoreProbe {
                            report_id: 0x52,
                            len: 185
                        }
                    )]
                );
                assert_eq!(run.params.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }
        let cli = Cli::try_parse_from(["lightloom", "rainbow", "--fps", "60", "--duration", "600"]).unwrap();
        assert!(
            matches!(cli.command, Command::Rainbow { fps, ref run } if fps == 60.0 && run.duration == Some(600.0))
        );
        let cli = Cli::try_parse_from(["lightloom", "bench", "--mock", "1462:7c91", "--seconds", "2"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Bench {
                mock: Some((0x1462, 0x7c91)),
                ..
            }
        ));
        // `--restore-check` alone means every declared feature report; it never eats the next token.
        let cli = Cli::try_parse_from(["lightloom", "rainbow", "--restore-check", "--fps", "60"]).unwrap();
        assert!(
            matches!(cli.command, Command::Rainbow { ref run, .. } if run.restore_check == vec![run::RestoreArg::Auto])
        );
        assert!(Cli::try_parse_from(["lightloom", "light", "notacolour"]).is_err());
        let cli =
            Cli::try_parse_from(["lightloom", "plugins", "--signalrgb", "--plugins", "x", "--json"]).unwrap();
        assert!(
            matches!(cli.command, Command::Plugins { ref catalog, no_smbus: false, dry_run: false, json: true } if catalog.signalrgb && catalog.plugin_dirs.len() == 1)
        );
        // Out-of-range times and rates are rejected before anything starts.
        assert!(Cli::try_parse_from(["lightloom", "light", "red", "--duration", "1e30"]).is_err());
        assert!(Cli::try_parse_from(["lightloom", "light", "red", "--duration", "0"]).is_err());
        assert!(Cli::try_parse_from(["lightloom", "light", "red", "--stats-every", "NaN"]).is_err());
        assert!(Cli::try_parse_from(["lightloom", "bench", "--seconds", "inf"]).is_err());
        assert!(Cli::try_parse_from(["lightloom", "rainbow", "--fps", "0"]).is_err());
        assert!(Cli::try_parse_from(["lightloom", "rainbow", "--fps", "1000"]).is_err());
        assert!(Cli::try_parse_from(["lightloom", "light", "red", "--brightness", "101"]).is_err());
    }

    #[test]
    fn smbus_is_on_unless_no_smbus() {
        let cli = Cli::try_parse_from(["lightloom", "light", "red", "--dry-run"]).unwrap();
        assert!(matches!(cli.command, Command::Light { ref run, .. } if !run.no_smbus && run.dry_run));
        let cli = Cli::try_parse_from(["lightloom", "rainbow", "--no-smbus"]).unwrap();
        assert!(matches!(cli.command, Command::Rainbow { ref run, .. } if run.no_smbus));
        assert!(
            Cli::try_parse_from(["lightloom", "light", "red", "--smbus"]).is_err(),
            "the old opt-in flag is gone"
        );
        let cli = Cli::try_parse_from(["lightloom", "plugins"]).unwrap();
        assert!(matches!(cli.command, Command::Plugins { no_smbus: false, .. }));
        let cli = Cli::try_parse_from(["lightloom", "plugins", "--no-smbus"]).unwrap();
        assert!(matches!(cli.command, Command::Plugins { no_smbus: true, .. }));
        let cli = Cli::try_parse_from(["lightloom", "plugins", "--dry-run"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Plugins {
                no_smbus: false,
                dry_run: true,
                ..
            }
        ));
        assert!(Cli::try_parse_from(["lightloom", "plugins", "--dry-run", "--no-smbus"]).is_err());
        let cli = Cli::try_parse_from(["lightloom", "bench", "--mock-smbus", "msi-gpu"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Bench { mock_smbus: Some(ref p), mock: None, .. } if p == "msi-gpu"
        ));
        assert!(Cli::try_parse_from(["lightloom", "bench", "--mock-smbus", "x", "--mock", "1:2"]).is_err());
    }

    #[test]
    fn the_daemon_address_comes_from_the_flag_then_the_port_file_then_the_default() {
        let dir = std::env::temp_dir().join(format!("lightloom-cli-addr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(daemon_addr(None, &dir), "127.0.0.1:53000");
        lightloom_runtime::dirs::write_api_port(&dir, 17000).unwrap();
        assert_eq!(daemon_addr(None, &dir), "127.0.0.1:17000");
        assert_eq!(daemon_addr(Some(" "), &dir), "127.0.0.1:17000");
        assert_eq!(daemon_addr(Some("127.0.0.1:18000"), &dir), "127.0.0.1:18000");
        lightloom_runtime::dirs::remove_api_port(&dir, 17000);
        assert_eq!(daemon_addr(None, &dir), "127.0.0.1:53000");
        let _ = std::fs::remove_dir_all(&dir);

        let cli = Cli::try_parse_from(["lightloom", "--addr", "127.0.0.1:1234", "url"]).unwrap();
        assert_eq!(cli.addr.as_deref(), Some("127.0.0.1:1234"));
        assert!(matches!(cli.command, Command::Url));
    }

    #[test]
    fn action_parses() {
        let cli = Cli::try_parse_from(["lightloom", "action", "lightloom://brightness/40", "--json"]).unwrap();
        assert!(
            matches!(cli.command, Command::Action { ref url, json: true } if url == "lightloom://brightness/40")
        );
        assert!(Cli::try_parse_from(["lightloom", "action"]).is_err());
    }

    #[test]
    fn udev_flags_parse() {
        let cli = Cli::try_parse_from([
            "lightloom",
            "udev",
            "generate",
            "--plugins",
            "p",
            "--out",
            "r",
            "--check",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Udev {
                command: UdevCommand::Generate { ref plugins, out: Some(_), check: true }
            } if plugins.len() == 1
        ));
        assert!(Cli::try_parse_from(["lightloom", "udev", "generate", "--check"]).is_err());
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
