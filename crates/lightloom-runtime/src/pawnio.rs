// SPDX-License-Identifier: GPL-3.0-or-later
//! PawnIO / RAM lighting: what is installed (read-only facts for the
//! dashboard) and the elevated helper run behind its Install / Remove
//! buttons.
//!
//! The facts come from PawnIO's uninstall entry and the service manager
//! (read-only; this process never opens the PawnIO device) and, while the
//! SMBus buses are open, from the broker's own `Health` answer
//! ([`crate::smbus::SmbusHardware::broker`]).
//!
//! [`launch`] starts `lightloom-setup.exe` (next to this executable) with the
//! `runas` verb: Windows shows its UAC prompt, the user may decline it, and
//! the helper opens its own window with what it does. It is only ever called
//! on an explicit user action; nothing here elevates on its own.

pub use lightloom_hal_windows::pawnio::{
    describe_exit, exit, parse_version, PawnIoPresence, ServiceState, SetupAction, MIN_VERSION,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// What the broker said about itself in its `Health` answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrokerHealth {
    /// PawnIO driver version as the broker read it (`maj<<16 | min<<8 | patch`).
    pub pawnio_version: u32,
    /// `official`, `unrestricted` or `unknown`.
    pub edition: String,
    /// Why Lightloom does not use it, if it does not.
    pub problem: Option<String>,
    /// Chipset buses it serves (0 when unhealthy).
    pub buses: usize,
}

impl BrokerHealth {
    /// From a `Health` answer and the number of buses taken from it.
    pub fn from_report(h: &lightloom_protocol::broker::HealthReport, buses: usize) -> Self {
        use lightloom_protocol::broker::PawnIoEdition;
        Self {
            pawnio_version: h.pawnio_version,
            edition: match h.pawnio_edition {
                PawnIoEdition::Official => "official",
                PawnIoEdition::Unrestricted => "unrestricted",
                PawnIoEdition::Unknown => "unknown",
            }
            .into(),
            problem: lightloom_hal_windows::broker::health_problem(h),
            buses,
        }
    }
}

/// Read-only facts about PawnIO and the I/O broker on this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PawnIoFacts {
    /// Windows (RAM lighting goes through PawnIO only there).
    pub windows: bool,
    /// `lightloom-setup.exe` next to this executable, when it is there.
    pub helper: Option<PathBuf>,
    /// PawnIO's uninstall entry.
    pub presence: PawnIoPresence,
    /// The `LightloomIoBroker` service.
    pub service: ServiceState,
}

/// `lightloom-setup.exe` in `dir`, if it is there.
pub fn helper_in(dir: &Path) -> Option<PathBuf> {
    Some(dir.join(lightloom_hal_windows::pawnio::SETUP_EXE)).filter(|p| p.is_file())
}

/// Gather the facts (registry and service manager, read-only).
pub fn facts() -> PawnIoFacts {
    let helper = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(helper_in));
    #[cfg(windows)]
    {
        PawnIoFacts {
            windows: true,
            helper,
            presence: lightloom_hal_windows::pawnio::pawnio_presence(),
            service: lightloom_hal_windows::service::service_state(SERVICE_NAME),
        }
    }
    #[cfg(not(windows))]
    {
        PawnIoFacts {
            windows: false,
            helper,
            presence: PawnIoPresence::NotInstalled,
            service: ServiceState::NotInstalled,
        }
    }
}

/// The I/O broker's service name (`lightloom_iobroker::SERVICE_NAME`).
pub const SERVICE_NAME: &str = "LightloomIoBroker";

/// How a helper run ended: success, and the line for the user.
pub type SetupOutcome = (bool, String);

/// A running `lightloom-setup` (behind its UAC prompt).
#[derive(Debug)]
pub struct SetupRun {
    /// What it does.
    pub action: SetupAction,
    done: mpsc::Receiver<SetupOutcome>,
}

impl SetupRun {
    /// The outcome once the helper ended (or the prompt was declined).
    pub fn poll(&self) -> Option<SetupOutcome> {
        match self.done.try_recv() {
            Ok(o) => Some(o),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some((false, "lightloom-setup ended without a result".into()))
            }
        }
    }

    /// A run already over (tests, refusals).
    pub fn finished(action: SetupAction, outcome: SetupOutcome) -> Self {
        let (tx, done) = mpsc::channel();
        let _ = tx.send(outcome);
        Self { action, done }
    }
}

/// Start `helper` elevated for `action` (Windows shows the UAC prompt) on a
/// thread of its own, which waits for it; [`SetupRun::poll`] tells the end.
pub fn launch(helper: &Path, action: SetupAction) -> std::io::Result<SetupRun> {
    #[cfg(windows)]
    {
        use lightloom_hal_windows::pawnio::{command_line, setup_args, ERROR_CANCELLED};
        let (tx, done) = mpsc::channel();
        let helper = helper.to_path_buf();
        std::thread::Builder::new()
            .name("lightloom-setup-wait".into())
            .spawn(move || {
                let line = command_line(setup_args(action));
                let outcome = match lightloom_hal_windows::elevate::run_elevated(&helper, &line) {
                    Err(e) if e.raw_os_error() == Some(ERROR_CANCELLED) => {
                        describe_exit(action, exit::CANCELLED)
                    }
                    Err(e) => (false, format!("{} could not be started: {e}", helper.display())),
                    Ok(process) => match process.wait() {
                        Ok(code) => describe_exit(action, code),
                        Err(e) => (false, format!("waiting for lightloom-setup: {e}")),
                    },
                };
                let _ = tx.send(outcome);
            })?;
        Ok(SetupRun { action, done })
    }
    #[cfg(not(windows))]
    {
        let _ = (helper, action);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "PawnIO is Windows only",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_protocol::broker::{HealthReport, ModuleStatus, PawnIoEdition, BROKER_PROTOCOL_VERSION};

    #[test]
    fn health_summaries() {
        let mut h = HealthReport {
            version: "0.0.1".into(),
            protocol: BROKER_PROTOCOL_VERSION,
            uptime_secs: 1,
            dry_run: false,
            ops_served: 0,
            modules: vec![ModuleStatus {
                name: "SmbusPIIX4".into(),
                sha256: String::new(),
                loaded: true,
            }],
            pawnio_version: 0x0002_0200,
            pawnio_edition: PawnIoEdition::Official,
            ci_options: 0,
            sleep_mode: 0,
        };
        let ok = BrokerHealth::from_report(&h, 2);
        assert_eq!(ok.edition, "official");
        assert_eq!((ok.problem, ok.buses), (None, 2));
        h.pawnio_edition = PawnIoEdition::Unrestricted;
        let bad = BrokerHealth::from_report(&h, 0);
        assert_eq!(bad.edition, "unrestricted");
        assert!(bad.problem.is_some());
    }

    #[test]
    fn finished_runs_report_once_done() {
        let run = SetupRun::finished(SetupAction::Install, (false, "declined".into()));
        assert_eq!(run.poll(), Some((false, "declined".into())));
        assert!(run.poll().is_some(), "a dropped sender reads as ended");
    }

    #[test]
    fn helper_lookup() {
        let dir = std::env::temp_dir().join(format!("lightloom-helper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(helper_in(&dir), None);
        std::fs::write(dir.join("lightloom-setup.exe"), b"MZ").unwrap();
        assert_eq!(helper_in(&dir), Some(dir.join("lightloom-setup.exe")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn facts_are_read_only_and_answer() {
        let f = facts();
        assert!(f.windows);
    }
}
