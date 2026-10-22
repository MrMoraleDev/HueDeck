// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the daemon keeps things.
//!
//! * **data** (machine-local): `state.sqlite`, the API token, the instance
//!   lock, the renderer's work files: `%LOCALAPPDATA%\Lightloom` on Windows,
//!   `$XDG_DATA_HOME/lightloom` (else `~/.local/share/lightloom`) elsewhere.
//! * **config** (roams with the user): `config.toml` and the user's own
//!   `plugins`, `effects` and `components` directories: `%APPDATA%\Lightloom`,
//!   `$XDG_CONFIG_HOME/lightloom` (else `~/.config/lightloom`).
//! * **resources** shipped with Lightloom (first-party plugins, starter effects,
//!   the dashboard): next to the executable, else (development builds) in the
//!   source tree.
//!
//! `LIGHTLOOM_HOME` (or `--home`) puts data and config under one directory.

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
    /// The platform defaults, or `LIGHTLOOM_HOME`.
    pub fn from_env() -> Self {
        if let Some(home) = std::env::var_os("LIGHTLOOM_HOME").filter(|h| !h.is_empty()) {
            return Self::under(Path::new(&home));
        }
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = env("HOME")
            .or_else(|| env("USERPROFILE"))
            .unwrap_or_else(|| PathBuf::from("."));
        if cfg!(windows) {
            Self {
                data: env("LOCALAPPDATA")
                    .unwrap_or_else(|| home.join("AppData").join("Local"))
                    .join("Lightloom"),
                config: env("APPDATA")
                    .unwrap_or_else(|| home.join("AppData").join("Roaming"))
                    .join("Lightloom"),
            }
        } else {
            Self {
                data: env("XDG_DATA_HOME")
                    .unwrap_or_else(|| home.join(".local").join("share"))
                    .join("lightloom"),
                config: env("XDG_CONFIG_HOME")
                    .unwrap_or_else(|| home.join(".config"))
                    .join("lightloom"),
            }
        }
    }

    /// Data and config under one directory (`--home`, tests).
    pub fn under(root: &Path) -> Self {
        Self {
            data: root.join("data"),
            config: root.join("config"),
        }
    }

    /// The database.
    pub fn state_db(&self) -> PathBuf {
        self.data.join("state.sqlite")
    }

    /// The API token.
    pub fn token_file(&self) -> PathBuf {
        self.data.join("api-token")
    }

    /// The single-instance lock.
    pub fn lock_file(&self) -> PathBuf {
        self.data.join("lightloomd.lock")
    }

    /// The renderer's ring, profile and logs.
    pub fn renderer_work(&self) -> PathBuf {
        self.data.join("renderer")
    }

    /// `config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    /// The user's own plugins.
    pub fn user_plugins(&self) -> PathBuf {
        self.config.join("plugins")
    }

    /// The user's own effects.
    pub fn user_effects(&self) -> PathBuf {
        self.config.join("effects")
    }

    /// The user's own component descriptors.
    pub fn user_components(&self) -> PathBuf {
        self.config.join("components")
    }
}

/// A resource directory shipped with Lightloom: `<exe dir>/<name>`, else, in a
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
        "lightloom-renderer.exe"
    } else {
        "lightloom-renderer"
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
