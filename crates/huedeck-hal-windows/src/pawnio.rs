// SPDX-License-Identifier: GPL-3.0-or-later
//! Is the PawnIO driver installed? Read from its uninstall entry only.
//!
//! RAM lighting reaches the chipset SMBus through the HueDeck I/O broker,
//! which needs the PawnIO driver (official edition, 2.2.0 or later). This
//! unprivileged process never opens the PawnIO device: it reads
//! `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO`
//! (`DisplayVersion`, as LibreHardwareMonitor does) to tell the user what is
//! missing, and [`message`] says it. HueDeck never installs PawnIO.

/// PawnIO's uninstall key.
pub const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO";
/// Oldest PawnIO HueDeck uses (`maj<<16 | min<<8 | patch`).
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

/// What to tell the user about RAM lighting, or `None` when the broker answers.
pub fn message(presence: &PawnIoPresence, broker_up: bool) -> Option<String> {
    if broker_up {
        return None;
    }
    Some(match presence {
        PawnIoPresence::NotInstalled => "RAM lighting needs the PawnIO driver (official edition 2.2.0 or later, \
             https://pawnio.eu), which is not installed on this PC. HueDeck never installs it for you. Install it \
             yourself, then register the HueDeck I/O broker once with `huedeck-setup --smbus` (one UAC prompt). \
             GPU lighting works without it."
            .to_string(),
        PawnIoPresence::Installed { version } if parse_version(version).is_some_and(|v| v < MIN_VERSION) => {
            format!(
                "RAM lighting: PawnIO {version} is installed; update PawnIO to 2.2.0 or later (https://pawnio.eu), \
                 then register the HueDeck I/O broker once with `huedeck-setup --smbus`."
            )
        }
        PawnIoPresence::Installed { version } => {
            let v = if version.trim().is_empty() { "" } else { " " };
            format!(
                "RAM lighting: PawnIO{v}{} is installed but the HueDeck I/O broker service is not running; run \
                 `huedeck-setup --smbus` once (one UAC prompt).",
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
    fn presence_messages() {
        assert_eq!(
            message(&PawnIoPresence::NotInstalled, true),
            None,
            "the broker answers"
        );
        let missing = message(&PawnIoPresence::NotInstalled, false).unwrap();
        assert!(missing.contains("not installed") && missing.contains("never installs"));
        assert!(
            missing.contains("huedeck-setup --smbus") && missing.contains("GPU lighting works without it")
        );
        let old = message(
            &PawnIoPresence::Installed {
                version: "2.0.1".into(),
            },
            false,
        )
        .unwrap();
        assert!(old.contains("update PawnIO to 2.2.0 or later"), "{old}");
        let no_broker = message(
            &PawnIoPresence::Installed {
                version: "2.2.0".into(),
            },
            false,
        )
        .unwrap();
        assert!(
            no_broker.contains("PawnIO 2.2.0 is installed but the HueDeck I/O broker service is not running")
        );
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
