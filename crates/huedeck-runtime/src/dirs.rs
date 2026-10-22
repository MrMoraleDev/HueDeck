// SPDX-License-Identifier: GPL-3.0-or-later
//! Where HueDeck keeps its files, shared by `huedeckd` (which owns them) and
//! `huedeck` (which finds the daemon through them).
//!
//! * **data** (machine-local): `%LOCALAPPDATA%\HueDeck` on Windows,
//!   `$XDG_DATA_HOME/huedeck` (else `~/.local/share/huedeck`) elsewhere;
//! * **config** (roams with the user): `%APPDATA%\HueDeck`,
//!   `$XDG_CONFIG_HOME/huedeck` (else `~/.config/huedeck`).
//!
//! `HUEDECK_HOME` puts both under one directory (`data`, `config`).
//!
//! The data directory holds [`API_PORT_FILE`]: the port the daemon's native
//! API and dashboard listen on now, rewritten when it changes and removed
//! when the daemon exits.

use std::path::{Path, PathBuf};

/// Name of the file (in the data directory) with the native API port.
pub const API_PORT_FILE: &str = "api-port";

/// `(data, config)` under one root (`--home`, `HUEDECK_HOME`, tests).
pub fn under(root: &Path) -> (PathBuf, PathBuf) {
    (root.join("data"), root.join("config"))
}

/// `(data, config)`: the platform defaults, or `HUEDECK_HOME`.
pub fn from_env() -> (PathBuf, PathBuf) {
    if let Some(home) = std::env::var_os("HUEDECK_HOME").filter(|h| !h.is_empty()) {
        return under(Path::new(&home));
    }
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = env("HOME")
        .or_else(|| env("USERPROFILE"))
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(windows) {
        (
            env("LOCALAPPDATA")
                .unwrap_or_else(|| home.join("AppData").join("Local"))
                .join("HueDeck"),
            env("APPDATA")
                .unwrap_or_else(|| home.join("AppData").join("Roaming"))
                .join("HueDeck"),
        )
    } else {
        (
            env("XDG_DATA_HOME")
                .unwrap_or_else(|| home.join(".local").join("share"))
                .join("huedeck"),
            env("XDG_CONFIG_HOME")
                .unwrap_or_else(|| home.join(".config"))
                .join("huedeck"),
        )
    }
}

/// The port in `<data>/api-port`, if the file is there and holds one.
pub fn read_api_port(data: &Path) -> Option<u16> {
    std::fs::read_to_string(data.join(API_PORT_FILE))
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|p| *p != 0)
}

/// Write `port` to `<data>/api-port` (through a temporary file, so readers
/// never see half a number).
pub fn write_api_port(data: &Path, port: u16) -> std::io::Result<()> {
    std::fs::create_dir_all(data)?;
    let tmp = data.join(format!("{API_PORT_FILE}.tmp"));
    std::fs::write(&tmp, format!("{port}\n"))?;
    std::fs::rename(&tmp, data.join(API_PORT_FILE))
}

/// Remove `<data>/api-port` if it still names `port` (another daemon may
/// have written its own since).
pub fn remove_api_port(data: &Path, port: u16) {
    if read_api_port(data) == Some(port) {
        let _ = std::fs::remove_file(data.join(API_PORT_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_api_port_file_is_written_rewritten_and_removed() {
        let dir = std::env::temp_dir().join(format!("huedeck-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = dir.join("data");
        assert_eq!(read_api_port(&data), None);
        write_api_port(&data, 53000).unwrap();
        assert_eq!(read_api_port(&data), Some(53000));
        write_api_port(&data, 17000).unwrap();
        assert_eq!(read_api_port(&data), Some(17000));
        assert!(!data.join("api-port.tmp").exists());
        remove_api_port(&data, 53000);
        assert_eq!(read_api_port(&data), Some(17000), "another port stays");
        remove_api_port(&data, 17000);
        assert_eq!(read_api_port(&data), None);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join(API_PORT_FILE), "garbage").unwrap();
        assert_eq!(read_api_port(&data), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_home_holds_both_trees() {
        let (data, config) = under(Path::new("/tmp/hd"));
        assert_eq!(data, Path::new("/tmp/hd/data"));
        assert_eq!(config, Path::new("/tmp/hd/config"));
    }
}
