// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-tray`: Lightloom's icon in the Windows notification area.
//!
//! The tray is its own small GUI-subsystem process, next to `lightloomd`; it
//! never opens a device. Everything it does goes through the daemon's
//! loopback API, found the way `lightloom` finds it (the `api-port` file and
//! the `api-token` in the data directory):
//!
//! * **Open dashboard** (also a left click): the dashboard on the port the
//!   daemon serves now; when no daemon runs, *Start Lightloom* runs
//!   `lightloomw --open` (or a hidden `lightloomd --open`) from the tray's own
//!   directory.
//! * **Pause / Resume lighting**: `PATCH /v1/lighting {"enabled"}`.
//! * **Brightness** presets: `PATCH /v1/lighting {"brightness"}`.
//! * **Profiles**: the saved profiles (`GET /v1/profiles`), the current one
//!   checked; choosing one applies it (`POST /v1/profiles/{name}/apply`).
//! * **Start at sign-in**: the `Lightloom` value under
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, the same value the
//!   installer writes (`"<install dir>\lightloomw.exe"`).
//! * **Quit Lightloom**: `POST /v1/shutdown`, so every plugin restores its
//!   device, then the tray exits too.
//!
//! The tooltip says `Lightloom — N devices` (or paused, or not running).
//!
//! This module holds the logic, with the API, the registry and the desktop
//! behind traits so the tests run without a daemon or a registry; the
//! Windows shell glue is `win.rs` in the binary. On Linux the same menu is a
//! StatusNotifierItem with a dbusmenu ([`sni`]); without a StatusNotifier
//! host on the desktop the binary says so and exits.

#![deny(unsafe_code)]

/// The registry side of [`RunKey`] (the only unsafe code in the library).
#[cfg(windows)]
#[allow(unsafe_code)]
mod runkey;
#[cfg(windows)]
pub use runkey::WinRunKey;
/// The running tray, from another process.
#[cfg(windows)]
#[allow(unsafe_code)]
mod instance;
#[cfg(windows)]
pub use instance::{close_tray, WINDOW_CLASS};
/// The Linux tray: StatusNotifierItem and dbusmenu (the menu tree builds everywhere).
pub mod sni;

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What the daemon looks like from the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// No daemon answers.
    NotRunning,
    /// A daemon serves the API.
    Running {
        /// Devices it drives (`GET /v1/devices`).
        devices: usize,
        /// `false`: lighting is paused (`GET /v1/lighting`).
        enabled: bool,
        /// Global brightness `0..=100`.
        brightness: u8,
    },
}

/// The daemon's API, as the tray uses it.
pub trait Api {
    /// The daemon's state now.
    fn status(&self) -> Status;
    /// The dashboard's URL, when a daemon runs.
    fn dashboard_url(&self) -> Option<String>;
    /// Pause (`false`) or resume (`true`) lighting.
    fn set_enabled(&self, enabled: bool) -> Result<(), String>;
    /// Global brightness `0..=100`.
    fn set_brightness(&self, brightness: u8) -> Result<(), String>;
    /// Stop the daemon (plugins restore their devices).
    fn shutdown(&self) -> Result<(), String>;
    /// The saved profile names, and the profile applied or saved last.
    fn profiles(&self) -> (Vec<String>, Option<String>) {
        (Vec::new(), None)
    }
    /// Apply a profile.
    fn apply_profile(&self, name: &str) -> Result<(), String> {
        Err(format!("cannot apply {name}"))
    }
}

/// The values under one registry key (the per-user `Run` key).
pub trait RunKey {
    /// The string value `name`, if present.
    fn get(&self, name: &str) -> io::Result<Option<String>>;
    /// Create or replace the string value `name`.
    fn set(&self, name: &str, value: &str) -> io::Result<()>;
    /// Remove the value `name` (absent is fine).
    fn delete(&self, name: &str) -> io::Result<()>;
}

/// What the tray asks of the desktop.
pub trait Desktop {
    /// Open `url` in the default browser.
    fn open_url(&self, url: &str);
    /// Start the daemon and open the dashboard once it serves; `false` when
    /// there is nothing to start.
    fn start_and_open(&self) -> bool;
}

/// The `Run` subkey under `HKEY_CURRENT_USER`.
pub const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// The value name the installer writes (`packaging/wix/lightloom.wxs`).
pub const RUN_VALUE: &str = "Lightloom";

/// Start at sign-in: the installer's `Run` value.
pub struct Autostart<K> {
    key: K,
    /// What the value should hold (`"<dir>\lightloomw.exe"`); `None` when the
    /// launcher is not next to the tray, so it cannot be turned on.
    command: Option<String>,
}

impl<K: RunKey> Autostart<K> {
    /// `command` is what turning it on writes.
    pub fn new(key: K, command: Option<String>) -> Self {
        Self { key, command }
    }

    /// The command for the launcher at `exe`, quoted as the installer writes it.
    pub fn command_for(exe: &Path) -> String {
        format!("\"{}\"", exe.display())
    }

    /// Whether Lightloom starts at sign-in (the value exists, whatever its path).
    pub fn enabled(&self) -> bool {
        matches!(self.key.get(RUN_VALUE), Ok(Some(v)) if !v.trim().is_empty())
    }

    /// Whether the toggle can do anything now (turning off always can).
    pub fn available(&self) -> bool {
        self.enabled() || self.command.is_some()
    }

    /// Turn it on or off; the new state.
    pub fn toggle(&self) -> io::Result<bool> {
        if self.enabled() {
            self.key.delete(RUN_VALUE)?;
            Ok(false)
        } else {
            let Some(cmd) = &self.command else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "lightloomw is not next to the tray, so there is nothing to start at sign-in",
                ));
            };
            self.key.set(RUN_VALUE, cmd)?;
            Ok(true)
        }
    }
}

/// Turn start at sign-in on (the value runs `launcher`) or off in `key`
/// (`lightloomw --autostart on|off`, which the installer runs).
pub fn set_autostart<K: RunKey>(key: &K, on: bool, launcher: &Path) -> io::Result<()> {
    if on {
        key.set(RUN_VALUE, &Autostart::<K>::command_for(launcher))
    } else {
        key.delete(RUN_VALUE)
    }
}

/// [`set_autostart`] on this user's `Run` key.
pub fn set_user_autostart(on: bool, launcher: &Path) -> io::Result<()> {
    #[cfg(windows)]
    return set_autostart(&WinRunKey, on, launcher);
    #[cfg(not(windows))]
    {
        let _ = (on, launcher);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "start at sign-in is a Windows setting here (use the systemd user unit)",
        ))
    }
}

/// A menu command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the dashboard (or start Lightloom).
    OpenDashboard,
    /// Pause or resume lighting.
    TogglePause,
    /// Set the global brightness.
    Brightness(u8),
    /// Start at sign-in on or off.
    ToggleAutostart,
    /// Stop the daemon and the tray.
    Quit,
    /// Apply the profile at this index of the list the menu showed.
    Profile(u8),
}

/// The brightness presets in the menu.
pub const BRIGHTNESS_PRESETS: [u8; 4] = [25, 50, 75, 100];
/// Most profiles the menu lists (the dashboard has them all).
pub const MAX_MENU_PROFILES: usize = 30;

impl Action {
    /// The menu command id (non-zero: 0 means "no choice" to the shell).
    pub fn id(self) -> u16 {
        match self {
            Action::OpenDashboard => 1,
            Action::TogglePause => 2,
            Action::ToggleAutostart => 3,
            Action::Quit => 4,
            Action::Brightness(b) => 100 + u16::from(b.min(100)),
            Action::Profile(i) => 300 + u16::from(i),
        }
    }

    /// The command for menu id `id`.
    pub fn from_id(id: u16) -> Option<Action> {
        Some(match id {
            1 => Action::OpenDashboard,
            2 => Action::TogglePause,
            3 => Action::ToggleAutostart,
            4 => Action::Quit,
            100..=200 => Action::Brightness((id - 100) as u8),
            300..=555 => Action::Profile((id - 300) as u8),
            _ => return None,
        })
    }
}

/// One line of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A command.
    Item {
        /// What it does.
        action: Action,
        /// Its text.
        label: String,
        /// A check mark.
        checked: bool,
        /// Greyed out when `false`.
        enabled: bool,
        /// The default item (bold; what a left click does).
        default: bool,
    },
    /// A line.
    Separator,
    /// A submenu.
    Submenu {
        /// Its text.
        label: String,
        /// Greyed out when `false`.
        enabled: bool,
        /// Its items.
        items: Vec<Entry>,
    },
}

fn item(action: Action, label: impl Into<String>, checked: bool, enabled: bool) -> Entry {
    Entry::Item {
        action,
        label: label.into(),
        checked,
        enabled,
        default: false,
    }
}

/// The menu for `status`; `autostart` is `(on, available)`.
pub fn menu(status: Status, autostart: (bool, bool)) -> Vec<Entry> {
    menu_with_profiles(status, autostart, &[], None)
}

/// [`menu`] with a Profiles submenu (when there are profiles): the first
/// [`MAX_MENU_PROFILES`] of `profiles`, `current` checked.
pub fn menu_with_profiles(
    status: Status,
    autostart: (bool, bool),
    profiles: &[String],
    current: Option<&str>,
) -> Vec<Entry> {
    let mut entries = base_menu(status, autostart);
    let running = status != Status::NotRunning;
    if running && !profiles.is_empty() {
        let items = profiles
            .iter()
            .take(MAX_MENU_PROFILES)
            .enumerate()
            .map(|(i, name)| {
                // `&` marks a mnemonic in a Windows menu.
                item(
                    Action::Profile(i as u8),
                    name.replace('&', "&&"),
                    current == Some(name.as_str()),
                    true,
                )
            })
            .collect();
        // After Brightness.
        let at = entries
            .iter()
            .position(|e| matches!(e, Entry::Submenu { label, .. } if label == "Brightness"))
            .map_or(entries.len(), |i| i + 1);
        entries.insert(
            at,
            Entry::Submenu {
                label: "Profiles".into(),
                enabled: true,
                items,
            },
        );
    }
    entries
}

fn base_menu(status: Status, autostart: (bool, bool)) -> Vec<Entry> {
    let running = status != Status::NotRunning;
    let (enabled, brightness) = match status {
        Status::Running {
            enabled, brightness, ..
        } => (enabled, Some(brightness)),
        Status::NotRunning => (true, None),
    };
    let presets = BRIGHTNESS_PRESETS
        .iter()
        .map(|&b| {
            item(
                Action::Brightness(b),
                format!("{b}%"),
                brightness == Some(b),
                running,
            )
        })
        .collect();
    vec![
        Entry::Item {
            action: Action::OpenDashboard,
            label: if running {
                "Open dashboard"
            } else {
                "Start Lightloom"
            }
            .into(),
            checked: false,
            enabled: true,
            default: true,
        },
        Entry::Separator,
        item(
            Action::TogglePause,
            if enabled {
                "Pause lighting"
            } else {
                "Resume lighting"
            },
            false,
            running,
        ),
        Entry::Submenu {
            label: "Brightness".into(),
            enabled: running,
            items: presets,
        },
        Entry::Separator,
        item(
            Action::ToggleAutostart,
            "Start at sign-in",
            autostart.0,
            autostart.1,
        ),
        Entry::Separator,
        item(
            Action::Quit,
            if running { "Quit Lightloom" } else { "Close" },
            false,
            true,
        ),
    ]
}

/// The tooltip for `status`.
pub fn tooltip(status: Status) -> String {
    match status {
        Status::NotRunning => "Lightloom \u{2014} not running".into(),
        Status::Running { devices, enabled, .. } => {
            let n = match devices {
                1 => "1 device".to_string(),
                n => format!("{n} devices"),
            };
            if enabled {
                format!("Lightloom \u{2014} {n}")
            } else {
                format!("Lightloom \u{2014} paused, {n}")
            }
        }
    }
}

/// After a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Keep the icon.
    Stay,
    /// Remove the icon and exit.
    Exit,
}

/// Run `action`; an error is a message for the user.
pub fn perform<A: Api, K: RunKey, D: Desktop>(
    action: Action,
    api: &A,
    autostart: &Autostart<K>,
    desktop: &D,
) -> Result<Outcome, String> {
    let status = api.status();
    match action {
        Action::OpenDashboard => {
            match api.dashboard_url().filter(|_| status != Status::NotRunning) {
                Some(url) => desktop.open_url(&url),
                None => {
                    if !desktop.start_and_open() {
                        return Err("Lightloom is not running, and lightloomd is not next to the tray".into());
                    }
                }
            }
            Ok(Outcome::Stay)
        }
        Action::TogglePause => match status {
            Status::Running { enabled, .. } => api.set_enabled(!enabled).map(|_| Outcome::Stay),
            Status::NotRunning => Err("Lightloom is not running".into()),
        },
        Action::Brightness(b) => match status {
            Status::Running { .. } => api.set_brightness(b.min(100)).map(|_| Outcome::Stay),
            Status::NotRunning => Err("Lightloom is not running".into()),
        },
        Action::ToggleAutostart => autostart
            .toggle()
            .map(|_| Outcome::Stay)
            .map_err(|e| format!("cannot change Start at sign-in: {e}")),
        Action::Quit => match status {
            Status::NotRunning => Ok(Outcome::Exit),
            Status::Running { .. } => api.shutdown().map(|_| Outcome::Exit),
        },
        Action::Profile(i) => match status {
            Status::Running { .. } => {
                let (names, _) = api.profiles();
                let name = names
                    .get(usize::from(i))
                    .ok_or("that profile is gone; open the menu again")?;
                api.apply_profile(name).map(|_| Outcome::Stay)
            }
            Status::NotRunning => Err("Lightloom is not running".into()),
        },
    }
}

/// `text` as one URL path segment (RFC 3986 unreserved characters kept).
pub fn path_segment(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl<K: RunKey> RunKey for &K {
    fn get(&self, name: &str) -> io::Result<Option<String>> {
        (**self).get(name)
    }
    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        (**self).set(name, value)
    }
    fn delete(&self, name: &str) -> io::Result<()> {
        (**self).delete(name)
    }
}

/// An in-memory [`RunKey`] (tests).
#[derive(Default)]
pub struct MemRunKey(pub std::sync::Mutex<std::collections::BTreeMap<String, String>>);

impl RunKey for MemRunKey {
    fn get(&self, name: &str) -> io::Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }
    fn delete(&self, name: &str) -> io::Result<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}

/// The daemon's loopback API over plain HTTP/1.1 (no client library: four
/// requests, all to 127.0.0.1).
pub struct HttpApi {
    data: PathBuf,
}

impl HttpApi {
    /// The daemon whose files are in the data directory `data`.
    pub fn new(data: PathBuf) -> Self {
        Self { data }
    }

    fn port(&self) -> Option<u16> {
        lightloom_runtime::dirs::read_api_port(&self.data)
    }

    fn token(&self) -> String {
        std::fs::read_to_string(self.data.join("api-token"))
            .map(|t| t.trim().to_string())
            .unwrap_or_default()
    }

    fn call(&self, method: &str, path: &str, body: Option<&str>) -> Result<Vec<u8>, String> {
        let port = self.port().ok_or("Lightloom is not running")?;
        let token = if method == "GET" { None } else { Some(self.token()) };
        let (code, body) = http(port, method, path, token.as_deref(), body)
            .map_err(|e| format!("Lightloom does not answer: {e}"))?;
        if (200..300).contains(&code) {
            Ok(body)
        } else {
            let msg = serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                .unwrap_or_else(|| format!("HTTP {code}"));
            Err(msg)
        }
    }

    fn json(&self, path: &str) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.call("GET", path, None).ok()?).ok()
    }
}

impl Api for HttpApi {
    fn status(&self) -> Status {
        let Some(lighting) = self.json("/v1/lighting") else {
            return Status::NotRunning;
        };
        let devices = self
            .json("/v1/devices")
            .and_then(|d| d.as_array().map(Vec::len))
            .unwrap_or(0);
        Status::Running {
            devices,
            enabled: lighting.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            brightness: lighting
                .get("brightness")
                .and_then(|v| v.as_u64())
                .map_or(100, |b| b.min(100) as u8),
        }
    }

    fn dashboard_url(&self) -> Option<String> {
        self.port().map(lightloom_api::native_url)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let body = serde_json::json!({ "enabled": enabled }).to_string();
        self.call("PATCH", "/v1/lighting", Some(&body)).map(drop)
    }

    fn set_brightness(&self, brightness: u8) -> Result<(), String> {
        let body = serde_json::json!({ "brightness": brightness }).to_string();
        self.call("PATCH", "/v1/lighting", Some(&body)).map(drop)
    }

    fn shutdown(&self) -> Result<(), String> {
        self.call("POST", "/v1/shutdown", None).map(drop)
    }

    fn profiles(&self) -> (Vec<String>, Option<String>) {
        let names = self
            .json("/v1/profiles")
            .and_then(|v| {
                v.as_array().map(|a| {
                    a.iter()
                        .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(String::from))
                        .collect()
                })
            })
            .unwrap_or_default();
        let current = self
            .json("/v1/lighting")
            .and_then(|l| l.get("profile").and_then(|p| p.as_str()).map(String::from));
        (names, current)
    }

    fn apply_profile(&self, name: &str) -> Result<(), String> {
        let path = format!("/v1/profiles/{}/apply", path_segment(name));
        self.call("POST", &path, None).map(drop)
    }
}

/// The request text: a loopback `Host` and, for changes, the bearer token
/// (what the API's guard asks for); no `Origin`.
pub fn request_text(port: u16, method: &str, path: &str, token: Option<&str>, body: Option<&str>) -> String {
    let mut r = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    if let Some(t) = token {
        r.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    let body = body.unwrap_or("");
    if !body.is_empty() {
        r.push_str("Content-Type: application/json\r\n");
    }
    r.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    r
}

/// One request to `127.0.0.1:port`: the status code and the body.
pub fn http(
    port: u16,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> io::Result<(u16, Vec<u8>)> {
    let mut s = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(500),
    )?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    s.write_all(request_text(port, method, path, token, body).as_bytes())?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw)?;
    parse_response(&raw).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not an HTTP response"))
}

/// Status code and body of a whole HTTP/1.1 response (`Content-Length`,
/// chunked, or up to the end).
pub fn parse_response(raw: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..split]).ok()?;
    let rest = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let code = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let (mut len, mut chunked) = (None, false);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let v = v.trim();
        if k.eq_ignore_ascii_case("content-length") {
            len = v.parse::<usize>().ok();
        } else if k.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked") {
            chunked = true;
        }
    }
    let body = if chunked {
        dechunk(rest)?
    } else if let Some(n) = len {
        rest.get(..n)?.to_vec()
    } else {
        rest.to_vec()
    };
    Some((code, body))
}

fn dechunk(mut b: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = b.windows(2).position(|w| w == b"\r\n")?;
        let size_text = std::str::from_utf8(&b[..eol]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        b = &b[eol + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(b.get(..size)?);
        b = b.get(size + 2..)?;
    }
}

/// The image of the icon in `ico` best for `want` pixels: the smallest at
/// least that big, else the largest. The bytes are one PNG or DIB image, as
/// `CreateIconFromResourceEx` takes it.
pub fn ico_image(ico: &[u8], want: u32) -> Option<&[u8]> {
    let u16_at = |o: usize| Some(u16::from_le_bytes(ico.get(o..o + 2)?.try_into().ok()?));
    let u32_at = |o: usize| Some(u32::from_le_bytes(ico.get(o..o + 4)?.try_into().ok()?));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let mut images = Vec::new();
    for i in 0..usize::from(u16_at(4)?) {
        let e = 6 + 16 * i;
        let size = match *ico.get(e)? {
            0 => 256,
            s => u32::from(s),
        };
        let (len, off) = (u32_at(e + 8)? as usize, u32_at(e + 12)? as usize);
        images.push((size, ico.get(off..off.checked_add(len)?)?));
    }
    let fits = images.iter().filter(|(s, _)| *s >= want).min_by_key(|(s, _)| *s);
    fits.or_else(|| images.iter().max_by_key(|(s, _)| *s))
        .map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeApi {
        status: RefCell<Option<Status>>,
        calls: RefCell<Vec<String>>,
        fail: bool,
    }

    impl FakeApi {
        fn running(enabled: bool, brightness: u8) -> Self {
            let a = Self::default();
            *a.status.borrow_mut() = Some(Status::Running {
                devices: 3,
                enabled,
                brightness,
            });
            a
        }
        fn log(&self, s: String) -> Result<(), String> {
            self.calls.borrow_mut().push(s);
            if self.fail {
                Err("refused".into())
            } else {
                Ok(())
            }
        }
    }

    impl Api for FakeApi {
        fn status(&self) -> Status {
            self.status.borrow().unwrap_or(Status::NotRunning)
        }
        fn dashboard_url(&self) -> Option<String> {
            self.status.borrow().map(|_| "http://127.0.0.1:53001/".into())
        }
        fn set_enabled(&self, e: bool) -> Result<(), String> {
            self.log(format!("enabled={e}"))
        }
        fn set_brightness(&self, b: u8) -> Result<(), String> {
            self.log(format!("brightness={b}"))
        }
        fn shutdown(&self) -> Result<(), String> {
            self.log("shutdown".into())
        }
    }

    #[derive(Default)]
    struct FakeDesktop {
        opened: RefCell<Vec<String>>,
        started: RefCell<u32>,
        can_start: bool,
    }

    impl Desktop for FakeDesktop {
        fn open_url(&self, url: &str) {
            self.opened.borrow_mut().push(url.into());
        }
        fn start_and_open(&self) -> bool {
            *self.started.borrow_mut() += 1;
            self.can_start
        }
    }

    fn autostart(cmd: Option<&str>) -> Autostart<MemRunKey> {
        Autostart::new(MemRunKey::default(), cmd.map(String::from))
    }

    #[test]
    fn open_uses_the_current_port_or_starts_the_daemon() {
        let (a, d) = (FakeApi::running(true, 100), FakeDesktop::default());
        assert_eq!(
            perform(Action::OpenDashboard, &a, &autostart(None), &d),
            Ok(Outcome::Stay)
        );
        assert_eq!(*d.opened.borrow(), ["http://127.0.0.1:53001/"]);
        assert_eq!(*d.started.borrow(), 0);

        let a = FakeApi::default();
        let d = FakeDesktop {
            can_start: true,
            ..Default::default()
        };
        assert_eq!(
            perform(Action::OpenDashboard, &a, &autostart(None), &d),
            Ok(Outcome::Stay)
        );
        assert!(d.opened.borrow().is_empty());
        assert_eq!(*d.started.borrow(), 1);

        let d = FakeDesktop::default();
        assert!(perform(Action::OpenDashboard, &a, &autostart(None), &d).is_err());
    }

    #[test]
    fn pause_and_resume_flip_enabled() {
        let a = FakeApi::running(true, 100);
        perform(Action::TogglePause, &a, &autostart(None), &FakeDesktop::default()).unwrap();
        let a2 = FakeApi::running(false, 100);
        perform(
            Action::TogglePause,
            &a2,
            &autostart(None),
            &FakeDesktop::default(),
        )
        .unwrap();
        assert_eq!(*a.calls.borrow(), ["enabled=false"]);
        assert_eq!(*a2.calls.borrow(), ["enabled=true"]);
        let off = FakeApi::default();
        assert!(perform(
            Action::TogglePause,
            &off,
            &autostart(None),
            &FakeDesktop::default()
        )
        .is_err());
        assert!(off.calls.borrow().is_empty());
    }

    #[test]
    fn brightness_presets_round_trip_through_menu_ids() {
        let a = FakeApi::running(true, 100);
        for b in BRIGHTNESS_PRESETS {
            let action = Action::from_id(Action::Brightness(b).id()).unwrap();
            perform(action, &a, &autostart(None), &FakeDesktop::default()).unwrap();
        }
        assert_eq!(
            *a.calls.borrow(),
            [
                "brightness=25",
                "brightness=50",
                "brightness=75",
                "brightness=100"
            ]
        );
        for act in [
            Action::OpenDashboard,
            Action::TogglePause,
            Action::ToggleAutostart,
            Action::Quit,
        ] {
            assert_eq!(Action::from_id(act.id()), Some(act));
        }
        assert_eq!(Action::from_id(0), None);
        assert_eq!(Action::from_id(99), None);
    }

    #[test]
    fn autostart_writes_and_removes_the_installer_value() {
        let cmd = Autostart::<MemRunKey>::command_for(Path::new(r"C:\Apps\Lightloom\lightloomw.exe"));
        assert_eq!(cmd, r#""C:\Apps\Lightloom\lightloomw.exe""#);
        let s = autostart(Some(&cmd));
        assert!(!s.enabled() && s.available());
        let (a, d) = (FakeApi::default(), FakeDesktop::default());
        perform(Action::ToggleAutostart, &a, &s, &d).unwrap();
        assert!(s.enabled());
        assert_eq!(s.key.get("Lightloom").unwrap().as_deref(), Some(cmd.as_str()));
        perform(Action::ToggleAutostart, &a, &s, &d).unwrap();
        assert!(!s.enabled());
        assert_eq!(s.key.get("Lightloom").unwrap(), None);
    }

    #[test]
    fn the_launcher_flag_sets_the_same_value_as_the_menu() {
        let key = MemRunKey::default();
        let exe = Path::new(r"C:\Apps\Lightloom\lightloomw.exe");
        set_autostart(&key, true, exe).unwrap();
        assert_eq!(
            key.get(RUN_VALUE).unwrap().as_deref(),
            Some(r#""C:\Apps\Lightloom\lightloomw.exe""#)
        );
        assert!(Autostart::new(&key, None).enabled());
        set_autostart(&key, false, exe).unwrap();
        set_autostart(&key, false, exe).unwrap(); // already off: fine
        assert_eq!(key.get(RUN_VALUE).unwrap(), None);
    }

    #[test]
    fn autostart_without_the_launcher_can_only_be_turned_off() {
        let s = autostart(None);
        assert!(!s.available());
        assert!(perform(
            Action::ToggleAutostart,
            &FakeApi::default(),
            &s,
            &FakeDesktop::default()
        )
        .is_err());
        // The installer's value (another path) counts as on, and turning it off works.
        s.key
            .set("Lightloom", r#""D:\Lightloom\lightloomw.exe""#)
            .unwrap();
        assert!(s.enabled() && s.available());
        assert!(!s.toggle().unwrap());
        assert_eq!(s.key.get("Lightloom").unwrap(), None);
    }

    #[test]
    fn quit_shuts_the_daemon_down_first() {
        let a = FakeApi::running(true, 50);
        assert_eq!(
            perform(Action::Quit, &a, &autostart(None), &FakeDesktop::default()),
            Ok(Outcome::Exit)
        );
        assert_eq!(*a.calls.borrow(), ["shutdown"]);
        // A refused shutdown keeps the tray, so the user can retry.
        let a = FakeApi {
            fail: true,
            ..FakeApi::running(true, 50)
        };
        assert!(perform(Action::Quit, &a, &autostart(None), &FakeDesktop::default()).is_err());
        // Nothing running: the tray just goes.
        let a = FakeApi::default();
        assert_eq!(
            perform(Action::Quit, &a, &autostart(None), &FakeDesktop::default()),
            Ok(Outcome::Exit)
        );
        assert!(a.calls.borrow().is_empty());
    }

    fn find(m: &[Entry], a: Action) -> Option<&Entry> {
        m.iter().find_map(|e| match e {
            Entry::Item { action, .. } if *action == a => Some(e),
            Entry::Submenu { items, .. } => find(items, a),
            _ => None,
        })
    }

    #[test]
    fn the_menu_follows_the_daemon() {
        let m = menu(
            Status::Running {
                devices: 2,
                enabled: false,
                brightness: 75,
            },
            (true, true),
        );
        let Some(Entry::Item { label, enabled, .. }) = find(&m, Action::TogglePause) else {
            panic!()
        };
        assert_eq!((label.as_str(), *enabled), ("Resume lighting", true));
        let Some(Entry::Item { checked, .. }) = find(&m, Action::Brightness(75)) else {
            panic!()
        };
        assert!(*checked);
        let Some(Entry::Item { checked, .. }) = find(&m, Action::Brightness(50)) else {
            panic!()
        };
        assert!(!*checked);
        let Some(Entry::Item { checked, .. }) = find(&m, Action::ToggleAutostart) else {
            panic!()
        };
        assert!(*checked);

        let m = menu(Status::NotRunning, (false, false));
        let Some(Entry::Item { label, default, .. }) = find(&m, Action::OpenDashboard) else {
            panic!()
        };
        assert_eq!((label.as_str(), *default), ("Start Lightloom", true));
        let Some(Entry::Item { enabled, .. }) = find(&m, Action::TogglePause) else {
            panic!()
        };
        assert!(!*enabled);
        let Some(Entry::Item { enabled, .. }) = find(&m, Action::ToggleAutostart) else {
            panic!()
        };
        assert!(!*enabled);
    }

    #[test]
    fn tooltips() {
        assert_eq!(tooltip(Status::NotRunning), "Lightloom \u{2014} not running");
        let s = |devices, enabled| Status::Running {
            devices,
            enabled,
            brightness: 100,
        };
        assert_eq!(tooltip(s(1, true)), "Lightloom \u{2014} 1 device");
        assert_eq!(tooltip(s(4, true)), "Lightloom \u{2014} 4 devices");
        assert_eq!(tooltip(s(0, false)), "Lightloom \u{2014} paused, 0 devices");
    }

    #[test]
    fn requests_pass_the_api_guard() {
        let r = request_text(
            53001,
            "PATCH",
            "/v1/lighting",
            Some("tok"),
            Some(r#"{"enabled":false}"#),
        );
        assert!(r.starts_with("PATCH /v1/lighting HTTP/1.1\r\nHost: 127.0.0.1:53001\r\n"));
        assert!(r.contains("\r\nAuthorization: Bearer tok\r\n"));
        assert!(r.contains("\r\nContent-Type: application/json\r\n"));
        assert!(r.ends_with("Content-Length: 17\r\n\r\n{\"enabled\":false}"));
        assert!(!r.contains("Origin"));
        let r = request_text(1, "GET", "/v1/devices", None, None);
        assert!(!r.contains("Authorization") && r.ends_with("Content-Length: 0\r\n\r\n"));
    }

    #[test]
    fn responses_parse() {
        let r = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n[]extra";
        assert_eq!(parse_response(r), Some((200, b"[]".to_vec())));
        let r = b"HTTP/1.1 401 Unauthorized\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(parse_response(r), Some((401, b"{\"a\":1}".to_vec())));
        assert_eq!(parse_response(b"garbage"), None);
    }

    /// A one-shot server answering each request with the next canned reply,
    /// recording what it got.
    fn serve(replies: Vec<String>) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut got = Vec::new();
            for reply in replies {
                let (mut s, _) = l.accept().unwrap();
                let mut req = Vec::new();
                let mut buf = [0u8; 4096];
                while !complete(&req) {
                    let n = s.read(&mut buf).unwrap();
                    assert!(n > 0, "the client hung up mid-request");
                    req.extend_from_slice(&buf[..n]);
                }
                got.push(String::from_utf8_lossy(&req).into_owned());
                s.write_all(reply.as_bytes()).unwrap();
            }
            got
        });
        (port, h)
    }

    /// Whether `req` holds the whole request (headers and `Content-Length` bytes).
    fn complete(req: &[u8]) -> bool {
        let Some(end) = req.windows(4).position(|w| w == b"\r\n\r\n") else {
            return false;
        };
        let head = String::from_utf8_lossy(&req[..end]).to_ascii_lowercase();
        let len = head
            .split("\r\n")
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        req.len() >= end + 4 + len
    }

    fn reply(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    fn home(name: &str, port: u16) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lightloom-tray-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        lightloom_runtime::dirs::write_api_port(&dir, port).unwrap();
        std::fs::write(dir.join("api-token"), "secret\n").unwrap();
        dir
    }

    #[test]
    fn http_api_reads_status_and_sends_the_token() {
        let lighting = reply("200 OK", r#"{"enabled":false,"brightness":40}"#);
        let devices = reply("200 OK", r#"[{"k":1},{"k":2}]"#);
        let ok = reply("200 OK", "{}");
        let (port, h) = serve(vec![lighting, devices, ok.clone(), ok]);
        let dir = home("status", port);
        let api = HttpApi::new(dir.clone());
        assert_eq!(
            api.status(),
            Status::Running {
                devices: 2,
                enabled: false,
                brightness: 40
            }
        );
        assert_eq!(api.dashboard_url(), Some(format!("http://127.0.0.1:{port}/")));
        api.set_brightness(75).unwrap();
        api.shutdown().unwrap();
        let got = h.join().unwrap();
        assert!(got[0].starts_with("GET /v1/lighting ") && !got[0].contains("Authorization"));
        assert!(got[1].starts_with("GET /v1/devices "));
        assert!(got[2].starts_with("PATCH /v1/lighting ") && got[2].contains("Bearer secret\r\n"));
        assert!(got[2].ends_with("{\"brightness\":75}"));
        assert!(got[3].starts_with("POST /v1/shutdown ") && got[3].contains("Bearer secret\r\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_api_reports_errors_and_absence() {
        let refused = reply("401 Unauthorized", r#"{"code":"unauthorized","message":"no!"}"#);
        let (port, h) = serve(vec![refused]);
        let dir = home("errors", port);
        let api = HttpApi::new(dir.clone());
        assert_eq!(api.set_enabled(true), Err("no!".into()));
        h.join().unwrap();
        // No api-port file: nothing runs.
        let _ = std::fs::remove_dir_all(&dir);
        let api = HttpApi::new(dir);
        assert_eq!(api.status(), Status::NotRunning);
        assert_eq!(api.dashboard_url(), None);
        assert!(api.shutdown().is_err());
    }

    #[test]
    fn the_icon_has_a_small_image() {
        let ico = include_bytes!("../../../packaging/wix/lightloom.ico");
        let img16 = ico_image(ico, 16).unwrap();
        assert!(img16.starts_with(b"\x89PNG"));
        let img20 = ico_image(ico, 20).unwrap();
        assert_ne!(img16, img20, "20 px picks the 24 px image");
        assert!(ico_image(ico, 1000).is_some(), "too big: the largest");
        assert_eq!(ico_image(b"not an icon", 16), None);
    }

    struct ProfileApi {
        applied: RefCell<Vec<String>>,
    }

    impl Api for ProfileApi {
        fn status(&self) -> Status {
            Status::Running {
                devices: 1,
                enabled: true,
                brightness: 100,
            }
        }
        fn dashboard_url(&self) -> Option<String> {
            None
        }
        fn set_enabled(&self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn set_brightness(&self, _: u8) -> Result<(), String> {
            Ok(())
        }
        fn shutdown(&self) -> Result<(), String> {
            Ok(())
        }
        fn profiles(&self) -> (Vec<String>, Option<String>) {
            (vec!["Day".into(), "R&D".into()], Some("R&D".into()))
        }
        fn apply_profile(&self, name: &str) -> Result<(), String> {
            self.applied.borrow_mut().push(name.into());
            Ok(())
        }
    }

    #[test]
    fn profiles_are_a_quick_switch() {
        let api = ProfileApi {
            applied: RefCell::new(Vec::new()),
        };
        let (names, current) = api.profiles();
        let m = menu_with_profiles(api.status(), (false, true), &names, current.as_deref());
        let Some(Entry::Submenu { items, .. }) = m
            .iter()
            .find(|e| matches!(e, Entry::Submenu { label, .. } if label == "Profiles"))
        else {
            panic!("no Profiles submenu: {m:?}");
        };
        assert_eq!(
            items[1],
            Entry::Item {
                action: Action::Profile(1),
                label: "R&&D".into(),
                checked: true,
                enabled: true,
                default: false,
            }
        );
        assert_eq!(Action::from_id(Action::Profile(1).id()), Some(Action::Profile(1)));
        perform(
            Action::Profile(1),
            &api,
            &autostart(None),
            &FakeDesktop::default(),
        )
        .unwrap();
        assert_eq!(*api.applied.borrow(), ["R&D"]);
        assert!(perform(
            Action::Profile(7),
            &api,
            &autostart(None),
            &FakeDesktop::default()
        )
        .is_err());
        // No profiles, or no daemon: no submenu.
        assert_eq!(
            menu_with_profiles(Status::NotRunning, (false, true), &names, None),
            menu(Status::NotRunning, (false, true))
        );
        assert_eq!(path_segment("R&D night/1"), "R%26D%20night%2F1");
    }
}
