// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the daemon keeps things.
//!
//! * **data** (machine-local): `state.sqlite`, the API token, `api-port`, the instance
//!   lock, the renderer's work files: `%LOCALAPPDATA%\HueDeck` on Windows,
//!   `$XDG_DATA_HOME/huedeck` (else `~/.local/share/huedeck`) elsewhere.
//! * **config** (roams with the user): `config.toml` and the user's own
//!   `plugins`, `effects` and `components` directories: `%APPDATA%\HueDeck`,
//!   `$XDG_CONFIG_HOME/huedeck` (else `~/.config/huedeck`).
//! * **resources** shipped with HueDeck (first-party plugins, starter effects,
//!   the dashboard): next to the executable, else (development builds) in the
//!   source tree.
//!
//! `HUEDECK_HOME` (or `--home`) puts data and config under one directory.

use std::path::{Path, PathBuf};

/// The daemon's directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// Machine-local state.
    pub data: PathBuf,
    /// User configuration and content.
    pub config: PathBuf,
}

impl Paths {
    /// The platform defaults, or `HUEDECK_HOME` ([`huedeck_runtime::dirs`]).
    pub fn from_env() -> Self {
        let (data, config) = huedeck_runtime::dirs::from_env();
        Self { data, config }
    }

    /// Data and config under one directory (`--home`, tests).
    pub fn under(root: &Path) -> Self {
        let (data, config) = huedeck_runtime::dirs::under(root);
        Self { data, config }
    }

    /// The database.
    pub fn state_db(&self) -> PathBuf {
        self.data.join("state.sqlite")
    }

    /// The API token.
    pub fn token_file(&self) -> PathBuf {
        self.data.join("api-token")
    }

    /// The port the native API serves on now (for `huedeck`).
    pub fn port_file(&self) -> PathBuf {
        self.data.join(huedeck_runtime::dirs::API_PORT_FILE)
    }

    /// The single-instance lock.
    pub fn lock_file(&self) -> PathBuf {
        self.data.join("huedeckd.lock")
    }

    /// The renderer's ring, profile and logs.
    pub fn renderer_work(&self) -> PathBuf {
        self.data.join("renderer")
    }

    /// `config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    /// One line per "Save to devices" attempt (what was stored in which device, when).
    pub fn save_audit_log(&self) -> PathBuf {
        self.data.join("save-audit.log")
    }

    /// The user's own plugins.
    pub fn user_plugins(&self) -> PathBuf {
        self.config.join("plugins")
    }

    /// The user's own effects.
    pub fn user_effects(&self) -> PathBuf {
        self.config.join("effects")
    }

    /// Effect pages loaded through the compatible API's `lighting/loadEffect`.
    pub fn developer_effects(&self) -> PathBuf {
        self.data.join("developer-effects")
    }

    /// The user's own component descriptors.
    pub fn user_components(&self) -> PathBuf {
        self.config.join("components")
    }
}

/// A resource directory shipped with HueDeck: `<exe dir>/<name>`, else, in a
/// development build, `<source tree>/<dev>` (for the dashboard, `ui/dist`).
pub fn resource_dir(name: &str, dev: &str) -> Option<PathBuf> {
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join(name)))
        .filter(|d| d.is_dir())
    {
        return Some(dir);
    }
    let tree = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(dev);
    tree.is_dir()
        .then(|| std::fs::canonicalize(&tree).unwrap_or(tree))
}

/// The renderer executable next to this one, if it is there.
pub fn renderer_next_to_exe() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "huedeck-renderer.exe"
    } else {
        "huedeck-renderer"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join(name)))
        .filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_holds_both_trees() {
        let p = Paths::under(Path::new("/tmp/hd"));
        assert_eq!(p.state_db(), Path::new("/tmp/hd/data/state.sqlite"));
        assert_eq!(p.config_file(), Path::new("/tmp/hd/config/config.toml"));
        assert!(p.user_effects().ends_with("effects"));
    }

    #[test]
    fn development_builds_find_the_source_tree_resources() {
        // The workspace has starter effects and first-party plugins.
        assert!(resource_dir("effects", "effects").is_some());
        assert!(resource_dir("plugins", "plugins").is_some());
        assert!(resource_dir("no-such-dir", "no-such-dir").is_none());
    }
}
