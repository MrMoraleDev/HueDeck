// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux screen capture (docs/screen.md "Capture (Linux)").
//!
//! * **Wayland** (feature `screencast`, `portal.rs`): the xdg-desktop-portal
//!   ScreenCast interface. The compositor asks the user once which monitor to
//!   share; the grant is kept with a restore token, so later starts are
//!   silent. Frames arrive over a PipeWire stream.
//! * **X11** (feature `x11`, `x11.rs`): RandR monitors and MIT-SHM grabs of
//!   the root window, no permission needed.
//! * The backend follows the session: Wayland when `WAYLAND_DISPLAY` is set
//!   or `XDG_SESSION_TYPE` is `wayland`, else X11 when `DISPLAY` is set, else
//!   the portal. `LIGHTLOOM_SCREEN_BACKEND=portal|x11` overrides it.
//! * Both hand BGRA frames to the same reduction as Windows (`sample.rs`,
//!   `meters.rs`). Frames stay in memory in this process; nothing is stored.

mod drm;
pub(crate) mod pixels;
#[cfg(feature = "screencast")]
mod portal;
mod token;
#[cfg(feature = "x11")]
mod x11;

use super::{DisplayInfo, OpenSource};
use std::path::PathBuf;

/// Which capture path a session uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Portal,
    X11,
}

/// The backend for a session described by `env`.
fn backend(env: impl Fn(&str) -> Option<String>) -> Backend {
    match env("LIGHTLOOM_SCREEN_BACKEND").as_deref().map(str::trim) {
        Some("portal" | "wayland") => return Backend::Portal,
        Some("x11") => return Backend::X11,
        _ => {}
    }
    if env("WAYLAND_DISPLAY").is_some() || env("XDG_SESSION_TYPE").as_deref() == Some("wayland") {
        Backend::Portal
    } else if env("DISPLAY").is_some() {
        Backend::X11
    } else {
        Backend::Portal
    }
}

fn session() -> Backend {
    backend(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
}

/// The displays: RandR monitors on X11; the connected DRM connectors
/// otherwise (on Wayland the portal dialog picks the monitor).
pub fn displays() -> Vec<DisplayInfo> {
    #[cfg(feature = "x11")]
    if session() == Backend::X11 {
        match x11::displays() {
            Ok(d) if !d.is_empty() => return d,
            Ok(_) => {}
            Err(e) => tracing::debug!("screen: RandR: {e}"),
        }
    }
    drm::displays(std::path::Path::new("/sys/class/drm"))
}

/// An [`OpenSource`] for `display` on this session's backend. On Wayland the
/// portal's restore token is kept in `token_file` (`None`: not kept, the
/// dialog shows on every start).
pub fn desktop(display: Option<String>, token_file: Option<PathBuf>) -> OpenSource {
    match session() {
        Backend::X11 => x11_source(display),
        Backend::Portal => portal_source(display, token_file),
    }
}

#[cfg(feature = "x11")]
fn x11_source(display: Option<String>) -> OpenSource {
    Box::new(move || x11::Grab::open(display.as_deref()).map(|g| Box::new(g) as Box<dyn super::FrameSource>))
}

#[cfg(not(feature = "x11"))]
fn x11_source(_: Option<String>) -> OpenSource {
    Box::new(|| Err("X11 capture is not in this build (feature `x11`)".into()))
}

#[cfg(feature = "screencast")]
fn portal_source(display: Option<String>, token_file: Option<PathBuf>) -> OpenSource {
    let mut portal = portal::Portal::new(token::TokenStore::new(token_file, display.as_deref()));
    Box::new(move || portal.open())
}

#[cfg(not(feature = "screencast"))]
fn portal_source(_: Option<String>, token_file: Option<PathBuf>) -> OpenSource {
    let _ = token::TokenStore::new(token_file, None);
    Box::new(|| Err("Wayland capture is not in this build (feature `screencast`)".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn the_backend_follows_the_session() {
        assert_eq!(
            backend(env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")])),
            Backend::Portal
        );
        assert_eq!(backend(env(&[("XDG_SESSION_TYPE", "wayland")])), Backend::Portal);
        assert_eq!(
            backend(env(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")])),
            Backend::X11
        );
        assert_eq!(backend(env(&[])), Backend::Portal);
        assert_eq!(
            backend(env(&[
                ("WAYLAND_DISPLAY", "wayland-0"),
                ("LIGHTLOOM_SCREEN_BACKEND", "x11")
            ])),
            Backend::X11
        );
        assert_eq!(
            backend(env(&[("DISPLAY", ":0"), ("LIGHTLOOM_SCREEN_BACKEND", "portal")])),
            Backend::Portal
        );
    }
}
