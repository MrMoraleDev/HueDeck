// SPDX-License-Identifier: GPL-3.0-or-later
//! What the macro runtime needs from the daemon ([`MacroHost`]), and the
//! `signalrgb.*` methods mapped onto it ([`signalrgb`]).
//!
//! `signalrgb.*` (docs/host-api/macros-system.md §1.4 and the critic pass
//! "SignalRGBEndpoint full method table") maps to the engine:
//!
//! | method | Lightloom |
//! |---|---|
//! | `CurrentEffectName()` | the shown effect's title |
//! | `NextEffectName()` / `PreviousEffectName()` | the neighbours in the library |
//! | `LastEffectName()` | the effect shown before the current one (this run) |
//! | `SelectNextEffect()` / `SelectPreviousEffect()` | step through the library or the playing playlist |
//! | `ApplyEffectPreset(effect, preset)` | load the preset (by effect title or id) |
//! | `StartPlaylist(id)` / `StopPlaylist()` | play / stop a playlist |
//! | `EffectShuffle()` / `SetEffectShuffle(on)` | a flag of the macro runtime; turning it on shows a random effect |
//! | `Get/SetGlobalBrightness`, `IncrementGlobalBrightness(delta)` | global brightness, `0..=100` (invalid values are ignored with a log line) |
//! | `Get/SetCanvasPauseState` | the canvas pause |
//! | `Get/SetDeviceEnabled(uid, on)` | a device's enable (`uid` = device key, case-insensitive) |
//!
//! Engine errors are logged to the script's console and do not throw (as
//! SignalRGB logs `Failed to find device with uid`); bad argument types throw
//! `TypeError`.

use crate::script::{NativeError, NativeResult};
use serde_json::Value;
use std::sync::Mutex;

/// What `signalrgb.*` getters read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineState {
    /// Title of the effect shown.
    pub effect: Option<String>,
    /// Effect titles in library order.
    pub effects: Vec<String>,
    /// Global brightness.
    pub brightness: u8,
    /// The canvas is paused.
    pub paused: bool,
    /// Device keys and whether each is enabled.
    pub devices: Vec<(String, bool)>,
}

/// A request to the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineCall {
    /// A `lightloom://` action path (`system.action`).
    Action(String),
    /// Next (or previous) effect.
    StepEffect {
        /// Previous.
        previous: bool,
    },
    /// Load a preset of an effect (title or id).
    ApplyPreset {
        /// Effect.
        effect: String,
        /// Preset.
        preset: String,
    },
    /// Play a playlist.
    PlayPlaylist(String),
    /// Stop the playlist.
    StopPlaylist,
    /// Show a random effect.
    Shuffle,
    /// Global brightness.
    Brightness(u8),
    /// Pause (or resume) the canvas.
    Paused(bool),
    /// Enable or disable a device.
    DeviceEnabled {
        /// Device key.
        key: String,
        /// Enable.
        enabled: bool,
    },
}

/// A `system.prompt` for the dashboard.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// Answer with this id.
    pub id: u64,
    /// The asking script's name.
    pub script: String,
    /// Title.
    pub title: String,
    /// Message.
    pub message: String,
    /// Placeholder of the text field.
    pub placeholder: String,
    /// Confirm button.
    pub confirm_text: String,
    /// Cancel button.
    pub cancel_text: String,
}

/// The daemon side of the macro runtime. Called on macro threads (never on
/// the engine thread): a call may wait for the engine.
pub trait MacroHost: Send + Sync + 'static {
    /// Carry out `call`; the error is a message for the script's console.
    fn call(&self, call: EngineCall) -> Result<Value, String>;
    /// The current state.
    fn state(&self) -> EngineState;
    /// Show a prompt in the dashboard; `false` when no dashboard is connected.
    fn show_prompt(&self, prompt: &Prompt) -> bool;
    /// A line of a script (`console.*`, failures), or of the runtime (`script` empty).
    fn log(&self, script: &str, line: &str);
}

/// What the runtime remembers for `signalrgb.*` across scripts.
#[derive(Debug, Default)]
pub struct SignalRgbState {
    /// (current effect, the one before it).
    effects: Mutex<(Option<String>, Option<String>)>,
    shuffle: Mutex<bool>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl SignalRgbState {
    /// The effect shown changed to `title`.
    pub fn effect_changed(&self, title: &str) {
        let mut e = lock(&self.effects);
        if e.0.as_deref() != Some(title) {
            let before = e.0.take();
            *e = (Some(title.to_string()), before.or(e.1.take()));
        }
    }
}

fn want_bool(v: Option<&Value>, what: &str) -> Result<bool, NativeError> {
    match v {
        Some(Value::Bool(b)) => Ok(*b),
        Some(Value::Number(n)) => Ok(n.as_f64().is_some_and(|f| f != 0.0)),
        Some(Value::String(s)) if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false") => {
            Ok(s.eq_ignore_ascii_case("true"))
        }
        _ => Err(NativeError::type_error(format!(
            "{what}: Parameter must be a boolean!"
        ))),
    }
}

fn want_str<'a>(v: Option<&'a Value>, what: &str) -> Result<&'a str, NativeError> {
    v.and_then(Value::as_str)
        .ok_or_else(|| NativeError::type_error(format!("{what}: Parameter must be a string!")))
}

fn want_num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|f: &f64| f.is_finite())
}

/// `effects[i ± 1]` around the current effect, wrapping.
fn neighbour(s: &EngineState, step: isize) -> String {
    let Some(cur) = s.effect.as_deref() else {
        return String::new();
    };
    let n = s.effects.len() as isize;
    match s.effects.iter().position(|e| e == cur) {
        Some(i) if n > 0 => s.effects[((i as isize + step).rem_euclid(n)) as usize].clone(),
        _ => String::new(),
    }
}

/// `signalrgb.<method>(args)`. `log` writes to the calling script's console.
pub fn signalrgb(
    host: &dyn MacroHost,
    state: &SignalRgbState,
    method: &str,
    args: &[Value],
    log: &dyn Fn(&str),
) -> NativeResult {
    let a0 = args.first();
    let a1 = args.get(1);
    // Engine requests: a failure is logged, not thrown.
    let run = |call: EngineCall| -> NativeResult {
        if let Err(e) = host.call(call) {
            log(&format!("{method}: {e}"));
        }
        Ok(Value::Null)
    };
    match method {
        "CurrentEffectName" => Ok(Value::String(host.state().effect.unwrap_or_default())),
        "NextEffectName" => Ok(Value::String(neighbour(&host.state(), 1))),
        "PreviousEffectName" => Ok(Value::String(neighbour(&host.state(), -1))),
        "LastEffectName" => Ok(Value::String(lock(&state.effects).1.clone().unwrap_or_default())),
        "SelectNextEffect" => run(EngineCall::StepEffect { previous: false }),
        "SelectPreviousEffect" => run(EngineCall::StepEffect { previous: true }),
        "ApplyEffectPreset" => {
            let effect = want_str(a0, "ApplyEffectPreset()")?.to_string();
            let preset = want_str(a1, "ApplyEffectPreset()")?.to_string();
            run(EngineCall::ApplyPreset { effect, preset })
        }
        "StartPlaylist" => {
            let id = want_str(a0, "StartPlaylist()")?;
            if id.is_empty() {
                log("StartPlaylist: no playlist selected");
                return Ok(Value::Null);
            }
            run(EngineCall::PlayPlaylist(id.to_string()))
        }
        "StopPlaylist" => run(EngineCall::StopPlaylist),
        "EffectShuffle" => Ok(Value::Bool(*lock(&state.shuffle))),
        "SetEffectShuffle" => {
            let on = want_bool(a0, "SetEffectShuffle()")?;
            *lock(&state.shuffle) = on;
            if on {
                run(EngineCall::Shuffle)
            } else {
                Ok(Value::Null)
            }
        }
        "GetGlobalBrightness" => Ok(Value::from(host.state().brightness)),
        "SetGlobalBrightness" => match want_num(a0) {
            Some(v) if (0.0..=100.0).contains(&v) => run(EngineCall::Brightness(v.round() as u8)),
            _ => {
                log(&format!(
                    "SetGlobalBrightness ignored invalid value: {}",
                    a0.cloned().unwrap_or(Value::Null)
                ));
                Ok(Value::Null)
            }
        },
        "IncrementGlobalBrightness" => match want_num(a0) {
            Some(d) => {
                let b = (f64::from(host.state().brightness) + d).clamp(0.0, 100.0);
                run(EngineCall::Brightness(b.round() as u8))
            }
            None => {
                log(&format!(
                    "IncrementGlobalBrightness ignored invalid value: {}",
                    a0.cloned().unwrap_or(Value::Null)
                ));
                Ok(Value::Null)
            }
        },
        "GetCanvasPauseState" => Ok(Value::Bool(host.state().paused)),
        "SetCanvasPauseState" => {
            let paused = want_bool(a0, "SetCanvasPauseState()")?;
            run(EngineCall::Paused(paused))
        }
        "GetDeviceEnabled" => {
            let uid = want_str(a0, "GetDeviceEnabled()")?;
            match host
                .state()
                .devices
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(uid))
            {
                Some((_, on)) => Ok(Value::Bool(*on)),
                None => {
                    log(&format!(
                        "GetDeviceEnabled: Failed to find device with uid: {uid}"
                    ));
                    Ok(Value::Bool(false))
                }
            }
        }
        "SetDeviceEnabled" => {
            let uid = want_str(a0, "SetDeviceEnabled()")?;
            let enabled = want_bool(a1, "SetDeviceEnabled()")?;
            let key = host
                .state()
                .devices
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(uid))
                .map(|(k, _)| k.clone());
            match key {
                Some(key) => run(EngineCall::DeviceEnabled { key, enabled }),
                None => {
                    log(&format!(
                        "SetDeviceEnabled: Failed to find device with uid: {uid}"
                    ));
                    Ok(Value::Null)
                }
            }
        }
        other => Err(NativeError::type_error(format!(
            "signalrgb.{other} is not a function"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<EngineCall>>,
        state: EngineState,
    }

    impl MacroHost for Fake {
        fn call(&self, call: EngineCall) -> Result<Value, String> {
            let fail = matches!(&call, EngineCall::PlayPlaylist(id) if id == "missing");
            lock(&self.calls).push(call);
            if fail {
                Err("no playlist missing".into())
            } else {
                Ok(Value::Null)
            }
        }
        fn state(&self) -> EngineState {
            self.state.clone()
        }
        fn show_prompt(&self, _: &Prompt) -> bool {
            false
        }
        fn log(&self, _: &str, _: &str) {}
    }

    #[test]
    fn methods_map_onto_the_engine() {
        let host = Fake {
            state: EngineState {
                effect: Some("B".into()),
                effects: vec!["A".into(), "B".into(), "C".into()],
                brightness: 95,
                paused: false,
                devices: vec![("1532:007C:X".into(), true)],
            },
            ..Fake::default()
        };
        let st = SignalRgbState::default();
        let lines = Mutex::new(Vec::<String>::new());
        let log = |l: &str| lock(&lines).push(l.to_string());
        let call = |m: &str, a: Value| signalrgb(&host, &st, m, a.as_array().unwrap(), &log);
        assert_eq!(call("CurrentEffectName", json!([])), Ok(json!("B")));
        assert_eq!(call("NextEffectName", json!([])), Ok(json!("C")));
        assert_eq!(call("PreviousEffectName", json!([])), Ok(json!("A")));
        assert_eq!(call("GetGlobalBrightness", json!([])), Ok(json!(95)));
        call("IncrementGlobalBrightness", json!([10])).unwrap();
        call("SetGlobalBrightness", json!([140])).unwrap();
        call("SetGlobalBrightness", json!(["50"])).unwrap();
        assert_eq!(call("GetDeviceEnabled", json!(["1532:007c:x"])), Ok(json!(true)));
        call("SetDeviceEnabled", json!(["1532:007c:x", false])).unwrap();
        call("SetDeviceEnabled", json!(["nope", false])).unwrap();
        call("StartPlaylist", json!(["missing"])).unwrap();
        assert!(call("SetCanvasPauseState", json!(["maybe"])).is_err());
        call("SetCanvasPauseState", json!([true])).unwrap();
        call("SetEffectShuffle", json!([true])).unwrap();
        assert_eq!(call("EffectShuffle", json!([])), Ok(json!(true)));
        assert_eq!(
            *lock(&host.calls),
            vec![
                EngineCall::Brightness(100),
                EngineCall::Brightness(50),
                EngineCall::DeviceEnabled {
                    key: "1532:007C:X".into(),
                    enabled: false
                },
                EngineCall::PlayPlaylist("missing".into()),
                EngineCall::Paused(true),
                EngineCall::Shuffle,
            ]
        );
        let lines = lock(&lines).clone();
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines[0].contains("ignored invalid value: 140"));
        assert!(lines[1].contains("Failed to find device with uid: nope"));
        assert!(lines[2].contains("no playlist missing"));
        assert_eq!(call("LastEffectName", json!([])), Ok(json!("")));
        st.effect_changed("A");
        st.effect_changed("B");
        st.effect_changed("B");
        assert_eq!(call("LastEffectName", json!([])), Ok(json!("A")));
    }
}
