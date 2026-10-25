// SPDX-License-Identifier: GPL-3.0-or-later
//! Build-script helper: embed the HueDeck icon (`packaging/wix/huedeck.ico`)
//! and a version resource (product name, file description, version, licence)
//! into the Windows executables, so Explorer, the taskbar, the UAC prompt and
//! Apps & features show them.
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     huedeck_winres::embed("HueDeck daemon");
//! }
//! ```
//!
//! Only a Windows target built on a Windows host gets resources (the
//! resource compiler comes with the Windows SDK); everywhere else this does
//! nothing. A missing resource compiler is a build warning, not an error:
//! the program works the same without its icon.

#![forbid(unsafe_code)]

use std::path::PathBuf;

/// The icon, from the workspace root.
pub fn icon_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("packaging")
        .join("wix")
        .join("huedeck.ico")
}

/// `0.0.1` → `0x0000_0000_0001_0000` (`major.minor.patch.0`, the
/// `VS_FIXEDFILEINFO` layout); `None` for anything else.
pub fn version_word(version: &str) -> Option<u64> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u16>().ok());
    let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }
    Some((u64::from(major) << 48) | (u64::from(minor) << 32) | (u64::from(patch) << 16))
}

/// Embed the icon and the version resource, with `description` as the file
/// description. Call from a `build.rs`.
pub fn embed(description: &str) {
    println!("cargo::rerun-if-changed={}", icon_path().display());
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }
    #[cfg(windows)]
    {
        let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
        let mut res = winresource::WindowsResource::new();
        res.set_icon(&icon_path().to_string_lossy())
            .set("ProductName", "HueDeck")
            .set("FileDescription", description)
            .set("CompanyName", "HueDeck contributors")
            .set("LegalCopyright", "GPL-3.0-or-later, HueDeck contributors")
            .set("ProductVersion", &version)
            .set("FileVersion", &version);
        if let Some(word) = version_word(&version) {
            res.set_version_info(winresource::VersionInfo::FILEVERSION, word)
                .set_version_info(winresource::VersionInfo::PRODUCTVERSION, word);
        }
        if let Err(e) = res.compile() {
            println!("cargo::warning=no icon/version resource embedded ({e}); install the Windows SDK resource compiler");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = description;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_in_the_tree() {
        assert!(icon_path().is_file(), "{}", icon_path().display());
    }

    #[test]
    fn versions_pack_like_vs_fixedfileinfo() {
        assert_eq!(version_word("0.0.1"), Some(0x0000_0000_0001_0000));
        assert_eq!(version_word("1.2.3"), Some(0x0001_0002_0003_0000));
        assert_eq!(version_word("1.2.3-beta.1"), Some(0x0001_0002_0003_0000));
        assert_eq!(version_word("1.2"), None);
        assert_eq!(version_word("1.2.3.4"), None);
        assert_eq!(version_word("70000.0.0"), None);
    }
}
