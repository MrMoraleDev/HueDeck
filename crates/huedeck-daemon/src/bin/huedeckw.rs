// SPDX-License-Identifier: GPL-3.0-or-later
//! `huedeckw`: start `huedeckd` in the background, without a console window
//! (the installer's Start-menu shortcut and sign-in entry run this).
//!
//! It is a GUI-subsystem program, so nothing flashes on screen. When the
//! daemon already listens it only opens the dashboard (`--open`); otherwise
//! it starts `huedeckd.exe` from its own directory with a hidden console
//! (`CREATE_NO_WINDOW`: the daemon still gets the log-off and shut-down
//! events and restores the devices), its output in `<data dir>\logs\
//! huedeckd.log` (the previous run's in `huedeckd.old.log`), and exits.
//! Other arguments go to `huedeckd` as they are.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use huedeck_daemon::config::Config;
use huedeck_daemon::paths::Paths;
use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// `--open` and the arguments for `huedeckd`.
fn split_args(args: impl Iterator<Item = OsString>) -> (bool, Vec<OsString>) {
    let mut open = false;
    let mut rest = Vec::new();
    for a in args {
        if a == "--open" {
            open = true;
        } else {
            rest.push(a);
        }
    }
    (open, rest)
}

/// Keep the previous run's log as `*.old.log`; the new run's log files.
fn log_files(dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let out = dir.join("huedeckd.log");
    let err = dir.join("huedeckd.err.log");
    for f in [&out, &err] {
        if f.is_file() {
            let _ = std::fs::rename(f, f.with_extension("old.log"));
        }
    }
    Ok((out, err))
}

fn daemon_listens(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

fn daemon_exe() -> Option<PathBuf> {
    let name = if cfg!(windows) { "huedeckd.exe" } else { "huedeckd" };
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(name)).filter(|p| p.is_file())
}

fn main() {
    let (open, args) = split_args(std::env::args_os().skip(1));
    let paths = Paths::from_env();
    // A running daemon writes its port to `api-port`; else the port it would choose.
    let port = huedeck_runtime::dirs::read_api_port(&paths.data)
        .or_else(|| huedeck_daemon::port::saved_port(&paths.state_db()))
        .or_else(|| Config::load(&paths.config_file()).ok().and_then(|c| c.api.port))
        .unwrap_or(huedeck_api::NATIVE_PORT);
    if daemon_listens(port) {
        if open {
            huedeck_daemon::open_in_browser(&huedeck_api::native_url(port));
        }
        return;
    }
    let Some(exe) = daemon_exe() else {
        return; // nothing to start; no console to say so
    };
    let mut cmd = Command::new(&exe);
    cmd.args(&args).stdin(Stdio::null());
    if open {
        cmd.arg("--open");
    }
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    if let Ok((out, err)) = log_files(&paths.data.join("logs")) {
        if let (Ok(o), Ok(e)) = (std::fs::File::create(out), std::fs::File::create(err)) {
            cmd.stdout(o).stderr(e);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// A console of its own, never shown.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_is_ours_the_rest_goes_on() {
        let (open, rest) = split_args(["--open", "--log", "debug"].into_iter().map(OsString::from));
        assert!(open);
        assert_eq!(rest, ["--log", "debug"]);
        let (open, rest) = split_args(std::iter::empty());
        assert!(!open && rest.is_empty());
    }

    #[test]
    fn logs_rotate_once() {
        let dir = std::env::temp_dir().join(format!("huedeckw-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (out, _) = log_files(&dir).unwrap();
        std::fs::write(&out, "first run").unwrap();
        let (out, _) = log_files(&dir).unwrap();
        assert!(!out.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("huedeckd.old.log")).unwrap(),
            "first run"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
