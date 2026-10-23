// SPDX-License-Identifier: GPL-3.0-or-later
//! `--pawnio-install`: install the official PawnIO driver when it is
//! missing or older than 2.2.0, from a verified download of its installer.
//!
//! The installer comes from one pinned URL ([`PAWNIO_SETUP`]); its bytes must
//! match the pinned size and SHA-256 before they are written, again once
//! written (into `%ProgramFiles%\HueDeck\downloads`, where only
//! administrators write), and its Authenticode signature must verify with
//! the pinned signer before it runs. Any failure leaves the machine as it
//! was. HueDeck never installs PawnIO without the user asking for it.

use huedeck_iobroker::pins::{self, DownloadPin};
use std::path::{Path, PathBuf};

/// `PawnIO_setup.exe` 2.2.0 → `PawnIO_setup-2.2.0.exe`.
pub fn file_name(pin: &DownloadPin) -> String {
    match pin.name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-{}.{ext}", pin.version),
        None => format!("{}-{}", pin.name, pin.version),
    }
}

/// Check downloaded `bytes` against `pin`, write them into `dir`, read the
/// file back and check it again. The path of the checked file.
pub fn store_verified(dir: &Path, pin: &DownloadPin, bytes: &[u8]) -> Result<PathBuf, String> {
    pins::verify_download(pin, bytes).map_err(|e| format!("download of {}: {e}", pin.url))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(file_name(pin));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let written = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()));
    match written.and_then(|w| pins::verify_download(pin, &w).map_err(|e| format!("{}: {e}", path.display())))
    {
        Ok(()) => Ok(path),
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            Err(e)
        }
    }
}

#[cfg(windows)]
pub use win::ensure_pawnio;

#[cfg(windows)]
mod win {
    use super::*;
    use huedeck_hal_windows::pawnio::{install_needed, pawnio_presence, PawnIoPresence};
    use huedeck_iobroker::pins::PAWNIO_SETUP;

    fn describe(p: &PawnIoPresence) -> String {
        match p {
            PawnIoPresence::NotInstalled => "not installed".into(),
            PawnIoPresence::Installed { version } if version.trim().is_empty() => {
                "installed, version unknown".into()
            }
            PawnIoPresence::Installed { version } => format!("{} (older than 2.2.0)", version.trim()),
        }
    }

    /// Install PawnIO when needed. `Ok(true)`: a plan that would install it.
    pub fn ensure_pawnio(downloads: &Path, plan: bool) -> Result<bool, String> {
        let presence = pawnio_presence();
        if !install_needed(&presence) {
            if let PawnIoPresence::Installed { version } = &presence {
                println!("PawnIO {} is installed: kept as it is", version.trim());
            }
            return Ok(false);
        }
        let pin = &PAWNIO_SETUP;
        println!("PawnIO: {}", describe(&presence));
        if plan {
            println!(
                "would download {} ({} bytes, SHA-256 {}, signed by {}) into {} and run it with -install -silent",
                pin.url,
                pin.size,
                pin.sha256,
                pin.signer,
                downloads.display()
            );
            return Ok(true);
        }
        println!("downloading {}", pin.url);
        let bytes = crate::net::download(pin.url, pin.size)?;
        let path = store_verified(downloads, pin, &bytes)?;
        println!("  SHA-256 {} matches the pin", pin.sha256);
        let ran = run_installer(&path, pin);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(downloads);
        ran?;
        let after = pawnio_presence();
        if install_needed(&after) {
            return Err(format!(
                "the PawnIO installer finished, but PawnIO is {}",
                describe(&after)
            ));
        }
        if let PawnIoPresence::Installed { version } = &after {
            println!("PawnIO {} installed", version.trim());
        }
        Ok(false)
    }

    fn run_installer(path: &Path, pin: &DownloadPin) -> Result<(), String> {
        crate::trust::check_signer(path, pin.signer)?;
        println!("  Authenticode signature valid, signed by {}", pin.signer);
        println!("running {} -install -silent", path.display());
        let status = std::process::Command::new(path)
            .args(["-install", "-silent"])
            .status()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !status.success() {
            return Err(format!("the PawnIO installer ended with {status}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Vec<u8>, DownloadPin) {
        let bytes: Vec<u8> = b"MZ fixture installer "
            .iter()
            .copied()
            .cycle()
            .take(4096)
            .collect();
        let pin = DownloadPin {
            name: "Fixture_setup.exe",
            version: "9.8.7",
            url: "https://example.invalid/Fixture_setup.exe",
            size: bytes.len(),
            sha256: Box::leak(pins::sha256_hex(&bytes).into_boxed_str()),
            signer: "example",
        };
        (bytes, pin)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("huedeck-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn names_carry_the_version() {
        assert_eq!(
            file_name(&huedeck_iobroker::pins::PAWNIO_SETUP),
            "PawnIO_setup-2.2.0.exe"
        );
        let (_, mut pin) = fixture();
        pin.name = "noext";
        assert_eq!(file_name(&pin), "noext-9.8.7");
    }

    #[test]
    fn a_matching_download_is_stored() {
        let (bytes, pin) = fixture();
        let dir = temp("store-ok");
        let path = store_verified(&dir, &pin, &bytes).unwrap();
        assert_eq!(path, dir.join("Fixture_setup-9.8.7.exe"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        // Stored again over an older copy.
        std::fs::write(&path, b"stale").unwrap();
        assert_eq!(store_verified(&dir, &pin, &bytes).unwrap(), path);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_or_short_download_is_never_written() {
        let (bytes, pin) = fixture();
        let dir = temp("store-bad");
        let mut evil = bytes.clone();
        evil[100] ^= 1;
        let err = store_verified(&dir, &pin, &evil).unwrap_err();
        assert!(err.contains("does not match the pin"), "{err}");
        let err = store_verified(&dir, &pin, &bytes[..4000]).unwrap_err();
        assert!(err.contains("4000 bytes"), "{err}");
        assert!(
            store_verified(&dir, &pin, &[]).is_err(),
            "nothing downloaded (offline)"
        );
        assert!(
            !dir.join(file_name(&pin)).exists(),
            "no file is left behind for anything to run"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real download and checks, without running the installer:
    /// `cargo test -p huedeck-setup -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "downloads PawnIO_setup.exe from GitHub"]
    fn the_official_installer_verifies() {
        let pin = &huedeck_iobroker::pins::PAWNIO_SETUP;
        let bytes = crate::net::download(pin.url, pin.size).unwrap();
        let dir = temp("real");
        let path = store_verified(&dir, pin, &bytes).unwrap();
        crate::trust::check_signer(&path, pin.signer).unwrap();
        assert!(crate::trust::check_signer(&path, "someone.else").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
