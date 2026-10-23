// SPDX-License-Identifier: GPL-3.0-or-later
//! Local API of `huedeckd`: the native REST + WebSocket API and the dashboard
//! on [`NATIVE_PORT`], the SignalRGB-compatible REST API on
//! [`COMPAT_REST_PORT`] and its canvas-event listener on
//! [`COMPAT_CANVAS_EVENT_PORT`]. Loopback only; see [`server`] for the
//! security model.
//!
//! [`model`] holds what the daemon and clients exchange; [`server`] the axum
//! routers over a [`server::Backend`] (implemented by the daemon). The route
//! tables below document the surface and are checked against the routers by
//! the tests.

#![forbid(unsafe_code)]

pub mod model;
pub mod server;

pub use model::{ApiError, Event, Request, Snapshot};
pub use server::{canvas_event_router, compat_router, native_router, ApiContext, Backend};

/// Native REST + WS (dashboard, CLI).
pub const NATIVE_PORT: u16 = 16138;
/// SignalRGB-compatible REST `/api/v1/*`.
pub const COMPAT_REST_PORT: u16 = 16038;
/// SignalRGB-compatible `POST /canvas/event?sender&event`.
pub const COMPAT_CANVAS_EVENT_PORT: u16 = 16034;
/// OpenRGB SDK server (M6).
pub const OPENRGB_SDK_PORT: u16 = 6742;
/// Loopback only, always.
pub const BIND_ADDRESS: &str = "127.0.0.1";
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
    r("GET", "/lighting", false),
    r("PATCH", "/lighting/global_brightness", true),
    r("PATCH", "/lighting/enabled", true),
    r("GET", "/lighting/effects", false),
    r("GET", "/lighting/effects/{id}", false),
    r("POST", "/lighting/effects/{id}/apply", true),
    r("GET", "/lighting/effects/{id}/presets", false),
    r("PATCH", "/lighting/effects/{id}/presets", true),
    r("GET", "/lighting/next", false),
    r("POST", "/lighting/next", true),
    r("GET", "/lighting/previous", false),
    r("POST", "/lighting/previous", true),
    r("GET", "/lighting/playlists", false),
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
    r("GET", "/effects", false),
    r("POST", "/effects/next", true),
    r("POST", "/effects/previous", true),
    r("GET", "/effects/{id}", false),
    r("POST", "/effects/{id}/apply", true),
    r("PATCH", "/effects/{id}/values", true),
    r("PUT", "/effects/{id}/presets/{name}", true),
    r("DELETE", "/effects/{id}/presets/{name}", true),
    r("POST", "/effects/{id}/presets/{name}/apply", true),
    r("GET", "/lighting", false),
    r("PATCH", "/lighting", true),
    r("GET", "/layouts", false),
    r("GET", "/layouts/{name}", false),
    r("PUT", "/layouts/{name}", true),
    r("DELETE", "/layouts/{name}", true),
    r("POST", "/layouts/{name}/apply", true),
    r("PATCH", "/layout", true),
    r("PUT", "/placements/{uid}", true),
    r("GET", "/components", false),
    r("GET", "/settings", false),
    r("PATCH", "/settings", true),
    r("POST", "/import/signalrgb", true),
    r("GET", "/export", false),
    r("POST", "/import", true),
    r("GET", "/pawnio", false),
    r("POST", "/pawnio/install", true),
    r("POST", "/pawnio/remove", true),
    r("POST", "/rescan", true),
    r("POST", "/shutdown", true),
    r("POST", "/canvas/event", true),
    r("POST", "/canvas/tap", true),
    r("GET", "/events", false),  // WebSocket: JSON events
    r("GET", "/preview", false), // WebSocket: canvas frames
];

#[cfg(test)]
mod tests;
