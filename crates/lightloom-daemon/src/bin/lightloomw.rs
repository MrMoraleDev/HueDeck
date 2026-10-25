// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloomw`: start `lightloomd` in the background, without a console window
//! (the installer's Start-menu shortcut and sign-in entry run this).
//!
//! It is a GUI-subsystem program, so nothing flashes on screen. When the
//! daemon already listens it only opens the dashboard (`--open`); otherwise
//! it starts `lightloomd.exe` from its own directory with a hidden console
//! (`CREATE_NO_WINDOW`: the daemon still gets the log-off and shut-down
//! events and restores the devices), its output in `<data dir>\logs\
//! lightloomd.log` (the previous run's in `lightloomd.old.log`), and exits.
//! Other arguments go to `lightloomd` as they are.
//!
//! `lightloomw --stop` asks a running daemon to stop (`POST /v1/shutdown` with
//! the token from the data directory, so every plugin restores its device)
//! and waits up to 15 s for it to end: the installer runs it before it
//! replaces or removes the files. It closes the tray icon too.
//!
//! The tray: when `lightloom-tray.exe` sits next to it, `lightloomw` also starts
//! it (after starting the daemon, or when one already runs; the tray keeps
//! one instance per session itself). `--no-tray` skips it.
//!
//! `lightloomw --autostart on|off` writes or removes the per-user sign-in entry
//! (`HKCU\...\Run`, value `Lightloom` = this executable), the same value as
//! the tray's *Start at sign-in*. The installer runs it on a first install
//! (option checked) and on uninstall, so the entry belongs to the user: a
//! repair or an upgrade does not bring back an entry they turned off.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use lightloom_daemon::config::Config;
use lightloom_daemon::paths::Paths;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What `lightloomw` was asked to do.
#[derive(Debug, Default, PartialEq)]
struct Args {
    open: bool,
    stop: bool,
    no_tray: bool,
    /// `--autostart on|off`.
    autostart: Option<bool>,
    /// The arguments for `lightloomd`.
    rest: Vec<OsString>,
}

fn split_args(args: impl Iterator<Item = OsString>) -> Args {
    let mut out = Args::default();
    let mut args = args.peekable();
    while let Some(a) = args.next() {
        if a == "--open" {
            out.open = true;
        } else if a == "--stop" {
            out.stop = true;
        } else if a == "--no-tray" {
            out.no_tray = true;
        } else if a == "--autostart" {
            out.autostart = match args.peek().and_then(|v| v.to_str()) {
                Some("on") => Some(true),
                Some("off") => Some(false),
                _ => None,
            };
            if out.autostart.is_some() {
                args.next();
            }
        } else {
            out.rest.push(a);
        }
    }
    out
}

/// The shutdown request for the daemon on `port`.
fn shutdown_request(port: u16, token: &str) -> String {
    format!(
        "POST /v1/shutdown HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\n\
         Content-Length: 0\r\nConnection: close\r\n\r\n",
        token.trim()
    )
}

/// Ask the daemon on `port` to stop and wait for it to go.
fn stop_daemon(port: u16, token: &str) {
    if let Ok(mut s) = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(1),
    ) {
        let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
        if s.write_all(shutdown_request(port, token).as_bytes()).is_ok() {
            let _ = s.read(&mut [0u8; 512]);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while daemon_listens(port) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Keep the previous run's log as `*.old.log`; the new run's log files.
fn log_files(dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let out = dir.join("lightloomd.log");
    let err = dir.join("lightloomd.err.log");
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

/// `name` (plus `.exe` on Windows) next to this executable, if it is there.
fn sibling(name: &str) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(name)).filter(|p| p.is_file())
}

fn daemon_exe() -> Option<PathBuf> {
    sibling("lightloomd")
}

/// Start the tray icon when it ships next to us (it exits at once if one
/// already runs in this session).
fn start_tray(skip: bool) {
    if skip {
        return;
    }
    if let Some(tray) = sibling("lightloom-tray") {
        let mut cmd = Command::new(&tray);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(dir) = tray.parent() {
            cmd.current_dir(dir);
        }
        let _ = cmd.spawn();
    }
}

fn main() {
    let Args {
        open,
        stop,
        no_tray,
        autostart,
        rest: args,
    } = split_args(std::env::args_os().skip(1));
    if let Some(on) = autostart {
        if let Ok(exe) = std::env::current_exe() {
            let _ = lightloom_tray::set_user_autostart(on, &exe);
        }
        return;
    }
    let paths = Paths::from_env();
    // A running daemon writes its port to `api-port`; else the port it would choose.
    let port = lightloom_runtime::dirs::read_api_port(&paths.data)
        .or_else(|| lightloom_daemon::port::saved_port(&paths.state_db()))
        .or_else(|| Config::load(&paths.config_file()).ok().and_then(|c| c.api.port))
        .unwrap_or(lightloom_api::NATIVE_PORT);
    if stop {
        if daemon_listens(port) {
            let token = std::fs::read_to_string(paths.token_file()).unwrap_or_default();
            stop_daemon(port, &token);
        }
        // The tray too, so its executable is not in use.
        #[cfg(windows)]
        lightloom_tray::close_tray(Duration::from_secs(3));
        return;
    }
    if daemon_listens(port) {
        if open {
            lightloom_daemon::open_in_browser(&lightloom_api::native_url(port));
        }
        start_tray(no_tray);
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
    if cmd.spawn().is_ok() {
        start_tray(no_tray);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_is_ours_the_rest_goes_on() {
        let a = split_args(["--open", "--log", "debug"].into_iter().map(OsString::from));
        assert!(a.open && !a.stop && !a.no_tray && a.autostart.is_none());
        assert_eq!(a.rest, ["--log", "debug"]);
        assert_eq!(split_args(std::iter::empty()), Args::default());
        assert!(split_args(std::iter::once(OsString::from("--stop"))).stop);
        assert!(split_args(std::iter::once(OsString::from("--no-tray"))).no_tray);
        let a = split_args(["--autostart", "on"].into_iter().map(OsString::from));
        assert_eq!((a.autostart, a.rest.len()), (Some(true), 0));
        let a = split_args(["--autostart", "off"].into_iter().map(OsString::from));
        assert_eq!(a.autostart, Some(false));
        // Without on/off nothing changes, and the next argument is not eaten.
        let a = split_args(["--autostart", "--open"].into_iter().map(OsString::from));
        assert!(a.autostart.is_none() && a.open);
    }

    #[test]
    fn the_shutdown_request_passes_the_api_guard() {
        let r = shutdown_request(53000, "abcd\r\n");
        assert!(r.starts_with("POST /v1/shutdown HTTP/1.1\r\n"));
        assert!(r.contains("\r\nHost: 127.0.0.1:53000\r\n"), "a loopback Host");
        assert!(
            r.contains("\r\nAuthorization: Bearer abcd\r\n"),
            "the token, trimmed"
        );
        assert!(r.contains("\r\nContent-Length: 0\r\n"));
        assert!(r.ends_with("\r\n\r\n") && !r.contains(" \r\n"));
    }

    #[test]
    fn logs_rotate_once() {
        let dir = std::env::temp_dir().join(format!("lightloomw-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (out, _) = log_files(&dir).unwrap();
        std::fs::write(&out, "first run").unwrap();
        let (out, _) = log_files(&dir).unwrap();
        assert!(!out.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("lightloomd.old.log")).unwrap(),
            "first run"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
