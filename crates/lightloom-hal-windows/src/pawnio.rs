// SPDX-License-Identifier: GPL-3.0-or-later
//! Is the PawnIO driver installed? Read from its uninstall entry only. And
//! the contract with the elevated helper `lightloom-setup` that installs it.
//!
//! RAM lighting reaches the chipset SMBus through the Lightloom I/O broker,
//! which needs the PawnIO driver (official edition, 2.2.0 or later). This
//! unprivileged process never opens the PawnIO device: it reads
//! `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO`
//! (`DisplayVersion`, as LibreHardwareMonitor does) to tell the user what is
//! missing, and [`message`] says it. Installing PawnIO is the user's choice:
//! the installer's checkbox, or the dashboard's Install button, both of which
//! start `lightloom-setup` ([`setup_args`]) behind one UAC prompt. Nothing here
//! installs anything or elevates by itself.

/// PawnIO's uninstall key.
pub const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO";
/// Oldest PawnIO Lightloom uses (`maj<<16 | min<<8 | patch`).
pub const MIN_VERSION: u32 = 0x0002_0200;

/// What the uninstall entry says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PawnIoPresence {
    /// No uninstall entry.
    NotInstalled,
    /// Installed, with its `DisplayVersion` (may be empty).
    Installed {
        /// `DisplayVersion`.
        version: String,
    },
}

/// `"2.2.0"` → `0x00020200`; `None` when it is not `major.minor[.patch…]`.
pub fn parse_version(text: &str) -> Option<u32> {
    let mut parts = text.trim().split('.').map(|p| p.trim().parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next().unwrap_or(Some(0))?;
    (major <= 0xFF && minor <= 0xFF && patch <= 0xFF).then_some((major << 16) | (minor << 8) | patch)
}

/// Whether the PawnIO installer has to run: PawnIO is missing, older than
/// 2.2.0, or its version cannot be read (the installer then repairs it).
pub fn install_needed(presence: &PawnIoPresence) -> bool {
    match presence {
        PawnIoPresence::NotInstalled => true,
        PawnIoPresence::Installed { version } => parse_version(version).is_none_or(|v| v < MIN_VERSION),
    }
}

/// Where RAM lighting is set up, for the messages.
const WHERE: &str = "Settings → PawnIO / RAM lighting → Install in the dashboard (one UAC prompt)";

/// What to tell the user about RAM lighting, or `None` when the broker answers.
pub fn message(presence: &PawnIoPresence, broker_up: bool) -> Option<String> {
    if broker_up {
        return None;
    }
    Some(match presence {
        PawnIoPresence::NotInstalled => format!(
            "RAM lighting is off: it needs the PawnIO driver (official edition 2.2.0 or later, https://pawnio.eu), \
             which is not installed on this PC. Install it with the Lightloom I/O service from {WHERE}, or run \
             `lightloom-setup --pawnio-install --smbus --elevate`. GPU lighting works without it."
        ),
        PawnIoPresence::Installed { version } if parse_version(version).is_some_and(|v| v < MIN_VERSION) => {
            format!(
                "RAM lighting: PawnIO {version} is installed; it needs 2.2.0 or later. Update it from {WHERE}, or \
                 run `lightloom-setup --pawnio-install --smbus --elevate`."
            )
        }
        PawnIoPresence::Installed { version } => {
            let v = if version.trim().is_empty() { "" } else { " " };
            format!(
                "RAM lighting: PawnIO{v}{} is installed but the Lightloom I/O broker service is not running; set it \
                 up from {WHERE}, or run `lightloom-setup --smbus --elevate`.",
                version.trim()
            )
        }
    })
}

/// Read the uninstall entry (read-only registry access; no device is opened).
#[cfg(windows)]
pub fn pawnio_presence() -> PawnIoPresence {
    match crate::registry::local_machine_strings(UNINSTALL_KEY) {
        Ok(values) if !values.is_empty() => PawnIoPresence::Installed {
            version: values
                .into_iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("DisplayVersion"))
                .map(|(_, v)| v)
                .unwrap_or_default(),
        },
        _ => PawnIoPresence::NotInstalled,
    }
}

/// The elevated helper, next to `lightloomd.exe`.
pub const SETUP_EXE: &str = "lightloom-setup.exe";

/// Exit codes of `lightloom-setup`.
pub mod exit {
    /// Done.
    pub const OK: u32 = 0;
    /// A step failed (the message was printed).
    pub const FAILED: u32 = 1;
    /// Bad command line.
    pub const USAGE: u32 = 2;
    /// PawnIO could not be downloaded or did not verify (offline, wrong hash
    /// or signer), or its own installer failed: nothing else was changed and
    /// RAM lighting stays off. Not a failure of the Lightloom installation.
    pub const PAWNIO_UNAVAILABLE: u32 = 3;
    /// The UAC prompt was declined.
    pub const CANCELLED: u32 = 4;
}

/// `ERROR_CANCELLED`: the user declined the UAC prompt.
pub const ERROR_CANCELLED: i32 = 1223;

/// What the dashboard asks the elevated helper to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupAction {
    /// Install PawnIO when needed (verified download), then (re)register the I/O broker.
    Install,
    /// Remove the I/O broker service and its files; PawnIO stays.
    Remove,
}

impl SetupAction {
    /// `install` / `remove`.
    pub fn name(self) -> &'static str {
        match self {
            SetupAction::Install => "install",
            SetupAction::Remove => "remove",
        }
    }
}

/// `lightloom-setup`'s arguments for `action` when the caller starts it with
/// the `runas` verb itself (the dashboard): its window stays open on an error.
pub fn setup_args(action: SetupAction) -> &'static [&'static str] {
    match action {
        SetupAction::Install => &["--pawnio-install", "--smbus", "--pause"],
        SetupAction::Remove => &["--smbus", "--uninstall", "--pause"],
    }
}

/// What a `lightloom-setup` exit code means for `action`: success, and the line for the user.
pub fn describe_exit(action: SetupAction, code: u32) -> (bool, String) {
    match (action, code) {
        (SetupAction::Install, exit::OK) => (
            true,
            "RAM lighting is set up: PawnIO and the Lightloom I/O service are installed.".into(),
        ),
        (SetupAction::Remove, exit::OK) => (
            true,
            "The Lightloom I/O service was removed: RAM lighting is off. PawnIO stays installed (other programs may use \
             it); remove it from Windows Settings → Apps if you no longer need it."
                .into(),
        ),
        (_, exit::CANCELLED) => (
            false,
            "The Windows permission prompt was declined: nothing was changed.".into(),
        ),
        (SetupAction::Install, exit::PAWNIO_UNAVAILABLE) => (
            false,
            "PawnIO could not be downloaded or verified (offline?): nothing was changed and RAM lighting stays off. \
             Try again later."
                .into(),
        ),
        (_, code) => (
            false,
            format!(
                "lightloom-setup ({}) failed with exit code {code}; its window said why.",
                action.name()
            ),
        ),
    }
}

/// One command line from `args`, quoted so that `CommandLineToArgvW` (and
/// the Rust runtime) split it back into the same arguments.
pub fn command_line<S: AsRef<str>>(args: &[S]) -> String {
    let mut out = String::new();
    for (i, arg) in args.iter().enumerate() {
        let arg = arg.as_ref();
        if i > 0 {
            out.push(' ');
        }
        if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
            out.push_str(arg);
            continue;
        }
        out.push('"');
        let mut backslashes = 0usize;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    out.extend(std::iter::repeat_n('\\', backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n('\\', backslashes * 2));
        out.push('"');
    }
    out
}

/// The I/O broker service as the service manager reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceState {
    /// Not registered.
    NotInstalled,
    /// Registered, not running.
    Stopped,
    /// Starting, stopping, pausing.
    Pending,
    /// Running.
    Running,
    /// Could not be asked.
    Unknown,
}

impl ServiceState {
    /// camelCase name for the API.
    pub fn name(self) -> &'static str {
        match self {
            ServiceState::NotInstalled => "notInstalled",
            ServiceState::Stopped => "stopped",
            ServiceState::Pending => "pending",
            ServiceState::Running => "running",
            ServiceState::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse() {
        assert_eq!(parse_version("2.2.0"), Some(0x0002_0200));
        assert_eq!(parse_version(" 2.1 "), Some(0x0002_0100));
        assert_eq!(parse_version("2.0.1.4"), Some(0x0002_0001));
        assert_eq!(parse_version("x"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn the_installer_runs_only_when_needed() {
        assert!(install_needed(&PawnIoPresence::NotInstalled));
        let at = |v: &str| PawnIoPresence::Installed { version: v.into() };
        assert!(install_needed(&at("2.0.1")));
        assert!(install_needed(&at("2.1.9")));
        assert!(install_needed(&at("")), "unreadable: the installer repairs it");
        assert!(install_needed(&at("garbage")));
        assert!(!install_needed(&at("2.2.0")));
        assert!(!install_needed(&at("2.2.1")));
        assert!(!install_needed(&at("2.10.0")), "numeric, not lexical");
        assert!(!install_needed(&at("3.0")));
    }

    #[test]
    fn setup_contract() {
        assert_eq!(
            setup_args(SetupAction::Install),
            ["--pawnio-install", "--smbus", "--pause"]
        );
        assert_eq!(
            setup_args(SetupAction::Remove),
            ["--smbus", "--uninstall", "--pause"]
        );
        let (ok, line) = describe_exit(SetupAction::Install, exit::OK);
        assert!(ok && line.contains("set up"));
        let (ok, line) = describe_exit(SetupAction::Remove, exit::OK);
        assert!(ok && line.contains("PawnIO stays installed"));
        let (ok, line) = describe_exit(SetupAction::Install, exit::PAWNIO_UNAVAILABLE);
        assert!(!ok && line.contains("RAM lighting stays off"));
        let (ok, line) = describe_exit(SetupAction::Remove, exit::CANCELLED);
        assert!(!ok && line.contains("declined"));
        let (ok, line) = describe_exit(SetupAction::Remove, 7);
        assert!(!ok && line.contains("exit code 7"));
    }

    #[test]
    fn command_lines_round_trip() {
        assert_eq!(command_line(&["--smbus", "--plan"]), "--smbus --plan");
        assert_eq!(
            command_line(&["--pawnio-modules", r"C:\Program Files\Lightloom\pawnio modules\"]),
            r#"--pawnio-modules "C:\Program Files\Lightloom\pawnio modules\\""#
        );
        assert_eq!(command_line(&[""]), r#""""#);
        assert_eq!(command_line(&[r#"say "hi""#]), r#""say \"hi\"""#);
        assert_eq!(
            command_line(&[r"a\\b"]),
            r"a\\b",
            "backslashes alone stay as they are"
        );
        assert_eq!(command_line(&[r#"a\"b"#]), r#""a\\\"b""#);
    }

    #[test]
    fn presence_messages() {
        assert_eq!(
            message(&PawnIoPresence::NotInstalled, true),
            None,
            "the broker answers"
        );
        let missing = message(&PawnIoPresence::NotInstalled, false).unwrap();
        assert!(missing.contains("not installed") && missing.contains("RAM lighting is off"));
        assert!(
            missing.contains("PawnIO / RAM lighting") && missing.contains("GPU lighting works without it")
        );
        assert!(missing.contains("lightloom-setup --pawnio-install --smbus"));
        let old = message(
            &PawnIoPresence::Installed {
                version: "2.0.1".into(),
            },
            false,
        )
        .unwrap();
        assert!(old.contains("it needs 2.2.0 or later"), "{old}");
        let no_broker = message(
            &PawnIoPresence::Installed {
                version: "2.2.0".into(),
            },
            false,
        )
        .unwrap();
        assert!(no_broker
            .contains("PawnIO 2.2.0 is installed but the Lightloom I/O broker service is not running"));
        let unknown = message(
            &PawnIoPresence::Installed {
                version: String::new(),
            },
            false,
        )
        .unwrap();
        assert!(
            unknown.starts_with("RAM lighting: PawnIO is installed"),
            "{unknown}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn presence_reads_without_touching_the_device() {
        // Whatever this machine has, the answer is one of the two shapes.
        let _ = pawnio_presence();
    }
}
