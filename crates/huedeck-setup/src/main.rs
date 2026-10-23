// SPDX-License-Identifier: GPL-3.0-or-later
//! `huedeck-setup`: one-shot elevated registration of the HueDeck I/O broker.
//!
//! RAM lighting needs the chipset SMBus, which only a privileged process may
//! reach (through the PawnIO driver). Run once from an elevated prompt (one
//! UAC prompt), and only when the user opts into RAM lighting:
//!
//! ```text
//! huedeck-setup --smbus --pawnio-modules <dir> [--client-sid SID] [--plan]
//! huedeck-setup --smbus --uninstall [--plan]
//! ```
//!
//! Install:
//! 1. check the PawnIO driver: installed, official edition, 2.2.0 or later.
//!    **This tool never downloads or installs PawnIO** (installing it is the
//!    user's choice: pawnio.eu, or an option of the HueDeck installer); when
//!    it is missing or too old this tool stops and says so;
//! 2. check `SmbusPIIX4.bin` and `Echo.bin` in `<dir>` (the signed PawnIO.Modules
//!    0.2.11 release the user downloaded) against their SHA-256 pins, and copy
//!    them with `COPYING` and the module sources found there (LGPL-2.1) to
//!    `%ProgramFiles%\HueDeck\pawnio\0.2.11\`;
//! 3. copy `huedeck-iobroker.exe` to `%ProgramFiles%\HueDeck\` (a LocalSystem
//!    service must not run from a folder its user can write), and check that
//!    the copy is byte for byte the file hashed before copying (its SHA-256 is
//!    printed, to compare with the release's);
//! 4. register `HueDeckIoBroker`: LocalSystem, automatic start, restart on
//!    failure, unrestricted service SID, the pipe opened to the interactive
//!    user only (`--client-sid`, default: the user of this process's token);
//! 5. start it.
//!
//! Nothing comes from the environment or the search path: Program Files is
//! read from `HKLM` ([`huedeck_iobroker::boot::program_files`]), the SID from
//! the process token, and no other program is started.
//!
//! `--uninstall` stops and deletes the service and removes those files. It
//! never uninstalls PawnIO. `--plan` prints the steps and changes nothing.

// Windows-only tool: elsewhere it only builds (CI), so its steps go unused.
#![cfg_attr(not(windows), allow(dead_code))]

use huedeck_iobroker::detect::PawnIoStatus;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// One thing the setup does.
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
    /// Stop it (uninstall).
    StopService,
    /// Delete it (uninstall).
    DeleteService,
    /// Remove the broker and the module copies (uninstall).
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
                huedeck_iobroker::SERVICE_NAME,
                exe.display()
            ),
            Step::StartService => write!(f, "start {}", huedeck_iobroker::SERVICE_NAME),
            Step::StopService => write!(f, "stop {}", huedeck_iobroker::SERVICE_NAME),
            Step::DeleteService => write!(f, "delete {}", huedeck_iobroker::SERVICE_NAME),
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
    /// `%ProgramFiles%\HueDeck`.
    root: PathBuf,
    /// The broker executable shipped next to this tool.
    broker_source: PathBuf,
}

impl Layout {
    fn modules(&self) -> PathBuf {
        self.root
            .join("pawnio")
            .join(huedeck_iobroker::pins::MODULES_RELEASE)
    }

    fn broker(&self) -> PathBuf {
        self.root.join("huedeck-iobroker.exe")
    }
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
/// stops when PawnIO is not usable: this tool does not install it.
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
                     (https://pawnio.eu), then run this again."
                ));
            }
            Ok(vec![
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
/// sources (LGPL-2.1 asks for the source next to the object code).
const MODULE_EXTRAS: &[&str] = &["COPYING", "SmbusPIIX4.p", "Echo.p", "sleepmode.inc", "SOURCE.txt"];

/// Check the pinned blobs in `dir` (and that `COPYING` is there).
fn check_modules(dir: &Path) -> Result<(), String> {
    for pin in huedeck_iobroker::pins::MODULES {
        let path = dir.join(pin.file);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        huedeck_iobroker::pins::verify_against(pin, &bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if !dir.join("COPYING").is_file() {
        return Err(format!(
            "{}: COPYING (the modules' LGPL-2.1 licence) is missing; use the PawnIO.Modules {} release as downloaded",
            dir.display(),
            huedeck_iobroker::pins::MODULES_RELEASE
        ));
    }
    Ok(())
}

/// Copy `from` to `to` and check the copy against the SHA-256 of `from`
/// taken before copying (the source folder may be writable by the user: a
/// file swapped during the copy is caught). Returns the hash.
fn copy_verified(from: &Path, to: &Path) -> Result<String, String> {
    let bytes = std::fs::read(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let want = huedeck_iobroker::pins::sha256_hex(&bytes);
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(to, &bytes).map_err(|e| format!("{}: {e}", to.display()))?;
    let copied = std::fs::read(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let got = huedeck_iobroker::pins::sha256_hex(&copied);
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
    uninstall: bool,
    plan: bool,
    modules: Option<PathBuf>,
    client_sid: Option<String>,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--smbus" => out.smbus = true,
            "--uninstall" => out.uninstall = true,
            "--plan" => out.plan = true,
            "--pawnio-modules" => {
                out.modules = Some(it.next().ok_or("--pawnio-modules needs a directory")?.into())
            }
            "--client-sid" => out.client_sid = Some(it.next().ok_or("--client-sid needs a SID")?),
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    if !out.smbus {
        return Err("nothing to do: --smbus is the only setup there is".into());
    }
    if !out.uninstall && out.modules.is_none() {
        return Err(
            "--pawnio-modules <dir> is required: the PawnIO.Modules 0.2.11 release, as downloaded".into(),
        );
    }
    Ok(out)
}

const USAGE: &str = "usage: huedeck-setup --smbus --pawnio-modules <dir> [--client-sid SID] [--plan]\n       huedeck-setup --smbus --uninstall [--plan]";

fn main() -> ExitCode {
    let args = match parse(std::env::args()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("huedeck-setup: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    #[cfg(windows)]
    {
        win::run(args)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        eprintln!("huedeck-setup: Windows only (Linux reaches /dev/i2c-* without a broker)");
        ExitCode::from(2)
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use huedeck_iobroker::{detect, SERVICE_DISPLAY_NAME, SERVICE_NAME};
    use std::ffi::OsString;
    use std::time::Duration;
    use windows_service::service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions,
        ServiceFailureResetPeriod, ServiceInfo, ServiceSidType, ServiceStartType, ServiceState, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    fn layout() -> Result<Layout, String> {
        let root = huedeck_iobroker::boot::program_files().join("HueDeck");
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let dir = exe.parent().ok_or("no directory for huedeck-setup.exe")?;
        Ok(Layout {
            root,
            broker_source: dir.join("huedeck-iobroker.exe"),
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
        let pins = huedeck_iobroker::pins::MODULES.iter().map(|p| p.file);
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
        // Copied blobs are checked again: the broker loads only pinned files.
        check_modules(to)
    }

    fn manager(access: ServiceManagerAccess) -> Result<ServiceManager, String> {
        ServiceManager::local_computer(None::<&str>, access)
            .map_err(|e| format!("service manager (run elevated): {e}"))
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
        let service = m
            .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
            .map_err(|e| format!("registering {SERVICE_NAME}: {e}"))?;
        let _ = service.set_description(
            "Serves the chipset SMBus (RAM lighting) to HueDeck through the PawnIO driver: closed operation set, \
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

    fn execute(step: &Step) -> Result<(), String> {
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
            Step::StopService => {
                let m = manager(ServiceManagerAccess::CONNECT)?;
                match m.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::QUERY_STATUS) {
                    Ok(s) => {
                        if s.query_status()
                            .is_ok_and(|st| st.current_state != ServiceState::Stopped)
                        {
                            s.stop().map_err(|e| format!("stopping {SERVICE_NAME}: {e}"))?;
                        }
                        Ok(())
                    }
                    Err(_) => Ok(()), // not registered
                }
            }
            Step::DeleteService => {
                let m = manager(ServiceManagerAccess::CONNECT)?;
                match m.open_service(SERVICE_NAME, ServiceAccess::DELETE) {
                    Ok(s) => s.delete().map_err(|e| format!("deleting {SERVICE_NAME}: {e}")),
                    Err(_) => Ok(()),
                }
            }
            Step::RemoveFiles { broker, modules } => {
                let _ = std::fs::remove_file(broker);
                match std::fs::remove_dir_all(modules) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                        Err(format!("{}: {e}", modules.display()))
                    }
                    _ => Ok(()),
                }
            }
        }
    }

    pub fn run(args: Args) -> ExitCode {
        let layout = match layout() {
            Ok(l) => l,
            Err(e) => {
                eprintln!("huedeck-setup: {e}");
                return ExitCode::from(1);
            }
        };
        let (action, pawnio) = if args.uninstall {
            (Action::Uninstall, PawnIoStatus::NotInstalled)
        } else {
            let modules_from = args.modules.clone().unwrap_or_default();
            if let Err(e) = check_modules(&modules_from) {
                eprintln!("huedeck-setup: {e}");
                return ExitCode::from(1);
            }
            let client_sid = match args.client_sid.clone().map_or_else(token_sid, Ok) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("huedeck-setup: {e}; pass --client-sid");
                    return ExitCode::from(1);
                }
            };
            // Opens the PawnIO device (elevated) and runs the Echo probes.
            let pawnio = detect::probe(Some(&modules_from)).status;
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
                eprintln!("huedeck-setup: {e}");
                return ExitCode::from(1);
            }
        };
        for step in &steps {
            if args.plan {
                println!("would {step}");
                continue;
            }
            println!("{step}");
            if let Err(e) = execute(step) {
                eprintln!("huedeck-setup: {e}");
                return ExitCode::from(1);
            }
        }
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            root: PathBuf::from(r"C:\Program Files\HueDeck"),
            broker_source: PathBuf::from(r"D:\HueDeck\huedeck-iobroker.exe"),
        }
    }

    fn install() -> Action {
        Action::Install {
            modules_from: PathBuf::from(r"D:\Downloads\release_0_2_11"),
            client_sid: "S-1-5-21-1-2-3-1001".into(),
        }
    }

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
        assert_eq!(steps.len(), 4);
        assert!(matches!(&steps[0], Step::CopyModules { to, .. } if to.ends_with(r"pawnio\0.2.11")));
        assert!(
            matches!(&steps[1], Step::CopyBroker { to, .. } if to.starts_with(r"C:\Program Files\HueDeck")),
            "the service binary lives where only administrators write"
        );
        assert!(
            matches!(&steps[2], Step::RegisterService { client_sid, .. } if client_sid == "S-1-5-21-1-2-3-1001")
        );
        assert_eq!(steps[3], Step::StartService);
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

    #[test]
    fn modules_must_match_their_pins() {
        let dir = std::env::temp_dir().join(format!("huedeck-setup-pins-{}", std::process::id()));
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
    fn broker_copy_is_checked() {
        let dir = std::env::temp_dir().join(format!("huedeck-setup-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("huedeck-iobroker.exe");
        std::fs::write(&from, b"MZ broker").unwrap();
        let to = dir.join("pf").join("HueDeck").join("huedeck-iobroker.exe");
        let sha = copy_verified(&from, &to).unwrap();
        assert_eq!(sha, huedeck_iobroker::pins::sha256_hex(b"MZ broker"));
        assert_eq!(std::fs::read(&to).unwrap(), b"MZ broker");
        assert!(copy_verified(&dir.join("missing.exe"), &to).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_line() {
        let parse_list = |list: &[&str]| {
            parse(
                std::iter::once("huedeck-setup")
                    .chain(list.iter().copied())
                    .map(String::from),
            )
        };
        assert!(parse_list(&[]).is_err());
        assert!(parse_list(&["--smbus"]).is_err(), "install needs the modules");
        let a = parse_list(&["--smbus", "--pawnio-modules", "x", "--plan"]).unwrap();
        assert!(a.plan && !a.uninstall);
        assert!(parse_list(&["--smbus", "--uninstall"]).unwrap().uninstall);
    }
}
