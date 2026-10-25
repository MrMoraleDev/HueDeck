// SPDX-License-Identifier: GPL-3.0-or-later
//! The action router's grammar: `lightloom://<endpoint>/<verb>[/<arg>][?k=v&...]`
//! URLs, the paths of SignalRGB's `signalrgb://` / `system.action()` router
//! (docs/host-api/macros-system.md §2) under Lightloom's own scheme.
//!
//! [`parse`] turns a URL (or a bare path, as `system.action()` takes) into an
//! [`Action`]. It is strict about what it knows: a malformed URL, a bad
//! percent escape, a duplicate query key, a missing or extra argument, or a
//! brightness that is not `0..=100` is an [`ActionError`] (the daemon answers
//! `400 invalid_action`). What it does not know (another endpoint, another
//! verb, a SignalRGB page Lightloom has no counterpart for) is
//! [`Action::Unknown`]: the daemon drops it with a warning, as SignalRGB
//! ignores paths it has no handler for. The daemon dispatches the rest to its
//! ordinary requests (docs/integrations/actions.md).

use serde_json::{json, Value};
use std::fmt;

/// Lightloom's URL scheme.
pub const SCHEME: &str = "lightloom";
/// Longest URL accepted, in bytes.
pub const MAX_URL_LEN: usize = 2048;

/// Query keys of SignalRGB's router that carry no meaning here (telemetry,
/// focus and toast bookkeeping); ignored wherever they appear.
pub const RESERVED_KEYS: &[&str] = &[
    "_location",
    "_silent",
    "_sessionId",
    "_timestamp",
    "sender",
    "version",
];

/// One parsed action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// `app/raiseWindow`: bring the dashboard to the front.
    RaiseWindow,
    /// `app/restart`: restart the engine in place (devices, plugins and effects
    /// reloaded; the process and the API stay up).
    Restart,
    /// `app/toast?title=&message=`: a system notification (and a dashboard banner).
    Toast {
        /// First line.
        title: String,
        /// The text under it.
        message: String,
    },
    /// `view/<page>[/<sub>][?uid=]`: show a dashboard tab.
    View {
        /// The page as written (`devices`, `settings/Video`, `showdevice`, ...).
        page: String,
        /// The dashboard tab it maps to (`effects`, `layout`, `devices`, `macros`, `settings`).
        tab: &'static str,
        /// A device to show (`showdevice?uid=`, `devices?uid=`).
        uid: Option<String>,
    },
    /// `layout/apply/<name>`.
    ApplyLayout {
        /// Layout name.
        name: String,
    },
    /// `layout/applyprevious`: the layout before the current one.
    ApplyPreviousLayout,
    /// `effect/apply/<name>?<property>=<value>&...`.
    ApplyEffect {
        /// Effect id or title.
        name: String,
        /// Control values as written (the daemon coerces them to the controls' types),
        /// in query order; [`RESERVED_KEYS`] and other `_` keys are left out.
        properties: Vec<(String, String)>,
    },
    /// `effect/install/<x>`, `effect/update/<x>`: refused (Lightloom has no marketplace).
    Refused {
        /// The path.
        path: String,
        /// Why.
        reason: String,
    },
    /// `device/<uid>/enable` / `device/<uid>/disable`.
    SetDevice {
        /// Device key.
        uid: String,
        /// Enable or disable.
        enabled: bool,
    },
    /// `playlist/<id>/play`.
    PlayPlaylist {
        /// Playlist id.
        id: String,
    },
    /// `playlist/<id>/stop`.
    StopPlaylist {
        /// Playlist id.
        id: String,
    },
    /// `brightness/<0-100>`: global brightness.
    Brightness(u8),
    /// `canvas/pause` (`false`) / `canvas/resume` (`true`).
    Canvas {
        /// Resume.
        enabled: bool,
    },
    /// A well-formed path the router has no handler for: dropped with a warning.
    Unknown {
        /// The path.
        path: String,
        /// What is not known.
        warning: String,
    },
}

impl Action {
    /// Short canonical name for logs and replies (`effect/apply`, `view`, ...).
    pub fn name(&self) -> &'static str {
        match self {
            Action::RaiseWindow => "app/raiseWindow",
            Action::Restart => "app/restart",
            Action::Toast { .. } => "app/toast",
            Action::View { .. } => "view",
            Action::ApplyLayout { .. } => "layout/apply",
            Action::ApplyPreviousLayout => "layout/applyprevious",
            Action::ApplyEffect { .. } => "effect/apply",
            Action::Refused { .. } => "refused",
            Action::SetDevice { enabled: true, .. } => "device/enable",
            Action::SetDevice { enabled: false, .. } => "device/disable",
            Action::PlayPlaylist { .. } => "playlist/play",
            Action::StopPlaylist { .. } => "playlist/stop",
            Action::Brightness(_) => "brightness",
            Action::Canvas { enabled: true } => "canvas/resume",
            Action::Canvas { enabled: false } => "canvas/pause",
            Action::Unknown { .. } => "unknown",
        }
    }

    /// The `action` event the dashboard follows (`raiseWindow`, `view`,
    /// `toast`); `None` for actions the daemon carries out itself.
    pub fn event_data(&self) -> Option<Value> {
        match self {
            Action::RaiseWindow => Some(json!({ "action": "raiseWindow" })),
            Action::Toast { title, message } => {
                Some(json!({ "action": "toast", "title": title, "message": message }))
            }
            Action::View { page, tab, uid } => {
                Some(json!({ "action": "view", "page": page, "tab": tab, "uid": uid }))
            }
            _ => None,
        }
    }
}

/// A URL the router refuses to parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionError(pub String);

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ActionError {}

fn err<T>(message: impl Into<String>) -> Result<T, ActionError> {
    Err(ActionError(message.into()))
}

/// SignalRGB's `view/` pages and the Lightloom tab each one shows; `None`: no
/// counterpart (the action is dropped). Lightloom's own tab names are accepted too.
fn view_tab(page: &str) -> Option<Option<&'static str>> {
    Some(match page.to_ascii_lowercase().as_str() {
        "dashboard" | "customize" | "effects" | "library" => Some("effects"),
        "layouts" | "layout" => Some("layout"),
        "devices" | "showdevice" | "network" => Some("devices"),
        "settings" | "addons" | "debuginfo" | "logs" => Some("settings"),
        "macros" => Some("macros"),
        "cooling" | "monitoring" | "dumps" => None,
        _ => return None,
    })
}

/// Percent-decode `s` (`+` is a space when `plus` is set); strict: a bad
/// escape, invalid UTF-8 or a control character is an error.
fn decode(s: &str, plus: bool) -> Result<String, ActionError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes
                    .get(i + 1..i + 3)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                let Some(b) = hex else {
                    return err(format!("bad percent escape in {s:?}"));
                };
                out.push(b);
                i += 3;
            }
            b'+' if plus => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    let text = String::from_utf8(out).map_err(|_| ActionError(format!("{s:?} is not UTF-8 once decoded")))?;
    if text.chars().any(char::is_control) {
        return err(format!("{s:?} holds a control character"));
    }
    Ok(text)
}

/// The path part of `url`: `lightloom://<path>`, `lightloom:<path>` or a bare
/// path. Other schemes are refused.
fn strip_scheme(url: &str) -> Result<&str, ActionError> {
    if let Some(colon) = url.find(':') {
        let (scheme, rest) = url.split_at(colon);
        // A scheme is letters, digits, `+`, `-`, `.` before the first `/` or `?`.
        let looks_like_scheme = !scheme.is_empty()
            && !scheme.contains(['/', '?'])
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if looks_like_scheme {
            if !scheme.eq_ignore_ascii_case(SCHEME) {
                return err(format!("only {SCHEME}:// URLs are actions (got {scheme}:)"));
            }
            let rest = &rest[1..];
            return Ok(rest.strip_prefix("//").unwrap_or(rest));
        }
    }
    Ok(url)
}

/// Parse an action URL.
pub fn parse(url: &str) -> Result<Action, ActionError> {
    let url = url.trim();
    if url.is_empty() {
        return err("empty action URL");
    }
    if url.len() > MAX_URL_LEN {
        return err(format!("action URL longer than {MAX_URL_LEN} bytes"));
    }
    if url.chars().any(|c| c.is_control() || c == ' ') {
        return err("the action URL holds spaces or control characters (percent-encode them)");
    }
    let rest = strip_scheme(url)?;
    // A fragment carries nothing here.
    let rest = rest.split('#').next().unwrap_or("");
    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (rest, None),
    };
    let path = path.strip_prefix('/').unwrap_or(path);
    // Browsers and shells often add one trailing slash.
    let path = path.strip_suffix('/').unwrap_or(path);
    if path.is_empty() {
        return err("the action URL has no path (e.g. lightloom://app/raiseWindow)");
    }
    let mut segments = Vec::new();
    for raw in path.split('/') {
        if raw.is_empty() {
            return err(format!("empty path segment in {path:?}"));
        }
        segments.push(decode(raw, false)?);
    }
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(q) = query {
        for pair in q.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let (k, v) = (decode(k, true)?, decode(v, true)?);
            if k.is_empty() {
                return err(format!("query parameter without a name in {q:?}"));
            }
            if params.iter().any(|(have, _)| *have == k) {
                return err(format!("query parameter {k:?} given twice"));
            }
            params.push((k, v));
        }
    }
    let plain = segments.join("/");
    route(&segments, &params, &plain)
}

fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn unknown(path: &str, warning: impl Into<String>) -> Result<Action, ActionError> {
    Ok(Action::Unknown {
        path: path.to_string(),
        warning: warning.into(),
    })
}

/// `name` must be one argument; anything after it is an error.
fn one_arg<'a>(endpoint: &str, args: &'a [String]) -> Result<&'a str, ActionError> {
    match args {
        [one] if !one.trim().is_empty() => Ok(one),
        [] => err(format!("{endpoint} needs a name")),
        [_] => err(format!("{endpoint} needs a non-blank name")),
        _ => err(format!(
            "{endpoint} takes one name (percent-encode a '/' in it as %2F)"
        )),
    }
}

fn no_args(endpoint: &str, args: &[String]) -> Result<(), ActionError> {
    if args.is_empty() {
        Ok(())
    } else {
        err(format!("{endpoint} takes no argument"))
    }
}

fn route(segments: &[String], params: &[(String, String)], path: &str) -> Result<Action, ActionError> {
    let endpoint = segments[0].to_ascii_lowercase();
    let verb = segments.get(1).map(|v| v.to_ascii_lowercase());
    let tail = segments.get(2..).unwrap_or(&[]);
    match (endpoint.as_str(), verb.as_deref()) {
        ("app", Some("raisewindow")) => no_args("app/raiseWindow", tail).map(|_| Action::RaiseWindow),
        ("app", Some("restart")) => no_args("app/restart", tail).map(|_| Action::Restart),
        ("app", Some("toast")) => {
            no_args("app/toast", tail)?;
            let title = param(params, "title").unwrap_or("").trim().to_string();
            let message = param(params, "message").unwrap_or("").trim().to_string();
            if title.is_empty() && message.is_empty() {
                return err("app/toast needs a title or a message (?title=...&message=...)");
            }
            Ok(Action::Toast {
                title: if title.is_empty() { "Lightloom".into() } else { title },
                message,
            })
        }
        ("app", _) => unknown(path, format!("no app action {path:?}")),
        ("view", None) => err("view needs a page (e.g. view/devices)"),
        ("view", Some(_)) => {
            let page_name = &segments[1];
            let page = segments[1..].join("/");
            // Only settings pages have sub-pages.
            if !tail.is_empty() && !page_name.eq_ignore_ascii_case("settings") {
                return err(format!("view/{page_name} has no sub-page"));
            }
            match view_tab(page_name) {
                None => unknown(path, format!("no page {page_name:?}")),
                Some(None) => unknown(path, format!("Lightloom has no {page_name} page")),
                Some(Some(tab)) => {
                    let uid = param(params, "uid").map(str::trim).filter(|u| !u.is_empty());
                    if page_name.eq_ignore_ascii_case("showdevice") && uid.is_none() {
                        return err("view/showdevice needs ?uid=<device key>");
                    }
                    Ok(Action::View {
                        page,
                        tab,
                        uid: uid.map(str::to_string),
                    })
                }
            }
        }
        ("layout", Some("apply")) => Ok(Action::ApplyLayout {
            name: one_arg("layout/apply", tail)?.to_string(),
        }),
        ("layout", Some("applyprevious")) => {
            no_args("layout/applyprevious", tail).map(|_| Action::ApplyPreviousLayout)
        }
        ("layout", _) => unknown(path, format!("no layout action {path:?}")),
        ("effect", Some("apply")) => {
            let name = one_arg("effect/apply", tail)?.to_string();
            let properties = params
                .iter()
                .filter(|(k, _)| !k.starts_with('_') && !RESERVED_KEYS.contains(&k.as_str()))
                .cloned()
                .collect();
            Ok(Action::ApplyEffect { name, properties })
        }
        ("effect", Some(v @ ("install" | "update"))) => Ok(Action::Refused {
            path: path.to_string(),
            reason: format!(
                "effect/{v} is refused: Lightloom has no effect marketplace (put effect files in the user effects folder)"
            ),
        }),
        ("effect", _) => unknown(path, format!("no effect action {path:?}")),
        ("device", Some(_)) => {
            let uid = segments[1].trim();
            match segments.get(2).map(|v| v.to_ascii_lowercase()).as_deref() {
                Some(v @ ("enable" | "disable")) => {
                    if segments.len() > 3 {
                        return err(format!("device/<uid>/{v} takes no further argument"));
                    }
                    if uid.is_empty() {
                        return err("device/<uid> needs a device key");
                    }
                    Ok(Action::SetDevice {
                        uid: uid.to_string(),
                        enabled: v == "enable",
                    })
                }
                None => err("device/<uid> needs a verb (enable or disable)"),
                Some(_) => unknown(path, format!("no device action {path:?}")),
            }
        }
        ("device", None) => err("device needs a device key and a verb (device/<uid>/enable)"),
        ("playlist", Some(_)) => {
            let id = segments[1].trim();
            match segments.get(2).map(|v| v.to_ascii_lowercase()).as_deref() {
                Some(v @ ("play" | "stop")) => {
                    if segments.len() > 3 {
                        return err(format!("playlist/<id>/{v} takes no further argument"));
                    }
                    if id.is_empty() {
                        return err("playlist/<id> needs a playlist id");
                    }
                    let id = id.to_string();
                    Ok(if v == "play" {
                        Action::PlayPlaylist { id }
                    } else {
                        Action::StopPlaylist { id }
                    })
                }
                None => err("playlist/<id> needs a verb (play or stop)"),
                Some(_) => unknown(path, format!("no playlist action {path:?}")),
            }
        }
        ("playlist", None) => err("playlist needs an id and a verb (playlist/<id>/play)"),
        ("brightness", Some(_)) => {
            if !tail.is_empty() {
                return err("brightness takes one number");
            }
            let raw = &segments[1];
            if raw.is_empty() || raw.len() > 3 || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return err(format!("brightness must be a whole number 0-100 (got {raw:?})"));
            }
            match raw.parse::<u8>() {
                Ok(b) if b <= 100 => Ok(Action::Brightness(b)),
                _ => err(format!("brightness must be 0-100 (got {raw})")),
            }
        }
        ("brightness", None) => err("brightness needs a value (brightness/<0-100>)"),
        ("canvas", Some("pause")) => no_args("canvas/pause", tail).map(|_| Action::Canvas { enabled: false }),
        ("canvas", Some("resume")) => no_args("canvas/resume", tail).map(|_| Action::Canvas { enabled: true }),
        ("canvas", _) => unknown(path, format!("no canvas action {path:?}")),
        _ => unknown(path, format!("no action endpoint {:?}", segments[0])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(url: &str) -> Action {
        parse(url).unwrap_or_else(|e| panic!("{url}: {e}"))
    }

    fn props(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn the_grammar_table() {
        let table: Vec<(&str, Action)> = vec![
            ("lightloom://app/raiseWindow", Action::RaiseWindow),
            ("LIGHTLOOM://APP/RAISEWINDOW/", Action::RaiseWindow),
            ("lightloom:app/raiseWindow", Action::RaiseWindow),
            ("app/raiseWindow", Action::RaiseWindow),
            ("/app/raiseWindow", Action::RaiseWindow),
            ("lightloom://app/restart", Action::Restart),
            (
                "lightloom://app/toast?_silent=true&sender=default&title=Hi&message=Hello%20there",
                Action::Toast {
                    title: "Hi".into(),
                    message: "Hello there".into(),
                },
            ),
            (
                "lightloom://app/toast?message=Only+text",
                Action::Toast {
                    title: "Lightloom".into(),
                    message: "Only text".into(),
                },
            ),
            (
                "lightloom://view/devices",
                Action::View {
                    page: "devices".into(),
                    tab: "devices",
                    uid: None,
                },
            ),
            (
                "lightloom://view/dashboard",
                Action::View {
                    page: "dashboard".into(),
                    tab: "effects",
                    uid: None,
                },
            ),
            (
                "lightloom://view/layouts",
                Action::View {
                    page: "layouts".into(),
                    tab: "layout",
                    uid: None,
                },
            ),
            (
                "lightloom://view/settings/Video",
                Action::View {
                    page: "settings/Video".into(),
                    tab: "settings",
                    uid: None,
                },
            ),
            (
                "lightloom://view/showdevice?uid=1234:4d4d:D",
                Action::View {
                    page: "showdevice".into(),
                    tab: "devices",
                    uid: Some("1234:4d4d:D".into()),
                },
            ),
            (
                "lightloom://view/devices?sender=LowLevelBattery&uid=K",
                Action::View {
                    page: "devices".into(),
                    tab: "devices",
                    uid: Some("K".into()),
                },
            ),
            (
                "lightloom://layout/apply/Desk%20Setup",
                Action::ApplyLayout {
                    name: "Desk Setup".into(),
                },
            ),
            (
                "lightloom://layout/apply/A%2FB",
                Action::ApplyLayout { name: "A/B".into() },
            ),
            ("lightloom://layout/applyprevious", Action::ApplyPreviousLayout),
            (
                "lightloom://effect/apply/Rainbow%20Rise?reverse=false&scale=1&speed=3&xPos=120&yPos=200",
                Action::ApplyEffect {
                    name: "Rainbow Rise".into(),
                    properties: props(&[
                        ("reverse", "false"),
                        ("scale", "1"),
                        ("speed", "3"),
                        ("xPos", "120"),
                        ("yPos", "200"),
                    ]),
                },
            ),
            (
                "lightloom://effect/apply/Side%20to%20Side?_location=LayoutTour",
                Action::ApplyEffect {
                    name: "Side to Side".into(),
                    properties: Vec::new(),
                },
            ),
            (
                "lightloom://effect/apply/x?color=%23ff0000",
                Action::ApplyEffect {
                    name: "x".into(),
                    properties: props(&[("color", "#ff0000")]),
                },
            ),
            (
                "lightloom://device/1234:4d4d:D/disable",
                Action::SetDevice {
                    uid: "1234:4d4d:D".into(),
                    enabled: false,
                },
            ),
            (
                "lightloom://device/K/Enable",
                Action::SetDevice {
                    uid: "K".into(),
                    enabled: true,
                },
            ),
            (
                "lightloom://playlist/evening/play",
                Action::PlayPlaylist { id: "evening".into() },
            ),
            (
                "lightloom://playlist/evening/stop",
                Action::StopPlaylist { id: "evening".into() },
            ),
            ("lightloom://brightness/0", Action::Brightness(0)),
            ("lightloom://brightness/55", Action::Brightness(55)),
            ("lightloom://brightness/100", Action::Brightness(100)),
            (
                "lightloom://view/macros",
                Action::View {
                    page: "macros".into(),
                    tab: "macros",
                    uid: None,
                },
            ),
            ("lightloom://canvas/pause", Action::Canvas { enabled: false }),
            ("lightloom://canvas/resume#x", Action::Canvas { enabled: true }),
        ];
        for (url, want) in table {
            assert_eq!(ok(url), want, "{url}");
        }
    }

    #[test]
    fn marketplace_paths_are_refused() {
        for url in [
            "lightloom://effect/install/Side%20to%20Side?_location=LayoutTour",
            "lightloom://effect/update/abc",
        ] {
            assert!(matches!(ok(url), Action::Refused { .. }), "{url}");
        }
    }

    #[test]
    fn unknown_paths_are_dropped_not_errors() {
        for url in [
            "lightloom://addon/install?repo=x",
            "lightloom://app/update?_silent=true&version=2",
            "lightloom://app/autominiplayer?enable=false",
            "lightloom://view/cooling",
            "lightloom://view/nowhere",
            "lightloom://layout/rename/x",
            "lightloom://effect/favourite/x",
            "lightloom://device/K/identify",
            "lightloom://playlist/p/shuffle",
            "lightloom://canvas/blink",
            "lightloom://whatever",
        ] {
            assert!(
                matches!(ok(url), Action::Unknown { .. }),
                "{url}: {:?}",
                parse(url)
            );
        }
    }

    #[test]
    fn malformed_urls_are_errors() {
        for url in [
            "",
            "   ",
            "lightloom://",
            "lightloom:///",
            "http://example.com/app/raiseWindow",
            "signalrgb://app/raiseWindow",
            "javascript:alert(1)",
            "lightloom://app//raiseWindow",
            "lightloom://app/raiseWindow/extra",
            "lightloom://app/restart/now",
            "lightloom://app/toast",
            "lightloom://app/toast?_silent=true&sender=default",
            "lightloom://view",
            "lightloom://view/devices/extra",
            "lightloom://view/showdevice",
            "lightloom://view/showdevice?uid=",
            "lightloom://layout/apply",
            "lightloom://layout/apply/%20",
            "lightloom://layout/apply/a/b",
            "lightloom://layout/applyprevious/x",
            "lightloom://effect/apply",
            "lightloom://effect/apply/x/y",
            "lightloom://effect/apply/x?speed=1&speed=2",
            "lightloom://effect/apply/x?=1",
            "lightloom://effect/apply/bad%zz",
            "lightloom://effect/apply/bad%",
            "lightloom://effect/apply/%FF%FE",
            "lightloom://effect/apply/a%0Ab",
            "lightloom://effect/apply/has space",
            "lightloom://device",
            "lightloom://device/K",
            "lightloom://device/K/enable/now",
            "lightloom://playlist",
            "lightloom://playlist/p",
            "lightloom://playlist/p/play/x",
            "lightloom://brightness",
            "lightloom://brightness/101",
            "lightloom://brightness/-1",
            "lightloom://brightness/1.5",
            "lightloom://brightness/0050",
            "lightloom://brightness/abc",
            "lightloom://brightness/5/6",
            "lightloom://canvas/pause/now",
        ] {
            assert!(parse(url).is_err(), "{url:?} parsed as {:?}", parse(url));
        }
        assert!(parse(&format!("lightloom://effect/apply/{}", "a".repeat(MAX_URL_LEN))).is_err());
    }

    #[test]
    fn view_actions_carry_their_event() {
        let v = ok("lightloom://view/showdevice?uid=K").event_data().unwrap();
        assert_eq!(v["action"], "view");
        assert_eq!(v["tab"], "devices");
        assert_eq!(v["uid"], "K");
        assert_eq!(
            ok("app/raiseWindow").event_data().unwrap()["action"],
            "raiseWindow"
        );
        assert!(ok("brightness/5").event_data().is_none());
        assert_eq!(ok("canvas/pause").name(), "canvas/pause");
    }
}
