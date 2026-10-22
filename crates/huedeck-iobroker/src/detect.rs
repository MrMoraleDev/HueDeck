// SPDX-License-Identifier: GPL-3.0-or-later
//! Is PawnIO usable? (research m5-privileged-buses.md §1.2, §1.4, §1.10)
//!
//! The broker requires the official PawnIO driver, 2.2.0 or later:
//!
//! 1. the device opens (it does not when the driver is not installed; only
//!    SYSTEM and elevated administrators may open it);
//! 2. `IOCTL_PIO_VERSION` answers 2.2.0 or later (2.0.x has no such IOCTL);
//! 3. the driver checks module signatures: code integrity has TESTSIGN off
//!    (the unrestricted edition is test-signed) and a copy of the Echo module
//!    with one signature byte flipped does not load.
//!
//! [`classify`] is the pure decision; [`probe`] (Windows) gathers the facts.
//! Nothing here installs or changes anything.

use crate::pawnio::{version_text, NtStatus, MIN_VERSION, VERSION_2_0};
use huedeck_protocol::broker::PawnIoEdition;

/// `CODEINTEGRITY_OPTION_TESTSIGN`.
pub const CI_TESTSIGN: u32 = 0x02;

/// What the broker found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PawnIoStatus {
    /// No PawnIO device.
    NotInstalled,
    /// The device exists but this process may not open it (not SYSTEM / admin).
    NoAccess,
    /// Opening or probing failed otherwise.
    Unavailable(NtStatus),
    /// Older than 2.2.0 (2.0.x reports as 2.0.0).
    TooOld(u32),
    /// The unrestricted edition (any module loads): refused.
    Unrestricted(u32),
    /// Usable, with this version.
    Ready(u32),
}

impl PawnIoStatus {
    /// Edition for `Health`.
    pub fn edition(self) -> PawnIoEdition {
        match self {
            PawnIoStatus::Ready(_) => PawnIoEdition::Official,
            PawnIoStatus::Unrestricted(_) => PawnIoEdition::Unrestricted,
            _ => PawnIoEdition::Unknown,
        }
    }

    /// Driver version when known.
    pub fn version(self) -> u32 {
        match self {
            PawnIoStatus::TooOld(v) | PawnIoStatus::Unrestricted(v) | PawnIoStatus::Ready(v) => v,
            _ => 0,
        }
    }

    /// Why the broker will not serve, or `None` when it will.
    pub fn problem(self) -> Option<String> {
        match self {
            PawnIoStatus::Ready(_) => None,
            PawnIoStatus::NotInstalled => Some(
                "PawnIO is not installed (official edition 2.2.0 or later, https://pawnio.eu); the broker does not install it"
                    .into(),
            ),
            PawnIoStatus::NoAccess => Some("PawnIO can only be opened by SYSTEM or an elevated administrator".into()),
            PawnIoStatus::Unavailable(s) => Some(format!("PawnIO could not be used ({s})")),
            PawnIoStatus::TooOld(v) => Some(format!(
                "PawnIO {} is too old: update it to 2.2.0 or later",
                version_text(v)
            )),
            PawnIoStatus::Unrestricted(v) => Some(format!(
                "PawnIO {} is the unrestricted (unsigned) edition, which loads any module: install the official edition",
                version_text(v)
            )),
        }
    }
}

/// The decision, from: opening the device, the version IOCTL, the
/// code-integrity options, and whether a tampered module loaded.
pub fn classify(
    open: Result<(), NtStatus>,
    version: Result<u32, NtStatus>,
    ci_options: u32,
    tamper_loaded: bool,
) -> PawnIoStatus {
    match open {
        Ok(()) => {}
        Err(NtStatus::OBJECT_NAME_NOT_FOUND | NtStatus::OBJECT_PATH_NOT_FOUND | NtStatus::NO_SUCH_DEVICE) => {
            return PawnIoStatus::NotInstalled
        }
        Err(NtStatus::ACCESS_DENIED) => return PawnIoStatus::NoAccess,
        Err(s) => return PawnIoStatus::Unavailable(s),
    }
    let v = match version {
        Ok(v) => v,
        Err(NtStatus::NOT_IMPLEMENTED) => return PawnIoStatus::TooOld(VERSION_2_0),
        Err(s) => return PawnIoStatus::Unavailable(s),
    };
    if v < MIN_VERSION {
        return PawnIoStatus::TooOld(v);
    }
    if ci_options & CI_TESTSIGN != 0 || tamper_loaded {
        return PawnIoStatus::Unrestricted(v);
    }
    PawnIoStatus::Ready(v)
}

/// What [`probe`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    /// The decision.
    pub status: PawnIoStatus,
    /// Code-integrity options (0 when unreadable).
    pub ci_options: u32,
}

#[cfg(windows)]
pub use win::probe;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::pawnio::{code_integrity_options, PawnExec, PawnIoDevice};
    use crate::pins::{self, ECHO};
    use std::path::Path;

    /// Load a copy of Echo with one signature byte flipped on a fresh handle;
    /// whether the driver accepted it (only an unrestricted driver does).
    fn tampered_echo_loads(echo: &[u8]) -> bool {
        let mut tampered = echo.to_vec();
        // Blob layout: u32 LE signature length, the signature, the AMX image.
        if tampered.len() > 4 {
            tampered[4] ^= 0x01;
        }
        match PawnIoDevice::open() {
            Ok(dev) => dev.load("Echo", &tampered).is_ok(),
            Err(_) => false,
        }
    }

    /// The genuine Echo answers `ioctl_not(x) == !x`.
    fn echo_works(echo: &[u8]) -> Result<(), NtStatus> {
        let mut module = PawnIoDevice::open()?.load("Echo", echo)?;
        let x = 0x0123_4567_89AB_CDEF_u64;
        let mut out = [0u64; 1];
        module.execute("ioctl_not", &[x], &mut out)?;
        if out[0] == !x {
            Ok(())
        } else {
            Err(NtStatus::DEVICE_PROTOCOL_ERROR)
        }
    }

    /// Gather the facts and [`classify`] them. With `modules` (the pinned
    /// blobs' directory, which the broker always passes) the Echo probes run
    /// too, and a missing or altered Echo blob makes PawnIO `Unavailable`
    /// (fail closed); without it an unrestricted driver is caught by the
    /// TESTSIGN flag only.
    pub fn probe(modules: Option<&Path>) -> Detection {
        let ci_options = code_integrity_options().unwrap_or(0);
        let (open, version) = match PawnIoDevice::open() {
            Ok(device) => (Ok(()), device.driver_version()),
            Err(s) => (Err(s), Err(s)),
        };
        let mut status = classify(open, version, ci_options, false);
        if let (PawnIoStatus::Ready(_), Some(dir)) = (status, modules) {
            let tamper_loaded = match std::fs::read(dir.join(ECHO.file)) {
                Ok(echo) if pins::verify_against(&ECHO, &echo).is_ok() => {
                    let loaded = tampered_echo_loads(&echo);
                    if let Err(s) = echo_works(&echo) {
                        return Detection {
                            status: PawnIoStatus::Unavailable(s),
                            ci_options,
                        };
                    }
                    loaded
                }
                // Without the pinned Echo the edition cannot be told apart:
                // refuse rather than trust the TESTSIGN flag alone.
                Ok(_) => {
                    tracing::error!(
                        "{} is not the pinned file: PawnIO edition not verified",
                        ECHO.file
                    );
                    return Detection {
                        status: PawnIoStatus::Unavailable(NtStatus::INVALID_SIGNATURE),
                        ci_options,
                    };
                }
                Err(e) => {
                    tracing::error!("{}: {e}: PawnIO edition not verified", ECHO.file);
                    return Detection {
                        status: PawnIoStatus::Unavailable(NtStatus::OBJECT_NAME_NOT_FOUND),
                        ci_options,
                    };
                }
            };
            status = classify(Ok(()), Ok(status.version()), ci_options, tamper_loaded);
        }
        Detection { status, ci_options }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_every_arm() {
        let ready = Ok(0x0002_0200);
        assert_eq!(
            classify(Err(NtStatus::OBJECT_NAME_NOT_FOUND), ready, 0, false),
            PawnIoStatus::NotInstalled
        );
        assert_eq!(
            classify(Err(NtStatus::ACCESS_DENIED), ready, 0, false),
            PawnIoStatus::NoAccess
        );
        assert_eq!(
            classify(Err(NtStatus::DEVICE_NOT_READY), ready, 0, false),
            PawnIoStatus::Unavailable(NtStatus::DEVICE_NOT_READY)
        );
        assert_eq!(
            classify(Ok(()), Err(NtStatus::NOT_IMPLEMENTED), 0, false),
            PawnIoStatus::TooOld(VERSION_2_0),
            "2.0.x has no version IOCTL"
        );
        assert_eq!(
            classify(Ok(()), Ok(0x0002_0100), 0, false),
            PawnIoStatus::TooOld(0x0002_0100)
        );
        assert_eq!(
            classify(Ok(()), Err(NtStatus::BUFFER_TOO_SMALL), 0, false),
            PawnIoStatus::Unavailable(NtStatus::BUFFER_TOO_SMALL)
        );
        assert_eq!(
            classify(Ok(()), ready, 0xF401 | CI_TESTSIGN, false),
            PawnIoStatus::Unrestricted(0x0002_0200)
        );
        assert_eq!(
            classify(Ok(()), ready, 0xF401, true),
            PawnIoStatus::Unrestricted(0x0002_0200)
        );
        assert_eq!(
            classify(Ok(()), ready, 0xF401, false),
            PawnIoStatus::Ready(0x0002_0200)
        );
        assert_eq!(
            classify(Ok(()), Ok(0x0002_0300), 0, false),
            PawnIoStatus::Ready(0x0002_0300)
        );
    }

    #[test]
    fn problems_explain_without_installing() {
        assert_eq!(PawnIoStatus::Ready(MIN_VERSION).problem(), None);
        assert!(PawnIoStatus::NotInstalled
            .problem()
            .unwrap()
            .contains("does not install it"));
        assert!(PawnIoStatus::TooOld(0x0002_0100)
            .problem()
            .unwrap()
            .contains("2.1.0 is too old"));
        assert!(PawnIoStatus::Unrestricted(MIN_VERSION)
            .problem()
            .unwrap()
            .contains("official edition"));
        assert_eq!(
            PawnIoStatus::Ready(MIN_VERSION).edition(),
            PawnIoEdition::Official
        );
        assert_eq!(
            PawnIoStatus::Unrestricted(1).edition(),
            PawnIoEdition::Unrestricted
        );
        assert_eq!(PawnIoStatus::NotInstalled.edition(), PawnIoEdition::Unknown);
    }
}
