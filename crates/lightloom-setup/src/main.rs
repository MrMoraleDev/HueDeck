// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-setup`: the elevated helper of RAM lighting. It installs the
//! PawnIO driver (verified download of the official installer) and registers
//! the Lightloom I/O broker.
//!
//! RAM lighting needs the chipset SMBus, which only a privileged process may
//! reach (through the PawnIO driver). This runs elevated, behind one UAC
//! prompt, and only when the user opts in: the installer's checkbox, the
//! dashboard's Settings → PawnIO / RAM lighting buttons, or by hand:
//!
//! ```text
//! lightloom-setup --pawnio-install [--smbus] [--elevate] [--pause] [--plan]
//! lightloom-setup --smbus [--pawnio-modules <dir>] [--client-sid SID] [--elevate] [--pause] [--plan]
//! lightloom-setup --smbus --uninstall [--elevate] [--pause] [--plan]
//! ```
//!
//! `--pawnio-install` ([`pawnio_setup`]): when PawnIO is missing, older than
//! 2.2.0 or unreadable, download the official `PawnIO_setup.exe` 2.2.0 from
//! its GitHub release into `%ProgramFiles%\Lightloom\downloads` (writable by
//! administrators only), check its size, SHA-256 and Authenticode signer
//! (`namazso.eu`) against [`lightloom_iobroker::pins::PAWNIO_SETUP`], run it
//! with `-install -silent`, delete it, and read PawnIO's version again. When
//! the download or a check fails (offline, say) nothing is changed and the
//! exit code is [`exit::PAWNIO_UNAVAILABLE`]: RAM lighting stays off, the
//! dashboard offers to try again. This is the only network access Lightloom
//! makes on its own (docs/privacy.md).
//!
//! `--smbus` (install):
//! 1. check the PawnIO driver: installed, official edition, 2.2.0 or later
//!    (without `--pawnio-install` this tool does not install it and stops);
//! 2. check `SmbusPIIX4.bin`, `SmbusI801.bin` and `Echo.bin` in the modules directory (default:
//!    `pawnio-modules` next to this executable, where the Lightloom installer
//!    puts the signed PawnIO.Modules 0.2.11 release) against their SHA-256
//!    pins, and copy them with `COPYING` and the module sources (LGPL-2.1) to
//!    `%ProgramFiles%\Lightloom\pawnio\0.2.11\`;
//! 3. stop and delete an existing `LightloomIoBroker` (an update), copy
//!    `lightloom-iobroker.exe` to `%ProgramFiles%\Lightloom\` (a LocalSystem
//!    service must not run from a folder its user can write), and check that
//!    the copy is byte for byte the file hashed before copying (its SHA-256 is
//!    printed, to compare with the release's);
//! 4. register `LightloomIoBroker`: LocalSystem, automatic start, restart on
//!    failure, unrestricted service SID, the pipe opened to the interactive
//!    user only (`--client-sid`, default: the user of this process's token);
//! 5. start it.
//!
//! Nothing comes from the environment or the search path: Program Files is
//! read from `HKLM` ([`lightloom_iobroker::boot::program_files`]), the SID from
//! the process token.
//!
//! `--uninstall` stops and deletes the service and removes those files. It
//! never uninstalls PawnIO (other programs use it). `--plan` prints the steps
//! and changes nothing. `--elevate` starts this same command through the UAC
//! prompt when not elevated yet (and waits for it); `--pause` keeps the
//! window open on an error.

// Windows-only tool: elsewhere it only builds (CI), so its steps go unused.
#![cfg_attr(not(windows), allow(dead_code))]

mod pawnio_setup;

#[cfg(windows)]
mod net;
#[cfg(windows)]
mod trust;

use lightloom_hal_windows::pawnio::exit;
use lightloom_iobroker::detect::PawnIoStatus;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// One thing the broker setup does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    /// Copy the pinned module blobs, `COPYING` and their sources.
    CopyModules { from: PathBuf, to: PathBuf },
    /// Copy the broker executable to its protected place.
    CopyBroker { from: PathBuf, to: PathBuf },
    /// Register the service.
    RegisterService {
        exe: PathBuf,
        modules: PathBuf,
        client_sid: String,
    },
    /// Start it.
    StartService,
    /// Stop it (uninstall, or before an update) and wait until it stopped.
    StopService,
    /// Delete it (uninstall, or before an update).
    DeleteService,
    /// Remove the broker, the module copies and the download folder (uninstall).
    RemoveFiles { broker: PathBuf, modules: PathBuf },
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Step::CopyModules { from, to } => write!(
                f,
                "copy the pinned PawnIO modules {} -> {}",
                from.display(),
                to.display()
            ),
            Step::CopyBroker { from, to } => write!(f, "copy {} -> {}", from.display(), to.display()),
            Step::RegisterService { exe, client_sid, .. } => write!(
                f,
                "register {} (LocalSystem, automatic start) running {} for user {client_sid}",
                lightloom_iobroker::SERVICE_NAME,
                exe.display()
            ),
            Step::StartService => write!(f, "start {}", lightloom_iobroker::SERVICE_NAME),
            Step::StopService => write!(f, "stop {} (if it runs)", lightloom_iobroker::SERVICE_NAME),
            Step::DeleteService => write!(f, "delete {} (if registered)", lightloom_iobroker::SERVICE_NAME),
            Step::RemoveFiles { broker, modules } => {
                write!(
                    f,
                    "remove {} and {} (PawnIO itself stays)",
                    broker.display(),
                    modules.display()
                )
            }
        }
    }
}

/// Where things go.
#[derive(Clone, Debug)]
struct Layout {
    /// `%ProgramFiles%\Lightloom`.
    root: PathBuf,
    /// The broker executable shipped next to this tool.
    broker_source: PathBuf,
}

impl Layout {
    fn modules(&self) -> PathBuf {
        self.root
            .join("pawnio")
            .join(lightloom_iobroker::pins::MODULES_RELEASE)
    }

    fn broker(&self) -> PathBuf {
        self.root.join("lightloom-iobroker.exe")
    }

    /// Where `--pawnio-install` puts the installer while it checks and runs it.
    fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }
}

/// The modules directory the Lightloom installer lays out next to this tool.
fn bundled_modules(exe_dir: &Path) -> PathBuf {
    exe_dir.join("pawnio-modules")
}

/// What to do.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Install {
        modules_from: PathBuf,
        client_sid: String,
    },
    Uninstall,
}

/// The steps for `action`, given what PawnIO detection found. An install
/// stops when PawnIO is not usable. An install over an existing service
/// replaces it (stop, delete, copy, register, start).
fn plan_steps(action: &Action, pawnio: PawnIoStatus, layout: &Layout) -> Result<Vec<Step>, String> {
    match action {
        Action::Uninstall => Ok(vec![
            Step::StopService,
            Step::DeleteService,
            Step::RemoveFiles {
                broker: layout.broker(),
                modules: layout.modules(),
            },
        ]),
        Action::Install {
            modules_from,
            client_sid,
        } => {
            if let Some(problem) = pawnio.problem() {
                return Err(format!(
                    "{problem}. RAM lighting needs it: install the official edition of PawnIO \
                     (https://pawnio.eu, or `lightloom-setup --pawnio-install`), then run this again."
                ));
            }
            Ok(vec![
                Step::StopService,
                Step::DeleteService,
                Step::CopyModules {
                    from: modules_from.clone(),
                    to: layout.modules(),
                },
                Step::CopyBroker {
                    from: layout.broker_source.clone(),
                    to: layout.broker(),
                },
                Step::RegisterService {
                    exe: layout.broker(),
                    modules: layout.modules(),
                    client_sid: client_sid.clone(),
                },
                Step::StartService,
            ])
        }
    }
}

/// Files copied with the blobs when present: the licence and the module
/// sources (LGPL-2.1 asks for the source next to the object code). The
/// `include` directory of the sources is copied as well.
const MODULE_EXTRAS: &[&str] = &[
    "COPYING",
    "SmbusPIIX4.p",
    "SmbusI801.p",
    "Echo.p",
    "sleepmode.inc",
    "SOURCE.txt",
];

/// Check the pinned blobs in `dir` (and that `COPYING` is there).
fn check_modules(dir: &Path) -> Result<(), String> {
    for pin in lightloom_iobroker::pins::MODULES {
        let path = dir.join(pin.file);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        lightloom_iobroker::pins::verify_against(pin, &bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if !dir.join("COPYING").is_file() {
        return Err(format!(
            "{}: COPYING (the modules' LGPL-2.1 licence) is missing; use the PawnIO.Modules {} release as downloaded",
            dir.display(),
            lightloom_iobroker::pins::MODULES_RELEASE
        ));
    }
    Ok(())
}

/// Copy `from` to `to` and check the copy against the SHA-256 of `from`
/// taken before copying (the source folder may be writable by the user: a
/// file swapped during the copy is caught). Returns the hash.
fn copy_verified(from: &Path, to: &Path) -> Result<String, String> {
    let bytes = std::fs::read(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let want = lightloom_iobroker::pins::sha256_hex(&bytes);
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(to, &bytes).map_err(|e| format!("{}: {e}", to.display()))?;
    let copied = std::fs::read(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let got = lightloom_iobroker::pins::sha256_hex(&copied);
    if got != want {
        let _ = std::fs::remove_file(to);
        return Err(format!(
            "{}: the copy does not match the file read (SHA-256 {got}, expected {want}); removed",
            to.display()
        ));
    }
    Ok(want)
}

#[derive(Debug, Default)]
struct Args {
    smbus: bool,
    pawnio_install: bool,
    uninstall: bool,
    plan: bool,
    elevate: bool,
    pause: bool,
    modules: Option<PathBuf>,
    client_sid: Option<String>,
    /// The arguments as given, without `--elevate` (for the elevated copy).
    forward: Vec<String>,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.skip(1);
    while let Some(a) = it.next() {
        if a != "--elevate" {
            out.forward.push(a.clone());
        }
        match a.as_str() {
            "--smbus" => out.smbus = true,
            "--pawnio-install" => out.pawnio_install = true,
            "--uninstall" => out.uninstall = true,
            "--plan" => out.plan = true,
            "--elevate" => out.elevate = true,
            "--pause" => out.pause = true,
            "--pawnio-modules" => {
                let dir = it.next().ok_or("--pawnio-modules needs a directory")?;
                out.forward.push(dir.clone());
                out.modules = Some(dir.into());
            }
            "--client-sid" => {
                let sid = it.next().ok_or("--client-sid needs a SID")?;
                out.forward.push(sid.clone());
                out.client_sid = Some(sid);
            }
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    if !out.smbus && !out.pawnio_install {
        return Err("nothing to do: give --pawnio-install and/or --smbus".into());
    }
    if out.uninstall && (out.pawnio_install || !out.smbus) {
        return Err("--uninstall goes with --smbus alone (PawnIO is never uninstalled by Lightloom)".into());
    }
    Ok(out)
}

const USAGE: &str = "usage: lightloom-setup --pawnio-install [--smbus] [--elevate] [--pause] [--plan]\n       \
lightloom-setup --smbus [--pawnio-modules <dir>] [--client-sid SID] [--elevate] [--pause] [--plan]\n       \
lightloom-setup --smbus --uninstall [--elevate] [--pause] [--plan]";

fn main() -> ExitCode {
    let args = match parse(std::env::args()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("lightloom-setup: {e}\n{USAGE}");
            return ExitCode::from(exit::USAGE as u8);
        }
    };
    #[cfg(windows)]
    {
        ExitCode::from(win::run(args) as u8)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        eprintln!("lightloom-setup: Windows only (Linux reaches /dev/i2c-* without a broker)");
        ExitCode::from(exit::USAGE as u8)
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use lightloom_hal_windows::elevate;
    use lightloom_hal_windows::pawnio::{command_line, ERROR_CANCELLED};
    use lightloom_iobroker::{detect, SERVICE_DISPLAY_NAME, SERVICE_NAME};
    use std::ffi::OsString;
    use std::time::{Duration, Instant};
    use windows_service::service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions,
        ServiceFailureResetPeriod, ServiceInfo, ServiceSidType, ServiceStartType, ServiceState, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    /// `ERROR_SERVICE_MARKED_FOR_DELETE`: the old service is still going away.
    const MARKED_FOR_DELETE: i32 = 1072;

    fn exe_dir() -> Result<PathBuf, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        exe.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "no directory for lightloom-setup.exe".into())
    }

    fn layout() -> Result<Layout, String> {
        Ok(Layout {
            root: lightloom_iobroker::boot::program_files().join("Lightloom"),
            broker_source: exe_dir()?.join("lightloom-iobroker.exe"),
        })
    }

    /// The user SID of this process's token (`S-1-5-21-…`).
    fn token_sid() -> Result<String, String> {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
        use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
        use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let mut token = HANDLE::default();
        // SAFETY: the pseudo handle of this process; `token` is a valid out pointer.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
            .map_err(|e| format!("process token: {e}"))?;
        // TOKEN_USER plus the SID it points into: 8-byte aligned, ample.
        let mut buf = [0u64; 64];
        let mut len = 0u32;
        // SAFETY: valid token; `buf` is writable for the size passed; `len` is a valid out pointer.
        let info = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                Some(buf.as_mut_ptr().cast()),
                std::mem::size_of_val(&buf) as u32,
                &mut len,
            )
        };
        // SAFETY: the token handle was opened above and is closed once.
        let _ = unsafe { CloseHandle(token) };
        info.map_err(|e| format!("token user: {e}"))?;
        // SAFETY: GetTokenInformation(TokenUser) filled `buf` with a TOKEN_USER
        // whose SID pointer points inside `buf`, which outlives this use.
        let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text = PWSTR::null();
        // SAFETY: a valid SID; `text` receives a LocalAlloc'ed string freed below.
        unsafe { ConvertSidToStringSidW(sid, &mut text) }.map_err(|e| format!("SID text: {e}"))?;
        // SAFETY: NUL-terminated string from ConvertSidToStringSidW.
        let out = unsafe { text.to_string() }.map_err(|e| format!("SID text: {e}"));
        // SAFETY: allocated by ConvertSidToStringSidW, freed once.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        }
        out
    }

    fn copy_modules(from: &Path, to: &Path) -> Result<(), String> {
        check_modules(from)?;
        std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let pins = lightloom_iobroker::pins::MODULES.iter().map(|p| p.file);
        for name in pins.chain(MODULE_EXTRAS.iter().copied()) {
            let src = from.join(name);
            if src.is_file() {
                std::fs::copy(&src, to.join(name)).map_err(|e| format!("{}: {e}", src.display()))?;
            } else if name.ends_with(".p") {
                println!(
                    "note: {name} not found in {}: add the module sources for LGPL-2.1",
                    from.display()
                );
            }
        }
        let include = from.join("include");
        if include.is_dir() {
            let dest = to.join("include");
            std::fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
            for entry in std::fs::read_dir(&include).map_err(|e| format!("{}: {e}", include.display()))? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_file() && path.extension().is_some_and(|x| x == "inc") {
                    if let Some(name) = path.file_name() {
                        std::fs::copy(&path, dest.join(name))
                            .map_err(|e| format!("{}: {e}", path.display()))?;
                    }
                }
            }
        }
        // Copied blobs are checked again: the broker loads only pinned files.
        check_modules(to)
    }

    fn manager(access: ServiceManagerAccess) -> Result<ServiceManager, String> {
        ServiceManager::local_computer(None::<&str>, access)
            .map_err(|e| format!("service manager (run elevated): {e}"))
    }

    fn os_code(e: &windows_service::Error) -> Option<i32> {
        match e {
            windows_service::Error::Winapi(io) => io.raw_os_error(),
            _ => None,
        }
    }

    fn register(exe: &Path, modules: &Path, sid: &str) -> Result<(), String> {
        let m = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
        let info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe.to_path_buf(),
            launch_arguments: vec![
                "service".into(),
                "--client-sid".into(),
                sid.into(),
                "--modules".into(),
                modules.into(),
            ],
            dependencies: Vec::new(),
            account_name: None, // LocalSystem
            account_password: None,
        };
        // A service deleted just before (an update) may take a moment to go.
        let deadline = Instant::now() + Duration::from_secs(15);
        let service = loop {
            match m.create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START) {
                Ok(s) => break s,
                Err(e) if os_code(&e) == Some(MARKED_FOR_DELETE) && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(e) => return Err(format!("registering {SERVICE_NAME}: {e}")),
            }
        };
        let _ = service.set_description(
            "Serves the chipset SMBus (RAM lighting) to Lightloom through the PawnIO driver: closed operation set, \
             SPD writes refused, global SMBus mutex.",
        );
        service
            .set_config_service_sid_info(ServiceSidType::Unrestricted)
            .map_err(|e| format!("service SID: {e}"))?;
        service
            .update_failure_actions(ServiceFailureActions {
                reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86400)),
                reboot_msg: None,
                command: None,
                actions: Some(vec![
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(5),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(60),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::None,
                        delay: Duration::ZERO,
                    },
                ]),
            })
            .map_err(|e| format!("failure actions: {e}"))
    }

    fn stop_service() -> Result<(), String> {
        let m = manager(ServiceManagerAccess::CONNECT)?;
        let Ok(s) = m.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::QUERY_STATUS) else {
            return Ok(()); // not registered
        };
        let stopped = |s: &windows_service::service::Service| {
            s.query_status()
                .is_ok_and(|st| st.current_state == ServiceState::Stopped)
        };
        if stopped(&s) {
            return Ok(());
        }
        s.stop().map_err(|e| format!("stopping {SERVICE_NAME}: {e}"))?;
        // Its executable is replaced or removed next: wait until it ended.
        let deadline = Instant::now() + Duration::from_secs(20);
        while !stopped(&s) {
            if Instant::now() >= deadline {
                return Err(format!("{SERVICE_NAME} did not stop within 20 s"));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Ok(())
    }

    fn execute(step: &Step, layout: &Layout) -> Result<(), String> {
        match step {
            Step::CopyModules { from, to } => copy_modules(from, to),
            Step::CopyBroker { from, to } => {
                let sha = copy_verified(from, to)?;
                println!("  {} SHA-256 {sha}", to.display());
                Ok(())
            }
            Step::RegisterService {
                exe,
                modules,
                client_sid,
            } => register(exe, modules, client_sid),
            Step::StartService => {
                let m = manager(ServiceManagerAccess::CONNECT)?;
                let s = m
                    .open_service(SERVICE_NAME, ServiceAccess::START)
                    .map_err(|e| e.to_string())?;
                s.start::<&str>(&[])
                    .map_err(|e| format!("starting {SERVICE_NAME}: {e}"))
            }
            Step::StopService => stop_service(),
            Step::DeleteService => {
                let m = manager(ServiceManagerAccess::CONNECT)?;
                match m.open_service(SERVICE_NAME, ServiceAccess::DELETE) {
                    Ok(s) => s.delete().map_err(|e| format!("deleting {SERVICE_NAME}: {e}")),
                    Err(_) => Ok(()),
                }
            }
            Step::RemoveFiles { broker, modules } => {
                let _ = std::fs::remove_file(broker);
                let _ = std::fs::remove_dir_all(layout.downloads());
                let result = match std::fs::remove_dir_all(modules) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                        Err(format!("{}: {e}", modules.display()))
                    }
                    _ => Ok(()),
                };
                // `pawnio\` and `Lightloom\` themselves, when nothing else is left in them.
                if let Some(parent) = modules.parent() {
                    let _ = std::fs::remove_dir(parent);
                }
                let _ = std::fs::remove_dir(&layout.root);
                result
            }
        }
    }

    /// Whether this process is the only one on its console: it was given a
    /// new window (started from the installer, not from a terminal).
    fn alone_on_console() -> bool {
        use windows::Win32::System::Console::GetConsoleProcessList;
        let mut ids = [0u32; 4];
        // SAFETY: `ids` is a writable buffer of the length passed.
        unsafe { GetConsoleProcessList(&mut ids) == 1 }
    }

    /// Start this command again through the UAC prompt and wait for it.
    fn relaunch_elevated(args: &Args) -> u32 {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("lightloom-setup: {e}");
                return exit::FAILED;
            }
        };
        let line = command_line(&args.forward);
        println!("Asking Windows for administrator rights (UAC prompt)…");
        if alone_on_console() {
            // The elevated copy opens its own window; this one has nothing to show.
            // SAFETY: detaching from this process's own console.
            let _ = unsafe { windows::Win32::System::Console::FreeConsole() };
        }
        match elevate::run_elevated(&exe, &line).and_then(|p| p.wait()) {
            Ok(code) => code,
            Err(e) if e.raw_os_error() == Some(ERROR_CANCELLED) => {
                eprintln!("lightloom-setup: the UAC prompt was declined; nothing was changed");
                exit::CANCELLED
            }
            Err(e) => {
                eprintln!("lightloom-setup: starting elevated: {e}");
                exit::FAILED
            }
        }
    }

    fn pause(code: u32) {
        use std::io::BufRead;
        if code == exit::OK {
            println!("Done. This window closes in a few seconds.");
            std::thread::sleep(Duration::from_secs(4));
        } else {
            println!("Press Enter to close this window.");
            let _ = std::io::stdin().lock().read_line(&mut String::new());
        }
    }

    pub fn run(args: Args) -> u32 {
        if args.elevate && !args.plan && !elevate::is_elevated() {
            return relaunch_elevated(&args);
        }
        let code = run_here(&args);
        if args.pause {
            pause(code);
        }
        code
    }

    fn run_here(args: &Args) -> u32 {
        let layout = match layout() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("lightloom-setup: {e}");
                return exit::FAILED;
            }
        };
        let mut pawnio_planned = false;
        if args.pawnio_install {
            match pawnio_setup::ensure_pawnio(&layout.downloads(), args.plan) {
                Ok(planned) => pawnio_planned = planned,
                Err(e) => {
                    eprintln!("lightloom-setup: {e}");
                    eprintln!(
                        "RAM lighting stays off; nothing else was changed. Try again later from the dashboard \
                         (Settings → PawnIO / RAM lighting)."
                    );
                    return exit::PAWNIO_UNAVAILABLE;
                }
            }
        }
        if !args.smbus {
            return exit::OK;
        }
        let (action, pawnio) = if args.uninstall {
            (Action::Uninstall, PawnIoStatus::NotInstalled)
        } else {
            let modules_from = match &args.modules {
                Some(m) => m.clone(),
                None => match exe_dir() {
                    Ok(d) => bundled_modules(&d),
                    Err(e) => {
                        eprintln!("lightloom-setup: {e}");
                        return exit::FAILED;
                    }
                },
            };
            if let Err(e) = check_modules(&modules_from) {
                eprintln!("lightloom-setup: {e}");
                return exit::FAILED;
            }
            let client_sid = match args.client_sid.clone().map_or_else(token_sid, Ok) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("lightloom-setup: {e}; pass --client-sid");
                    return exit::FAILED;
                }
            };
            // Opens the PawnIO device (elevated) and runs the Echo probes. A
            // plan whose PawnIO install would run first assumes it worked.
            let pawnio = if pawnio_planned {
                PawnIoStatus::Ready(lightloom_iobroker::pawnio::MIN_VERSION)
            } else {
                detect::probe(Some(&modules_from)).status
            };
            (
                Action::Install {
                    modules_from,
                    client_sid,
                },
                pawnio,
            )
        };
        let steps = match plan_steps(&action, pawnio, &layout) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lightloom-setup: {e}");
                return exit::FAILED;
            }
        };
        for step in &steps {
            if args.plan {
                println!("would {step}");
                continue;
            }
            println!("{step}");
            if let Err(e) = execute(step, &layout) {
                eprintln!("lightloom-setup: {e}");
                return exit::FAILED;
            }
        }
        if !args.plan && !args.uninstall {
            println!("RAM lighting is set up: Lightloom picks the I/O service up on its own.");
        }
        exit::OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            root: PathBuf::from(r"C:\Program Files\Lightloom"),
            broker_source: PathBuf::from(r"D:\Lightloom\lightloom-iobroker.exe"),
        }
    }

    fn install() -> Action {
        Action::Install {
            modules_from: PathBuf::from(r"D:\Downloads\release_0_2_11"),
            client_sid: "S-1-5-21-1-2-3-1001".into(),
        }
    }

    // Windows paths: `Path` only treats `\` as a separator there.
    #[cfg(windows)]
    #[test]
    fn install_plan_needs_a_usable_pawnio() {
        for status in [
            PawnIoStatus::NotInstalled,
            PawnIoStatus::TooOld(0x0002_0100),
            PawnIoStatus::Unrestricted(0x0002_0200),
            PawnIoStatus::NoAccess,
        ] {
            let err = plan_steps(&install(), status, &layout()).unwrap_err();
            assert!(
                err.contains("install the official edition of PawnIO"),
                "{status:?}: {err}"
            );
        }
        let steps = plan_steps(&install(), PawnIoStatus::Ready(0x0002_0200), &layout()).unwrap();
        assert_eq!(steps.len(), 6);
        assert_eq!(
            steps[..2],
            [Step::StopService, Step::DeleteService],
            "an existing broker is replaced (update)"
        );
        assert!(matches!(&steps[2], Step::CopyModules { to, .. } if to.ends_with(r"pawnio\0.2.11")));
        assert!(
            matches!(&steps[3], Step::CopyBroker { to, .. } if to.starts_with(r"C:\Program Files\Lightloom")),
            "the service binary lives where only administrators write"
        );
        assert!(
            matches!(&steps[4], Step::RegisterService { client_sid, .. } if client_sid == "S-1-5-21-1-2-3-1001")
        );
        assert_eq!(steps[5], Step::StartService);
    }

    #[test]
    fn uninstall_never_touches_pawnio() {
        let steps = plan_steps(&Action::Uninstall, PawnIoStatus::NotInstalled, &layout()).unwrap();
        assert_eq!(steps[..2], [Step::StopService, Step::DeleteService]);
        assert!(steps
            .iter()
            .all(|s| !s.to_string().to_lowercase().contains("uninstall pawnio")));
        assert!(steps[2].to_string().contains("PawnIO itself stays"));
    }

    // Windows paths: `Path` only treats `\` as a separator there.
    #[cfg(windows)]
    #[test]
    fn downloads_and_bundled_modules_have_fixed_places() {
        assert_eq!(
            layout().downloads(),
            PathBuf::from(r"C:\Program Files\Lightloom\downloads"),
            "an administrators-only folder: nothing can swap the installer between check and run"
        );
        assert_eq!(
            bundled_modules(Path::new(r"C:\Users\u\AppData\Local\Programs\Lightloom")),
            PathBuf::from(r"C:\Users\u\AppData\Local\Programs\Lightloom\pawnio-modules")
        );
    }

    #[test]
    fn modules_must_match_their_pins() {
        let dir = std::env::temp_dir().join(format!("lightloom-setup-pins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(check_modules(&dir).unwrap_err().contains("SmbusPIIX4.bin"));
        std::fs::write(dir.join("SmbusPIIX4.bin"), [0u8; 16]).unwrap();
        assert!(check_modules(&dir)
            .unwrap_err()
            .contains("not the pinned release file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_intel_module_and_its_source_are_installed() {
        let files: Vec<&str> = lightloom_iobroker::pins::MODULES.iter().map(|p| p.file).collect();
        assert_eq!(files, ["SmbusPIIX4.bin", "SmbusI801.bin", "Echo.bin"]);
        for source in ["SmbusPIIX4.p", "SmbusI801.p", "Echo.p", "sleepmode.inc"] {
            assert!(MODULE_EXTRAS.contains(&source), "{source}");
        }
    }

    #[test]
    fn broker_copy_is_checked() {
        let dir = std::env::temp_dir().join(format!("lightloom-setup-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("lightloom-iobroker.exe");
        std::fs::write(&from, b"MZ broker").unwrap();
        let to = dir.join("pf").join("Lightloom").join("lightloom-iobroker.exe");
        let sha = copy_verified(&from, &to).unwrap();
        assert_eq!(sha, lightloom_iobroker::pins::sha256_hex(b"MZ broker"));
        assert_eq!(std::fs::read(&to).unwrap(), b"MZ broker");
        assert!(copy_verified(&dir.join("missing.exe"), &to).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn parse_list(list: &[&str]) -> Result<Args, String> {
        parse(
            std::iter::once("lightloom-setup")
                .chain(list.iter().copied())
                .map(String::from),
        )
    }

    #[test]
    fn command_line() {
        assert!(parse_list(&[]).is_err());
        let a = parse_list(&["--smbus"]).unwrap();
        assert!(a.modules.is_none(), "the bundled modules are the default");
        let a = parse_list(&["--smbus", "--pawnio-modules", "x", "--plan"]).unwrap();
        assert!(a.plan && !a.uninstall && a.modules == Some(PathBuf::from("x")));
        assert!(parse_list(&["--smbus", "--uninstall"]).unwrap().uninstall);
        assert!(parse_list(&["--smbus", "--pawnio-modules"]).is_err());
        assert!(parse_list(&["--smbus", "--bogus"]).is_err());
    }

    #[test]
    fn pawnio_install_arguments() {
        let a = parse_list(&["--pawnio-install"]).unwrap();
        assert!(a.pawnio_install && !a.smbus);
        let a = parse_list(&["--pawnio-install", "--smbus", "--elevate", "--pause"]).unwrap();
        assert!(a.pawnio_install && a.smbus && a.elevate && a.pause);
        assert_eq!(
            a.forward,
            ["--pawnio-install", "--smbus", "--pause"],
            "the elevated copy gets everything but --elevate"
        );
        assert!(
            parse_list(&["--pawnio-install", "--uninstall", "--smbus"]).is_err(),
            "Lightloom never uninstalls PawnIO"
        );
        assert!(parse_list(&["--uninstall"]).is_err());
        assert!(parse_list(&["--elevate"]).is_err(), "nothing to do");
        let a = parse_list(&["--smbus", "--client-sid", "S-1-5-21-9", "--elevate"]).unwrap();
        assert_eq!(a.forward, ["--smbus", "--client-sid", "S-1-5-21-9"]);
    }

    #[test]
    fn dashboard_arguments_parse() {
        use lightloom_hal_windows::pawnio::{setup_args, SetupAction};
        for action in [SetupAction::Install, SetupAction::Remove] {
            let list: Vec<&str> = setup_args(action).to_vec();
            let a = parse_list(&list).unwrap();
            assert!(a.smbus && a.pause && !a.elevate, "{action:?}");
            assert_eq!(a.uninstall, action == SetupAction::Remove);
            assert_eq!(a.pawnio_install, action == SetupAction::Install);
        }
    }
}
