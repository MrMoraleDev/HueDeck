// SPDX-License-Identifier: GPL-3.0-or-later
//! Local API of `lightloomd`: the native REST + WebSocket API and the dashboard
//! on [`NATIVE_PORT`] (changeable from the dashboard, see [`listener`]), the
//! SignalRGB-compatible REST API on
//! [`COMPAT_REST_PORT`] and its canvas-event listener on
//! [`COMPAT_CANVAS_EVENT_PORT`]. Loopback only; see [`server`] for the
//! security model.
//!
//! [`model`] holds what the daemon and clients exchange; [`server`] the axum
//! routers over a [`server::Backend`] (implemented by the daemon). The route
//! tables below document the surface and are checked against the routers by
//! the tests.

#![forbid(unsafe_code)]

pub mod actions;
pub mod listener;
pub mod logs;
pub mod model;
pub mod openrgb;
pub mod server;

pub use listener::NativeListener;
pub use model::{ApiError, Event, Request, Snapshot};
pub use server::{canvas_event_router, compat_router, native_router, ApiContext, Backend, Updates};

/// Native REST + WS (dashboard, CLI): the default port, the same on every OS.
/// The dashboard setting (Settings → Port) and `config.toml` `[api] port`
/// override it.
pub const NATIVE_PORT: u16 = 53000;
/// Lowest port the native API may move to (no privileged ports).
pub const MIN_NATIVE_PORT: u16 = 1024;
/// SignalRGB-compatible REST `/api/v1/*`.
pub const COMPAT_REST_PORT: u16 = 16038;
/// SignalRGB-compatible `POST /canvas/event?sender&event`.
pub const COMPAT_CANVAS_EVENT_PORT: u16 = 16034;
/// OpenRGB SDK server ([`openrgb`]; opt-in).
pub const OPENRGB_SDK_PORT: u16 = 6742;
/// Loopback only, always.
pub const BIND_ADDRESS: &str = "127.0.0.1";
/// Whether the native API may listen on `port`: `1024..=65535`, and not one
/// of the SignalRGB-compatible ports or the OpenRGB SDK port. The message
/// says why not.
pub fn check_native_port(port: u32) -> Result<u16, String> {
    let Ok(p) = u16::try_from(port) else {
        return Err(format!(
            "port {port} is out of range: choose {MIN_NATIVE_PORT}-65535"
        ));
    };
    if p < MIN_NATIVE_PORT {
        return Err(format!(
            "port {p} is out of range: choose {MIN_NATIVE_PORT}-65535 (lower ports need administrator rights)"
        ));
    }
    if [COMPAT_REST_PORT, COMPAT_CANVAS_EVENT_PORT, OPENRGB_SDK_PORT].contains(&p) {
        return Err(format!(
            "port {p} is reserved for Lightloom's SignalRGB-compatible or OpenRGB listeners: choose another"
        ));
    }
    Ok(p)
}

/// The dashboard's address on `port`.
pub fn native_url(port: u16) -> String {
    format!("http://{BIND_ADDRESS}:{port}/")
}

/// `api_version` in the compat envelope.
pub const COMPAT_API_VERSION: &str = "1.0";
/// Compat path prefix.
pub const COMPAT_PREFIX: &str = "/api/v1";
/// Native path prefix.
pub const NATIVE_PREFIX: &str = "/v1";

/// One route of a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    /// HTTP method.
    pub method: &'static str,
    /// Path (may contain `{id}` placeholders).
    pub path: &'static str,
    /// Whether the call changes state (needs the token).
    pub mutating: bool,
}

const fn r(method: &'static str, path: &'static str, mutating: bool) -> Route {
    Route {
        method,
        path,
        mutating,
    }
}

/// SignalRGB-compatible routes on [`COMPAT_REST_PORT`] (relative to [`COMPAT_PREFIX`]).
pub const COMPAT_ROUTES: &[Route] = &[
    r("GET", "/app", false),
    r("GET", "/new", false),
    r("GET", "/partners/{id}", false),
    r("GET", "/lighting", false),
    r("PATCH", "/lighting/global_brightness", true),
    r("PATCH", "/lighting/enabled", true),
    r("GET", "/lighting/effects", false),
    r("GET", "/lighting/effects/{id}", false),
    r("GET", "/lighting/effects/{id}/image", false),
    r("POST", "/lighting/effects/{id}/apply", true),
    r("GET", "/lighting/effects/{id}/presets", false),
    r("PATCH", "/lighting/effects/{id}/presets", true),
    r("GET", "/lighting/next", false),
    r("POST", "/lighting/next", true),
    r("GET", "/lighting/previous", false),
    r("POST", "/lighting/previous", true),
    r("GET", "/lighting/shuffle", false),
    r("POST", "/lighting/shuffle", true),
    r("POST", "/lighting/loadEffect", true),
    r("GET", "/lighting/playlists", false),
    r("GET", "/lighting/playlists/{id}", false),
    r("POST", "/lighting/playlists/{id}/play", true),
    r("GET", "/scenes/current_layout", false),
    r("PATCH", "/scenes/current_layout", true),
    r("GET", "/scenes/layouts", false),
];

/// Compat canvas-event route on [`COMPAT_CANVAS_EVENT_PORT`].
pub const COMPAT_CANVAS_EVENT_ROUTE: Route = r("POST", "/canvas/event", true);

/// Native routes on [`NATIVE_PORT`] (relative to [`NATIVE_PREFIX`]).
pub const NATIVE_ROUTES: &[Route] = &[
    r("GET", "/health", false),
    r("GET", "/state", false),
    r("GET", "/devices", false),
    r("GET", "/devices/{key}", false),
    r("PATCH", "/devices/{key}", true),
    r("PUT", "/devices/{key}/channels/{channel}", true),
    r("PUT", "/devices/{key}/lcd", true),
    r("PUT", "/devices/{key}/mode", true),
    r("GET", "/devices/{key}/leds", false),
    r("PATCH", "/devices/{key}/leds/{uid}", true),
    r("PUT", "/devices/{key}/leds/{uid}/zones/{name}", true),
    r("DELETE", "/devices/{key}/leds/{uid}/zones/{name}", true),
    r("GET", "/lcd/faces", false),
    r("POST", "/save/preview", true),
    r("POST", "/save", true),
    r("GET", "/effects", false),
    r("POST", "/effects/next", true),
    r("POST", "/effects/previous", true),
    r("POST", "/effects/shuffle", true),
    r("GET", "/effects/{id}", false),
    r("POST", "/effects/{id}/apply", true),
    r("PATCH", "/effects/{id}/values", true),
    r("PUT", "/effects/{id}/presets/{name}", true),
    r("DELETE", "/effects/{id}/presets/{name}", true),
    r("POST", "/effects/{id}/presets/{name}/apply", true),
    r("GET", "/lighting", false),
    r("PATCH", "/lighting", true),
    r("PATCH", "/lighting/timer", true),
    r("GET", "/playlists", false),
    r("POST", "/playlists", true),
    r("POST", "/playlists/stop", true),
    r("GET", "/playlists/{id}", false),
    r("PUT", "/playlists/{id}", true),
    r("DELETE", "/playlists/{id}", true),
    r("POST", "/playlists/{id}/play", true),
    r("GET", "/layouts", false),
    r("GET", "/layouts/{name}", false),
    r("PUT", "/layouts/{name}", true),
    r("DELETE", "/layouts/{name}", true),
    r("POST", "/layouts/{name}/apply", true),
    r("PATCH", "/layout", true),
    r("GET", "/profiles", false),
    r("POST", "/profiles", true),
    r("PATCH", "/profiles", true),
    r("GET", "/profiles/{name}", false),
    r("PUT", "/profiles/{name}", true),
    r("DELETE", "/profiles/{name}", true),
    r("POST", "/profiles/{name}/apply", true),
    r("POST", "/profiles/{name}/rename", true),
    r("POST", "/profiles/{name}/duplicate", true),
    r("PUT", "/placements/{uid}", true),
    r("GET", "/components", false),
    r("GET", "/settings", false),
    r("PATCH", "/settings", true),
    r("GET", "/import/signalrgb", false),
    r("POST", "/import/signalrgb", true),
    r("GET", "/export", false),
    r("POST", "/import", true),
    r("GET", "/pawnio", false),
    r("POST", "/pawnio/install", true),
    r("POST", "/pawnio/remove", true),
    r("POST", "/rescan", true),
    r("GET", "/sensors", false),
    r("GET", "/inputs", false),
    r("PATCH", "/inputs", true),
    r("GET", "/logs", false),
    r("GET", "/update", false),
    r("PATCH", "/update", true),
    r("POST", "/update/check", true),
    r("POST", "/update/install", true),
    r("GET", "/network", false),
    r("POST", "/network/{plugin}/add", true),
    r("POST", "/network/{plugin}/controllers/{id}/{action}", true),
    r("POST", "/shutdown", true),
    r("POST", "/canvas/event", true),
    r("POST", "/canvas/tap", true),
    r("POST", "/actions", true),
    r("GET", "/macros", false),
    r("PATCH", "/macros", true),
    r("PATCH", "/macros/scripts/{id}", true),
    r("POST", "/macros/scripts/{id}/run", true),
    r("POST", "/macros/prompts/{id}", true),
    r("GET", "/events", false),  // WebSocket: JSON events
    r("GET", "/preview", false), // WebSocket: canvas frames
];

#[cfg(test)]
mod tests;
