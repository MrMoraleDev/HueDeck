// SPDX-License-Identifier: GPL-3.0-or-later
//! The action router: `lightloom://` URLs ([`lightloom_api::actions`]) carried
//! out with the engine's ordinary requests, and the opt-in URL-scheme
//! registration ([`crate::scheme`]).
//!
//! * `effect/apply/<name>` finds the effect by id, by id without its source
//!   (`rainbow` for `lightloom:rainbow`) or by title (case-insensitive), coerces
//!   the query values to the controls' types (strictly: a number that is not
//!   one, outside the control's range, a combobox value it does not list or a
//!   colour that is not `#rrggbb` is `400 invalid_action`; a property the
//!   effect does not have is ignored and reported) and applies it.
//! * `layout/applyprevious` goes back to the layout shown before the current
//!   one in this run.
//! * `profile/apply/<name>` applies a saved profile; `layout/apply/<name>`
//!   applies the profile `<name>` when there is no such layout but such a
//!   profile (SignalRGB macros switch "layouts", which hold more than placements).
//! * `app/restart` reloads everything in place (devices, plugins, effects);
//!   the process and the API stay up.
//! * `app/raiseWindow`, `view/<page>` and `app/toast` reach the dashboard as an
//!   `action` event; a toast is also a system notification.
//! * Unknown paths are dropped with a warning (logged, `dropped: true`);
//!   `effect/install` and `effect/update` are refused (`409 action_refused`).

use super::{internal, keys, Engine};
use crate::scheme::UrlScheme;
use lightloom_api::actions::{self, Action};
use lightloom_api::{ApiError, Event, Request};
use lightloom_effects::model::{ControlMeta, ControlType};
use lightloom_hal::notify::Notification;
use serde_json::{json, Map, Number, Value};

/// What the router keeps between actions.
pub(super) struct ActionState {
    /// How the URL scheme is registered.
    pub(super) url_scheme: UrlScheme,
    /// The user turned the URL scheme on.
    pub(super) url_scheme_on: bool,
    /// Why the registration is not what the setting says.
    pub(super) url_scheme_notice: Option<String>,
    /// The layout shown before the current one (this run).
    pub(super) previous_layout: Option<String>,
}

impl ActionState {
    pub(super) fn new(url_scheme: UrlScheme) -> Self {
        Self {
            url_scheme,
            url_scheme_on: false,
            url_scheme_notice: None,
            previous_layout: None,
        }
    }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError {
        status: 400,
        code: "invalid_action".into(),
        message: message.into(),
    }
}

/// `raw` as the value of control `c`, strictly.
fn coerce(c: &ControlMeta, raw: &str) -> Result<Value, ApiError> {
    let p = &c.property;
    match c.control_type {
        ControlType::Number | ControlType::Hue => {
            let v: f64 = raw
                .trim()
                .parse()
                .ok()
                .filter(|v: &f64| v.is_finite())
                .ok_or_else(|| invalid(format!("{p} must be a number (got {raw:?})")))?;
            let (lo, hi) = match c.control_type {
                ControlType::Hue => (Some(0.0), Some(360.0)),
                _ => (c.min, c.max),
            };
            if lo.is_some_and(|lo| v < lo) || hi.is_some_and(|hi| v > hi) {
                return Err(invalid(format!(
                    "{p} must be within {}..{} (got {v})",
                    lo.map_or("".into(), |x| x.to_string()),
                    hi.map_or("".into(), |x| x.to_string())
                )));
            }
            Ok(if v.fract() == 0.0 && v.abs() < 9.0e15 {
                Value::from(v as i64)
            } else {
                Number::from_f64(v).map_or(Value::Null, Value::Number)
            })
        }
        ControlType::Boolean => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(Value::Bool(true)),
            "false" | "0" => Ok(Value::Bool(false)),
            _ => Err(invalid(format!("{p} must be true or false (got {raw:?})"))),
        },
        ControlType::Color => {
            let ok =
                raw.len() == 7 && raw.starts_with('#') && raw[1..].bytes().all(|b| b.is_ascii_hexdigit());
            if ok {
                Ok(Value::String(raw.to_ascii_lowercase()))
            } else {
                Err(invalid(format!("{p} must be a colour #rrggbb (got {raw:?})")))
            }
        }
        ControlType::Combobox => {
            if c.values.is_empty() || c.values.iter().any(|v| v == raw) {
                Ok(Value::String(raw.to_string()))
            } else {
                Err(invalid(format!(
                    "{p} must be one of {} (got {raw:?})",
                    c.values.join(", ")
                )))
            }
        }
        ControlType::Textfield | ControlType::Sensor => Ok(Value::String(raw.to_string())),
    }
}

impl Engine {
    /// `Request::Action`: parse `url` and carry it out.
    pub(super) fn run_action(&mut self, url: &str) -> Result<Value, ApiError> {
        let action = actions::parse(url).map_err(|e| invalid(e.0))?;
        let name = action.name();
        match action {
            Action::Unknown { path, warning } => {
                let line = format!("action dropped: {warning} ({path})");
                tracing::warn!("{line}");
                self.log(line);
                Ok(json!({ "action": name, "dropped": true, "warning": warning }))
            }
            Action::Refused { path, reason } => {
                self.log(format!("action refused: {path}"));
                Err(ApiError {
                    status: 409,
                    code: "action_refused".into(),
                    message: reason,
                })
            }
            action => {
                let result = self.dispatch_action(action)?;
                self.log(format!("action {name}"));
                Ok(json!({ "action": name, "result": result }))
            }
        }
    }

    fn dispatch_action(&mut self, action: Action) -> Result<Value, ApiError> {
        if let Some(data) = action.event_data() {
            let delivered = self
                .shared
                .events
                .send(Event {
                    kind: "action".into(),
                    data,
                })
                .unwrap_or(0);
            if let Action::Toast { title, message } = &action {
                let shown = self
                    .opts
                    .notifier
                    .notify(&Notification {
                        title: title.clone(),
                        body: message.clone(),
                    })
                    .is_ok();
                return Ok(json!({ "delivered": delivered, "notified": shown }));
            }
            return Ok(json!({ "delivered": delivered }));
        }
        match action {
            Action::Restart => {
                self.log("restarting the engine (app/restart)");
                self.reload_everything();
                Ok(json!({ "restarted": true }))
            }
            // SignalRGB's macros switch "layouts"; a name that is no layout but a
            // profile applies the profile (the closer match of SignalRGB's layouts).
            Action::ApplyLayout { name } => {
                let is_layout = self.store.layout(&name).map_err(internal)?.is_some();
                if !is_layout && self.store.profile(&name).map_err(internal)?.is_some() {
                    return self.handle(Request::ApplyProfile { name });
                }
                self.handle(Request::ApplyLayout { name })
            }
            Action::ApplyProfile { name } => self.handle(Request::ApplyProfile { name }),
            Action::ApplyPreviousLayout => {
                let Some(name) = self.actions.previous_layout.clone() else {
                    return Err(ApiError::conflict("no previous layout in this run"));
                };
                self.handle(Request::ApplyLayout { name })
            }
            Action::ApplyEffect { name, properties } => {
                let Some(id) = self.find_effect(&name) else {
                    return Err(ApiError::not_found(format!("no effect {name:?}")));
                };
                let controls = self
                    .effects
                    .get(&id)
                    .map(|e| e.meta.properties.clone())
                    .unwrap_or_default();
                let mut values = Map::new();
                let mut ignored = Vec::new();
                for (k, raw) in properties {
                    match controls.iter().find(|c| c.property == k) {
                        Some(c) => {
                            values.insert(k, coerce(c, &raw)?);
                        }
                        None => ignored.push(k),
                    }
                }
                let mut r = self.handle(Request::ApplyEffect {
                    id: id.clone(),
                    values,
                })?;
                if let Some(obj) = r.as_object_mut() {
                    obj.insert("effect".into(), json!(id));
                    if !ignored.is_empty() {
                        obj.insert("ignored".into(), json!(ignored));
                    }
                }
                Ok(r)
            }
            Action::SetDevice { uid, enabled } => {
                let key = self
                    .store
                    .devices()
                    .map_err(internal)?
                    .into_iter()
                    .map(|r| r.key)
                    .find(|k| k.eq_ignore_ascii_case(&uid))
                    .ok_or_else(|| ApiError::not_found(format!("no device {uid}")))?;
                self.handle(Request::SetDevice {
                    key,
                    enabled: Some(enabled),
                    parameters: None,
                    shutdown_mode: None,
                    blink_low_battery: None,
                })
            }
            Action::PlayPlaylist { id } => self.handle(Request::PlayPlaylist { id, start: None }),
            Action::StopPlaylist { id } => {
                if self.player.playback().is_some_and(|p| p.id == id) {
                    return self.handle(Request::StopPlaylist);
                }
                let known = self
                    .store
                    .playlists()
                    .map_err(internal)?
                    .iter()
                    .any(|p| p.id == id);
                if known {
                    Ok(json!({ "stopped": false }))
                } else {
                    Err(ApiError::not_found(format!("no playlist {id}")))
                }
            }
            Action::Brightness(b) => self.handle(Request::SetLighting {
                brightness: Some(b),
                enabled: None,
            }),
            Action::Canvas { enabled } => self.handle(Request::SetLighting {
                brightness: None,
                enabled: Some(enabled),
            }),
            // Handled above (events, dropped, refused).
            Action::RaiseWindow
            | Action::Toast { .. }
            | Action::View { .. }
            | Action::Unknown { .. }
            | Action::Refused { .. } => Ok(Value::Null),
        }
    }

    /// The effect `name` names: an id, an id without its `<source>:`, or a title.
    fn find_effect(&self, name: &str) -> Option<String> {
        if self.effects.get(name).is_some() {
            return Some(name.to_string());
        }
        let short = |id: &str| id.split_once(':').map_or(id.to_string(), |(_, n)| n.to_string());
        self.effects
            .iter()
            .find(|e| short(&e.id).eq_ignore_ascii_case(name))
            .or_else(|| {
                self.effects
                    .iter()
                    .find(|e| e.meta.title.trim().eq_ignore_ascii_case(name.trim()))
            })
            .map(|e| e.id.clone())
    }

    /// `Request::SetUrlScheme`: register or remove the `lightloom://` scheme.
    pub(super) fn set_url_scheme(&mut self, enabled: bool) -> Result<Value, ApiError> {
        let failed = |message: String| ApiError {
            status: 500,
            code: "url_scheme_failed".into(),
            message,
        };
        if enabled {
            if let Some(reason) = self.actions.url_scheme.unavailable_reason() {
                return Err(ApiError::conflict(format!(
                    "lightloom:// links cannot be registered: {reason}"
                )));
            }
            self.actions.url_scheme.register().map_err(failed)?;
            self.log(format!(
                "lightloom:// links registered (HKCU\\{}: {})",
                crate::scheme::SCHEME_KEY,
                self.actions.url_scheme.command().unwrap_or_default()
            ));
        } else {
            self.actions.url_scheme.unregister().map_err(failed)?;
            if self.actions.url_scheme_on {
                self.log("lightloom:// links unregistered");
            }
        }
        self.store.set(keys::URL_SCHEME, &enabled).map_err(internal)?;
        self.actions.url_scheme_on = enabled;
        self.actions.url_scheme_notice = None;
        self.dirty = true;
        Ok(json!({ "urlScheme": enabled }))
    }

    /// Read the URL-scheme setting; while it is on, check the registration and
    /// (`repair`, at startup) write it again when the client moved.
    pub(super) fn sync_url_scheme(&mut self, repair: bool) {
        let on = self
            .store
            .get::<bool>(keys::URL_SCHEME)
            .ok()
            .flatten()
            .unwrap_or(false);
        self.actions.url_scheme_on = on;
        self.actions.url_scheme_notice = None;
        if !on {
            return;
        }
        if let Some(reason) = self.actions.url_scheme.unavailable_reason() {
            self.actions.url_scheme_notice = Some(format!("lightloom:// links are not registered: {reason}"));
            return;
        }
        if self.actions.url_scheme.is_current() {
            return;
        }
        if !repair {
            self.actions.url_scheme_notice = Some(
                "lightloom:// links are not registered for this copy of Lightloom: turn the setting off and on"
                    .into(),
            );
            return;
        }
        match self.actions.url_scheme.register() {
            Ok(()) => self.log("lightloom:// links registered again for this copy of Lightloom"),
            Err(e) => {
                self.actions.url_scheme_notice = Some(format!("lightloom:// links are not registered: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(t: ControlType) -> ControlMeta {
        let mut c: ControlMeta = serde_json::from_value(json!({
            "property": "p", "label": "P", "type": "number"
        }))
        .unwrap();
        c.control_type = t;
        c
    }

    #[test]
    fn query_values_are_coerced_strictly() {
        let mut n = control(ControlType::Number);
        n.min = Some(0.0);
        n.max = Some(10.0);
        assert_eq!(coerce(&n, "3").unwrap(), json!(3));
        assert_eq!(coerce(&n, "2.5").unwrap(), json!(2.5));
        assert!(coerce(&n, "11").is_err());
        assert!(coerce(&n, "-1").is_err());
        assert!(coerce(&n, "fast").is_err());
        assert!(coerce(&n, "NaN").is_err());
        let h = control(ControlType::Hue);
        assert_eq!(coerce(&h, "360").unwrap(), json!(360));
        assert!(coerce(&h, "361").is_err());
        let b = control(ControlType::Boolean);
        assert_eq!(coerce(&b, "FALSE").unwrap(), json!(false));
        assert_eq!(coerce(&b, "1").unwrap(), json!(true));
        assert!(coerce(&b, "yes").is_err());
        let c = control(ControlType::Color);
        assert_eq!(coerce(&c, "#FF0000").unwrap(), json!("#ff0000"));
        assert!(coerce(&c, "red").is_err());
        assert!(coerce(&c, "#ff00").is_err());
        let mut l = control(ControlType::Combobox);
        l.values = vec!["Left".into(), "Right".into()];
        assert_eq!(coerce(&l, "Right").unwrap(), json!("Right"));
        assert!(coerce(&l, "Up").is_err());
        let t = control(ControlType::Textfield);
        assert_eq!(coerce(&t, "any text").unwrap(), json!("any text"));
    }
}
