// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Local API surface of `lightloomd`.
//!
//! Stub for M0: ports and the route table as constants. The axum server,
//! token auth, WebSocket event stream and the compat listeners (which refuse
//! to bind while SignalRGB owns 16038/16034) arrive with M4.

#![forbid(unsafe_code)]

/// Native REST + WS (dashboard, CLI).
pub const NATIVE_PORT: u16 = 16138;
/// SignalRGB-compatible REST `/api/v1/*` (shape-copied, Pro gates removed).
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
    /// Whether the call changes state (needs the token by default on compat).
    pub mutating: bool,
    /// Milestone that implements it.
    pub milestone: &'static str,
}

const fn r(method: &'static str, path: &'static str, mutating: bool, milestone: &'static str) -> Route {
    Route {
        method,
        path,
        mutating,
        milestone,
    }
}

/// SignalRGB-compatible routes on [`COMPAT_REST_PORT`] (relative to [`COMPAT_PREFIX`]).
pub const COMPAT_ROUTES: &[Route] = &[
    r("GET", "/app", false, "M4"),
    r("GET", "/lighting", false, "M4"),
    r("PATCH", "/lighting/global_brightness", true, "M4"),
    r("PATCH", "/lighting/enabled", true, "M4"),
    r("GET", "/lighting/effects", false, "M4"),
    r("GET", "/lighting/effects/{id}", false, "M4"),
    r("GET", "/lighting/effects/{id}/image", false, "M4"),
    r("POST", "/lighting/effects/{id}/apply", true, "M4"),
    r("GET", "/lighting/effects/{id}/presets", false, "M4"),
    r("PATCH", "/lighting/effects/{id}/presets", true, "M4"),
    r("GET", "/lighting/next", true, "M4"),
    r("POST", "/lighting/next", true, "M4"),
    r("GET", "/lighting/previous", true, "M4"),
    r("POST", "/lighting/previous", true, "M4"),
    r("POST", "/lighting/shuffle", true, "M4"),
    r("POST", "/lighting/loadEffect", true, "M4"),
    r("GET", "/lighting/playlists", false, "M4"),
    r("GET", "/lighting/playlists/{id}", false, "M4"),
    r("POST", "/lighting/playlists/{id}/play", true, "M4"),
    r("GET", "/scenes/current_layout", false, "M4"),
    r("PATCH", "/scenes/current_layout", true, "M4"),
    r("GET", "/scenes/layouts", false, "M4"),
];

/// Compat canvas-event route on [`COMPAT_CANVAS_EVENT_PORT`].
pub const COMPAT_CANVAS_EVENT_ROUTE: Route = r("POST", "/canvas/event", true, "M3");

/// Native routes on [`NATIVE_PORT`] (relative to [`NATIVE_PREFIX`]).
pub const NATIVE_ROUTES: &[Route] = &[
    r("GET", "/health", false, "M1"),
    r("GET", "/devices", false, "M1"),
    r("GET", "/devices/{uid}", false, "M2"),
    r("PATCH", "/devices/{uid}", true, "M2"),
    r("POST", "/devices/{uid}/identify", true, "M4"),
    r("GET", "/layouts", false, "M4"),
    r("GET", "/layouts/{name}", false, "M4"),
    r("PUT", "/layouts/{name}", true, "M4"),
    r("POST", "/layouts/{name}/apply", true, "M4"),
    r("GET", "/effects", false, "M3"),
    r("GET", "/effects/{id}", false, "M3"),
    r("POST", "/effects/{id}/apply", true, "M3"),
    r("PATCH", "/effects/{id}/properties", true, "M3"),
    r("GET", "/channels/{uid}", false, "M4"),
    r("PUT", "/channels/{uid}/{name}", true, "M4"),
    r("GET", "/components", false, "M4"),
    r("GET", "/sources", false, "M4"),
    r("POST", "/sources/{id}/update", true, "M4"),
    r("GET", "/logs", false, "M4"),
    r("GET", "/events", false, "M4"), // WebSocket upgrade
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_duplicate_routes() {
        for table in [COMPAT_ROUTES, NATIVE_ROUTES] {
            let mut seen = HashSet::new();
            for route in table {
                assert!(
                    seen.insert((route.method, route.path)),
                    "duplicate {} {}",
                    route.method,
                    route.path
                );
                assert!(route.path.starts_with('/'));
            }
        }
    }

    #[test]
    fn compat_mutations_use_non_get_except_next_previous() {
        for route in COMPAT_ROUTES {
            if route.method == "GET" && route.mutating {
                assert!(
                    route.path.ends_with("/next") || route.path.ends_with("/previous"),
                    "{}",
                    route.path
                );
            }
        }
    }
}
