// SPDX-License-Identifier: GPL-3.0-or-later
//! The dashboard's PawnIO / RAM lighting panel: the status line from the
//! read-only facts ([`huedeck_runtime::pawnio::facts`]) and the broker's own
//! `Health`, and the Install / Remove runs of the elevated helper (one UAC
//! prompt each, only when the user presses the button).

use huedeck_api::model::{PawnIoJob, PawnIoView};
use huedeck_runtime::pawnio::{
    parse_version, BrokerHealth, PawnIoFacts, PawnIoPresence, ServiceState, SetupAction, SetupRun,
    MIN_VERSION,
};

/// `"2.2.0"` is 2.2.0 or later.
fn version_ok(version: &str) -> bool {
    parse_version(version).is_some_and(|v| v >= MIN_VERSION)
}

/// The panel, from the facts, the broker's answer (when the buses are
/// open), whether SMBus is on, whether buttons may run the helper, and the
/// last helper run.
pub fn view(
    facts: &PawnIoFacts,
    broker: Option<&BrokerHealth>,
    smbus_on: bool,
    can_run: bool,
    job: Option<PawnIoJob>,
) -> PawnIoView {
    let (installed, version) = match &facts.presence {
        PawnIoPresence::NotInstalled => (false, None),
        PawnIoPresence::Installed { version } => {
            (true, Some(version.trim().to_string()).filter(|v| !v.is_empty()))
        }
    };
    let ok_version = version.as_deref().is_some_and(version_ok);
    let healthy = broker.is_some_and(|b| b.problem.is_none());
    let ram_buses = broker.map_or(0, |b| b.buses);
    let status = if !facts.windows {
        "RAM lighting through PawnIO is a Windows feature.".to_string()
    } else if healthy && ram_buses > 0 {
        format!(
            "RAM lighting is on: the HueDeck I/O service serves {ram_buses} chipset SMBus bus(es) through PawnIO."
        )
    } else if let Some(problem) = broker.and_then(|b| b.problem.clone()) {
        format!("The HueDeck I/O service is not usable: {problem}. Press Install to repair it.")
    } else if !installed {
        "RAM lighting is off: the PawnIO driver is not installed. Install sets it up (one Windows permission prompt)."
            .to_string()
    } else if !ok_version {
        format!(
            "RAM lighting is off: PawnIO {} is older than 2.2.0. Install updates it.",
            version.as_deref().unwrap_or("(unknown version)")
        )
    } else {
        match facts.service {
            ServiceState::NotInstalled => {
                "PawnIO is installed; the HueDeck I/O service is not. Install sets it up.".to_string()
            }
            ServiceState::Running if !smbus_on => {
                "PawnIO and the HueDeck I/O service are installed; SMBus devices are turned off above.".to_string()
            }
            ServiceState::Running => {
                "PawnIO and the HueDeck I/O service are installed and running; HueDeck connects to it at the next \
                 device scan."
                    .to_string()
            }
            ServiceState::Stopped | ServiceState::Pending => {
                "The HueDeck I/O service is installed but not running. Install repairs it.".to_string()
            }
            ServiceState::Unknown => "The state of the HueDeck I/O service could not be read.".to_string(),
        }
    };
    PawnIoView {
        supported: facts.windows && facts.helper.is_some() && can_run,
        installed,
        version,
        version_ok: ok_version,
        edition: broker.map(|b| b.edition.clone()),
        service: facts.service.name().into(),
        broker_healthy: healthy,
        ram_buses,
        status,
        job,
    }
}

/// The current or last helper run.
#[derive(Debug, Default)]
pub struct Jobs {
    running: Option<SetupRun>,
    last: Option<PawnIoJob>,
}

impl Jobs {
    /// A run is going on.
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Track `run`.
    pub fn start(&mut self, run: SetupRun) {
        self.last = Some(PawnIoJob {
            action: run.action.name().into(),
            running: true,
            ok: None,
            message: match run.action {
                SetupAction::Install => {
                    "Waiting for the Windows permission prompt, then for huedeck-setup (its window shows each step)…"
                }
                SetupAction::Remove => "Waiting for the Windows permission prompt, then for huedeck-setup…",
            }
            .into(),
        });
        self.running = Some(run);
    }

    /// The run ended: its outcome (once).
    pub fn poll(&mut self) -> Option<(SetupAction, bool, String)> {
        let run = self.running.as_ref()?;
        let (ok, message) = run.poll()?;
        let action = run.action;
        self.running = None;
        self.last = Some(PawnIoJob {
            action: action.name().into(),
            running: false,
            ok: Some(ok),
            message: message.clone(),
        });
        Some((action, ok, message))
    }

    /// For the view.
    pub fn job(&self) -> Option<PawnIoJob> {
        self.last.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn facts(presence: PawnIoPresence, service: ServiceState) -> PawnIoFacts {
        PawnIoFacts {
            windows: true,
            helper: Some(PathBuf::from(r"C:\HueDeck\huedeck-setup.exe")),
            presence,
            service,
        }
    }

    fn at(v: &str) -> PawnIoPresence {
        PawnIoPresence::Installed { version: v.into() }
    }

    #[test]
    fn nothing_installed() {
        let v = view(
            &facts(PawnIoPresence::NotInstalled, ServiceState::NotInstalled),
            None,
            true,
            true,
            None,
        );
        assert!(v.supported && !v.installed && !v.version_ok && !v.broker_healthy);
        assert_eq!(v.service, "notInstalled");
        assert!(v.status.contains("not installed"), "{}", v.status);
    }

    #[test]
    fn versions_and_services() {
        let old = view(
            &facts(at("2.0.1"), ServiceState::NotInstalled),
            None,
            true,
            true,
            None,
        );
        assert!(old.installed && !old.version_ok && old.status.contains("older than 2.2.0"));
        let no_service = view(
            &facts(at("2.2.0"), ServiceState::NotInstalled),
            None,
            true,
            true,
            None,
        );
        assert!(no_service.version_ok && no_service.status.contains("is not"));
        let stopped = view(&facts(at("2.10"), ServiceState::Stopped), None, true, true, None);
        assert!(stopped.version_ok && stopped.status.contains("not running"));
        let off = view(
            &facts(at("2.2.0"), ServiceState::Running),
            None,
            false,
            true,
            None,
        );
        assert!(off.status.contains("turned off"));
    }

    #[test]
    fn the_broker_answer_wins() {
        let healthy = BrokerHealth {
            pawnio_version: 0x0002_0200,
            edition: "official".into(),
            problem: None,
            buses: 2,
        };
        let v = view(
            &facts(at("2.2.0"), ServiceState::Running),
            Some(&healthy),
            true,
            true,
            None,
        );
        assert!(v.broker_healthy && v.ram_buses == 2 && v.status.contains("RAM lighting is on"));
        assert_eq!(v.edition.as_deref(), Some("official"));
        let sick = BrokerHealth {
            edition: "unrestricted".into(),
            problem: Some("PawnIO edition Unrestricted, not the official one".into()),
            buses: 0,
            ..healthy
        };
        let v = view(
            &facts(at("2.2.0"), ServiceState::Running),
            Some(&sick),
            true,
            true,
            None,
        );
        assert!(!v.broker_healthy && v.status.contains("not usable") && v.status.contains("repair"));
    }

    #[test]
    fn buttons_need_windows_the_helper_and_real_hardware() {
        let mut f = facts(PawnIoPresence::NotInstalled, ServiceState::NotInstalled);
        assert!(
            !view(&f, None, true, false, None).supported,
            "mock / no-hardware runs"
        );
        f.helper = None;
        assert!(!view(&f, None, true, true, None).supported);
        f.windows = false;
        let v = view(&f, None, true, true, None);
        assert!(!v.supported && v.status.contains("Windows"));
    }

    #[test]
    fn jobs_report_their_end_once() {
        let mut jobs = Jobs::default();
        assert!(!jobs.busy() && jobs.poll().is_none() && jobs.job().is_none());
        jobs.start(SetupRun::finished(
            SetupAction::Install,
            (false, "declined".into()),
        ));
        assert!(jobs.busy());
        assert_eq!(
            jobs.poll(),
            Some((SetupAction::Install, false, "declined".into()))
        );
        assert!(!jobs.busy() && jobs.poll().is_none());
        let job = jobs.job().unwrap();
        assert_eq!(
            (job.action.as_str(), job.running, job.ok),
            ("install", false, Some(false))
        );
    }
}
