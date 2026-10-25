// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-iobroker`: the privileged SMBus broker (see the library docs).
//!
//! * `lightloom-iobroker service [--client-sid SID] [--modules DIR]`: run by
//!   the Service Control Manager as `LightloomIoBroker` (LocalSystem), as
//!   registered by `lightloom-setup --smbus`.
//! * `lightloom-iobroker console [--dry-run] [--client-sid SID] [--modules DIR]`:
//!   development only, from an elevated prompt; every write refused with
//!   `--dry-run`.
//! * `lightloom-iobroker detect [--modules DIR]`: report whether PawnIO is
//!   usable (elevated; read-only, loads only the Echo probe module).
//!
//! It never installs anything. Logs go to `%ProgramData%\Lightloom\logs\iobroker.log`
//! (service) or the console.

use std::process::ExitCode;

#[cfg(windows)]
use lightloom_iobroker::boot::BootOptions;

/// Parsed command line.
#[derive(Debug, Default)]
struct Args {
    command: String,
    dry_run: bool,
    client_sid: Option<String>,
    modules: Option<std::path::PathBuf>,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dry-run" => out.dry_run = true,
            "--client-sid" => out.client_sid = Some(it.next().ok_or("--client-sid needs a SID")?),
            "--modules" => out.modules = Some(it.next().ok_or("--modules needs a directory")?.into()),
            "-V" | "--version" => out.command = "version".into(),
            cmd if out.command.is_empty() && !cmd.starts_with('-') => out.command = cmd.into(),
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    Ok(out)
}

const USAGE: &str =
    "usage: lightloom-iobroker service|console|detect [--dry-run] [--client-sid SID] [--modules DIR]";

fn main() -> ExitCode {
    let args = match parse(std::env::args()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("lightloom-iobroker: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match args.command.as_str() {
        "version" => {
            println!(
                "lightloom-iobroker {} (broker protocol v{})",
                env!("CARGO_PKG_VERSION"),
                lightloom_protocol::broker::BROKER_PROTOCOL_VERSION
            );
            ExitCode::SUCCESS
        }
        #[cfg(windows)]
        "service" => win::service(args),
        #[cfg(windows)]
        "console" => win::console(args),
        #[cfg(windows)]
        "detect" => win::detect(args),
        _ => {
            eprintln!("{USAGE}");
            if !cfg!(windows) {
                eprintln!("(the broker exists on Windows only; Linux reaches /dev/i2c-* directly)");
            }
            ExitCode::from(2)
        }
    }
}

#[cfg(windows)]
fn boot_options(args: &Args) -> BootOptions {
    let mut opts = BootOptions {
        dry_run: args.dry_run,
        ..BootOptions::default()
    };
    if let Some(dir) = &args.modules {
        opts.modules_dir = dir.clone();
    }
    opts
}

#[cfg(windows)]
mod win {
    use super::*;
    use lightloom_iobroker::{boot, detect, pipe, SERVICE_NAME};
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::{define_windows_service, service_dispatcher};

    /// The command line, for the service entry point (SCM start arguments are separate).
    static ARGS: OnceLock<Args> = OnceLock::new();

    fn log_to_file() {
        let dir = std::env::var_os("ProgramData")
            .map_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"), Into::into)
            .join("Lightloom")
            .join("logs");
        let file = std::fs::create_dir_all(&dir).and_then(|()| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("iobroker.log"))
        });
        if let Ok(file) = file {
            let _ = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(file))
                .try_init();
        }
    }

    pub fn detect(args: Args) -> ExitCode {
        let _ = tracing_subscriber::fmt().try_init();
        let d = detect::probe(Some(&boot_options(&args).modules_dir));
        println!(
            "PawnIO: {:?}, code integrity options 0x{:08X}",
            d.status, d.ci_options
        );
        match d.status.problem() {
            Some(p) => {
                println!("{p}");
                ExitCode::from(1)
            }
            None => ExitCode::SUCCESS,
        }
    }

    pub fn console(args: Args) -> ExitCode {
        let _ = tracing_subscriber::fmt().try_init();
        eprintln!(
            "lightloom-iobroker console: development only{}",
            if args.dry_run { ", dry run" } else { "" }
        );
        let mut server = match boot::bootstrap(&boot_options(&args)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lightloom-iobroker: {e}");
                return ExitCode::from(1);
            }
        };
        let stop = AtomicBool::new(false);
        match pipe::serve(&mut server, args.client_sid.as_deref(), &stop) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("lightloom-iobroker: {e}");
                ExitCode::from(1)
            }
        }
    }

    pub fn service(args: Args) -> ExitCode {
        log_to_file();
        let _ = ARGS.set(args);
        match service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                tracing::error!("service dispatcher: {e}");
                eprintln!("lightloom-iobroker: {e} (the service mode is started by Windows)");
                ExitCode::from(1)
            }
        }
    }

    define_windows_service!(ffi_service_main, service_main);

    fn service_main(_start_args: Vec<OsString>) {
        if let Err(e) = run_service() {
            tracing::error!("service: {e}");
        }
    }

    fn status(state: ServiceState, exit: u32) -> ServiceStatus {
        ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(exit),
            checkpoint: 0,
            wait_hint: Duration::from_secs(2),
            process_id: None,
        }
    }

    fn run_service() -> windows_service::Result<()> {
        let args = ARGS.get().map(|a| (boot_options(a), a.client_sid.clone()));
        let (opts, sid) = args.unwrap_or_else(|| (BootOptions::default(), None));
        let stop = Arc::new(AtomicBool::new(false));
        let handler_stop = Arc::clone(&stop);
        let handle = service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                handler_stop.store(true, Ordering::SeqCst);
                pipe::wake();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
        handle.set_service_status(status(ServiceState::Running, 0))?;
        // PawnIO calls and the SMBus mutex live on this one worker thread.
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::Builder::new()
            .name("lightloom-iobroker".into())
            .spawn(move || -> Result<(), String> {
                let mut server = boot::bootstrap(&opts)?;
                pipe::serve(&mut server, sid.as_deref(), &worker_stop).map_err(|e| e.to_string())
            });
        let exit = match worker.map(|w| w.join()) {
            Ok(Ok(Ok(()))) => 0,
            Ok(Ok(Err(e))) => {
                tracing::error!("{e}");
                1
            }
            _ => {
                tracing::error!("broker worker thread failed");
                1
            }
        };
        handle.set_service_status(status(ServiceState::Stopped, exit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse(
            std::iter::once("lightloom-iobroker")
                .chain(list.iter().copied())
                .map(String::from),
        )
    }

    #[test]
    fn command_line() {
        let a = args(&["console", "--dry-run", "--client-sid", "S-1-5-21-1-2-3-1001"]).unwrap();
        assert_eq!(a.command, "console");
        assert!(a.dry_run);
        assert_eq!(a.client_sid.as_deref(), Some("S-1-5-21-1-2-3-1001"));
        assert_eq!(args(&["--version"]).unwrap().command, "version");
        assert!(args(&["service", "extra"]).is_err());
        assert!(args(&["service", "--client-sid"]).is_err());
    }
}
