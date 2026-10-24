// SPDX-License-Identifier: GPL-3.0-or-later
//! The PawnIO modules the broker loads, pinned by SHA-256.
//!
//! The blobs are the signed release 0.2.11 of PawnIO.Modules
//! (https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.11,
//! LGPL-2.1-or-later), installed by `huedeck-setup --smbus` next to their
//! sources. A blob whose size or hash differs is never loaded (research
//! m5-privileged-buses.md §1.5).

use sha2::{Digest, Sha256};

/// PawnIO.Modules release the pins come from.
pub const MODULES_RELEASE: &str = "0.2.11";

/// One pinned module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
    /// Module name.
    pub name: &'static str,
    /// File name in the release.
    pub file: &'static str,
    /// Size in bytes.
    pub size: usize,
    /// Lower-case hex SHA-256 of the whole file.
    pub sha256: &'static str,
}

/// The chipset SMBus module (AMD FCH, ports 0x0B00/0x0B20).
pub const SMBUS_PIIX4: Pin = Pin {
    name: "SmbusPIIX4",
    file: "SmbusPIIX4.bin",
    size: 42404,
    sha256: "91f9b4b1c39e3d399ce48477a89d8f6bd3e58a2241064a4daec3a1513dff56e5",
};

/// The Intel PCH SMBus module (i801 family, MMIO BAR of 00:1F.4/1F.3/1F.1/80:1F.4).
pub const SMBUS_I801: Pin = Pin {
    name: "SmbusI801",
    file: "SmbusI801.bin",
    size: 55540,
    sha256: "a0f7d066e7efda28c0e754c1f52dbd8dc280d388ba8575b514a80cb67490530c",
};

/// The chipset SMBus modules, in the order the broker tries them: each
/// refuses (`STATUS_NOT_SUPPORTED`) a chipset it does not drive.
pub const CHIPSET_MODULES: &[Pin] = &[SMBUS_PIIX4, SMBUS_I801];

/// The harmless probe module (`ioctl_not`), used to detect an unrestricted driver.
pub const ECHO: Pin = Pin {
    name: "Echo",
    file: "Echo.bin",
    size: 2284,
    sha256: "96cb194cbbf1bd3ced0c0d5e2ac8ec55a5f8964b305dac005a30ac9b72787696",
};

/// Every module the broker may load.
pub const MODULES: &[Pin] = &[SMBUS_PIIX4, SMBUS_I801, ECHO];

/// Why a blob is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinError {
    /// Not a module the broker loads.
    Unknown(String),
    /// Wrong size.
    Size {
        /// Module.
        name: &'static str,
        /// Bytes found.
        found: usize,
    },
    /// Wrong SHA-256.
    Hash {
        /// Module.
        name: &'static str,
        /// Hash found.
        found: String,
    },
}

impl std::fmt::Display for PinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PinError::Unknown(n) => write!(f, "{n} is not a pinned PawnIO module"),
            PinError::Size { name, found } => write!(f, "{name}: {found} bytes, not the pinned release file"),
            PinError::Hash { name, found } => write!(f, "{name}: SHA-256 {found} does not match the pin"),
        }
    }
}

impl std::error::Error for PinError {}

/// Lower-case hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The pin of module `name`.
pub fn pin(name: &str) -> Option<&'static Pin> {
    MODULES.iter().find(|p| p.name == name)
}

/// Check `bytes` against `pin`.
pub fn verify_against(pin: &Pin, bytes: &[u8]) -> Result<(), PinError> {
    if bytes.len() != pin.size {
        return Err(PinError::Size {
            name: pin.name,
            found: bytes.len(),
        });
    }
    let found = sha256_hex(bytes);
    if found != pin.sha256 {
        return Err(PinError::Hash {
            name: pin.name,
            found,
        });
    }
    Ok(())
}

/// Check `bytes` against the pin of module `name`.
pub fn verify(name: &str, bytes: &[u8]) -> Result<&'static Pin, PinError> {
    let pin = pin(name).ok_or_else(|| PinError::Unknown(name.to_string()))?;
    verify_against(pin, bytes)?;
    Ok(pin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_reject_flipped_byte() {
        let blob: Vec<u8> = (0..64u8).collect();
        let pin = Pin {
            name: "Test",
            file: "Test.bin",
            size: blob.len(),
            sha256: Box::leak(sha256_hex(&blob).into_boxed_str()),
        };
        assert!(verify_against(&pin, &blob).is_ok());
        let mut flipped = blob.clone();
        flipped[10] ^= 0x01;
        assert!(matches!(
            verify_against(&pin, &flipped),
            Err(PinError::Hash { .. })
        ));
        assert!(matches!(
            verify_against(&pin, &blob[1..]),
            Err(PinError::Size { found: 63, .. })
        ));
    }

    #[test]
    fn pinned_modules_are_well_formed() {
        for p in MODULES {
            assert_eq!(p.sha256.len(), 64, "{}", p.name);
            assert!(p
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert_eq!(p.file, format!("{}.bin", p.name));
        }
        assert_eq!(pin("SmbusPIIX4"), Some(&SMBUS_PIIX4));
        assert_eq!(pin("SmbusI801"), Some(&SMBUS_I801));
        assert!(CHIPSET_MODULES.iter().all(|c| MODULES.contains(c)));
        assert!(pin("SmbusIntelSkylakeIMC").is_none(), "not pinned yet");
        assert!(matches!(verify("SmbusNCT6793", &[]), Err(PinError::Unknown(_))));
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
