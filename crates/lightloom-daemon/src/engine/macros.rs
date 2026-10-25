// SPDX-License-Identifier: GPL-3.0-or-later
//! The macro runtime in the engine (crate `lightloom_macros`, docs/macros.md).
//!
//! * The manager lives on the engine thread; its scripts run on threads of
//!   their own and reach the engine through [`EngineLink`]: requests go in
//!   like the API's (and wait for the answer, at most [`CALL_WAIT`]), getters
//!   read the published snapshot.
//! * The user's scripts are in `<config>/macroscripts/{Actions,Inputs}`
//!   (created at start); SignalRGB's bundled `Macroscripts` folder is listed
//!   too while its library is on.
//! * The engine feeds the bus: effect and layout changes, lock and unlock,
//!   sleep and resume. Device plugins' `keyboard` / `mouse` features feed it
//!   through the sessions' input sink.
//! * The settings are one database value ([`MACROS_KEY`]), so they travel
//!   with the export.
//! * Dashboards follow the runtime through `macros` events carrying the view
//!   of `GET /v1/macros`, sent when it changed (looked at every
//!   [`VIEW_EVERY`], and at once after a request), only while a client listens.
//! * Only real hardware gets this OS's desktop (`SendInput`, the hooks); runs
//!   without hardware get a recording desktop and no hooks.

use super::{Engine, Envelope, Shared};
use lightloom_api::{ApiError, Event, Request};
use lightloom_macros::host::{EngineCall, EngineState, MacroHost, Prompt};
use lightloom_macros::{MacroConfig, MacroError, ScriptDir};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// The database key of the macro settings.
pub const MACROS_KEY: &str = "macros";
/// Longest a macro waits for the engine's answer.
pub const CALL_WAIT: Duration = Duration::from_secs(5);
/// How often the macro view is compared with the one last sent (`macros` events).
pub const VIEW_EVERY: Duration = Duration::from_millis(500);

/// What macro scripts reach of the engine.
pub(crate) struct EngineLink {
    pub(super) shared: Arc<Shared>,
    /// Set while the engine shuts down: calls stop waiting.
    pub(super) closing: AtomicBool,
}

impl EngineLink {
    fn request(&self, request: Request) -> Result<Value, String> {
        if self.closing.load(Ordering::Acquire) {
            return Err("Lightloom is shutting down".into());
        }
        let (reply, mut rx) = oneshot::channel();
        self.shared
            .requests
            .send(Envelope { request, reply })
            .map_err(|_| "Lightloom is shutting down".to_string())?;
        let end = Instant::now() + CALL_WAIT;
        loop {
            match rx.try_recv() {
                Ok(r) => return r.map_err(|e| e.message),
                Err(oneshot::error::TryRecvError::Closed) => return Err("Lightloom is shutting down".into()),
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
            if self.closing.load(Ordering::Acquire) || Instant::now() >= end {
                return Err("the engine did not answer in time".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl MacroHost for EngineLink {
    fn call(&self, call: EngineCall) -> Result<Value, String> {
        let snapshot = self.shared.snapshot.load_full();
        let request = match call {
            EngineCall::Action(url) => Request::Action { url },
            EngineCall::StepEffect { previous } => Request::StepEffect { previous },
            EngineCall::ApplyPreset { effect, preset } => {
                let id = snapshot
                    .effects
                    .iter()
                    .find(|e| e.id == effect || e.title.trim().eq_ignore_ascii_case(effect.trim()))
                    .map(|e| e.id.clone())
                    .ok_or_else(|| format!("Failed to find effect with name: {effect}"))?;
                Request::ApplyPreset {
                    effect: id,
                    name: preset,
                }
            }
            EngineCall::PlayPlaylist(id) => {
                let id = snapshot
                    .playlists
                    .iter()
                    .find(|p| p.id == id || p.name.eq_ignore_ascii_case(&id))
                    .map(|p| p.id.clone())
                    .unwrap_or(id);
                Request::PlayPlaylist { id, start: None }
            }
            EngineCall::StopPlaylist => Request::StopPlaylist,
            EngineCall::Shuffle => Request::ShuffleEffect,
            EngineCall::Brightness(b) => Request::SetLighting {
                brightness: Some(b),
                enabled: None,
            },
            EngineCall::Paused(paused) => Request::SetLighting {
                brightness: None,
                enabled: Some(!paused),
            },
            EngineCall::DeviceEnabled { key, enabled } => Request::SetDevice {
                key,
                enabled: Some(enabled),
                parameters: None,
                shutdown_mode: None,
                blink_low_battery: None,
            },
        };
        self.request(request)
    }

    fn state(&self) -> EngineState {
        let s = self.shared.snapshot.load_full();
        EngineState {
            effect: s.lighting.effect_name.clone(),
            effects: s.effects.iter().map(|e| e.title.clone()).collect(),
            brightness: s.lighting.brightness,
            paused: !s.lighting.enabled,
            devices: s.devices.iter().map(|d| (d.key.clone(), d.enabled)).collect(),
        }
    }

    fn show_prompt(&self, p: &Prompt) -> bool {
        let mut data = serde_json::to_value(p).unwrap_or_default();
        if let Some(o) = data.as_object_mut() {
            o.insert("action".into(), json!("prompt"));
        }
        self.shared
            .events
            .send(Event {
                kind: "action".into(),
                data,
            })
            .is_ok_and(|n| n > 0)
    }

    fn log(&self, script: &str, line: &str) {
        let line = if script.is_empty() {
            tracing::info!(target: "macro", "{line}");
            format!("macros: {line}")
        } else {
            tracing::debug!(target: "macro", script, "{line}");
            format!("[macro {script}] {line}")
        };
        let _ = self.shared.events.send(Event::log(line));
    }
}

/// What the engine last told the macros (effect title, layout).
#[derive(Default)]
pub(super) struct MacroFeed {
    effect: Option<String>,
    layout: String,
    /// The view last sent in a `macros` event.
    view: Option<Value>,
    checked: Option<Instant>,
}

fn api_error(e: MacroError) -> ApiError {
    match e {
        MacroError::NotFound(m) => ApiError::not_found(m),
        MacroError::Invalid(m) => ApiError::bad_request(m),
        MacroError::Conflict(m) => ApiError::conflict(m),
    }
}

impl Engine {
    /// The saved macro settings.
    pub(super) fn macro_config(store: &crate::store::Store) -> MacroConfig {
        store
            .get::<MacroConfig>(MACROS_KEY)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// The user's macro folder.
    fn user_macros(&self) -> PathBuf {
        self.opts.paths.config.join("macroscripts")
    }

    /// List the user's folders (and SignalRGB's while its library is on).
    pub(super) fn sync_macro_dirs(&mut self) {
        let user = self.user_macros();
        if !self.opts.ephemeral {
            for kind in ["Actions", "Inputs"] {
                let _ = std::fs::create_dir_all(user.join(kind));
            }
        }
        let mut dirs = ScriptDir::pair("user", &user).to_vec();
        if self.signalrgb {
            if let Some(app) = lightloom_plugin_host::catalog::signalrgb_app_dir() {
                let bundled = app.join("Signal-x64").join("Macroscripts");
                if bundled.is_dir() {
                    dirs.extend(ScriptDir::pair("signalrgb", &bundled));
                }
            }
        }
        self.macros.set_dirs(dirs);
    }

    /// Feed the engine's changes to the macros, run their housekeeping and
    /// save their settings when they changed.
    pub(super) fn tick_macros(&mut self) {
        let effect = self
            .effect
            .as_deref()
            .and_then(|id| self.effects.get(id))
            .map(|e| e.meta.title.clone());
        if effect != self.macro_feed.effect {
            if let Some(t) = &effect {
                self.macros.effect_changed(t);
            }
            self.macro_feed.effect = effect;
        }
        if self.layout.name != self.macro_feed.layout {
            self.macro_feed.layout = self.layout.name.clone();
            self.macros.layout_changed(&self.layout.name);
        }
        self.macros.tick();
        self.follow_macro_sensors();
        self.save_macros();
        self.publish_macros(false);
    }

    /// Send a `macros` event when the view changed since the last one: from
    /// the tick at most every [`VIEW_EVERY`], at once with `now`. Nothing is
    /// built while no client listens.
    pub(super) fn publish_macros(&mut self, now: bool) {
        if !now && self.macro_feed.checked.is_some_and(|t| t.elapsed() < VIEW_EVERY) {
            return;
        }
        self.macro_feed.checked = Some(Instant::now());
        if self.shared.events.receiver_count() == 0 {
            self.macro_feed.view = None;
            return;
        }
        let Ok(view) = serde_json::to_value(self.macros.view()) else {
            return;
        };
        if self.macro_feed.view.as_ref() == Some(&view) {
            return;
        }
        self.macro_feed.view = Some(view.clone());
        let _ = self.shared.events.send(Event {
            kind: "macros".into(),
            data: view,
        });
    }

    fn save_macros(&mut self) {
        if self.macros.take_changed() {
            if let Err(e) = self.store.set(MACROS_KEY, self.macros.config()) {
                tracing::warn!("saving the macro settings: {e:#}");
            }
        }
    }

    /// The macro requests.
    pub(super) fn handle_macros(&mut self, request: Request) -> Result<Value, ApiError> {
        let answer = self.macro_request(request);
        self.follow_macro_sensors();
        self.publish_macros(true);
        answer
    }

    fn macro_request(&mut self, request: Request) -> Result<Value, ApiError> {
        let view =
            |e: &Engine| serde_json::to_value(e.macros.view()).map_err(|e| ApiError::internal(e.to_string()));
        match request {
            Request::Macros => view(self),
            Request::SetMacros {
                enabled,
                keyboard_hook,
                mouse_hook,
                allow_launch,
            } => {
                self.macros
                    .update_settings(enabled, keyboard_hook, mouse_hook, allow_launch);
                self.save_macros();
                view(self)
            }
            Request::SetMacro {
                id,
                enabled,
                options,
                actions,
            } => {
                self.macros
                    .update_script(&id, enabled, options, actions)
                    .map_err(api_error)?;
                self.save_macros();
                view(self)
            }
            Request::RunMacro { id } => {
                self.macros.run(&id).map_err(api_error)?;
                Ok(json!({ "queued": true }))
            }
            Request::AnswerPrompt { id, text } => {
                self.macros.answer(id, text).map_err(api_error)?;
                Ok(json!({ "answered": true }))
            }
            _ => Err(ApiError::internal("not a macro request")),
        }
    }
}
