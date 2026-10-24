// SPDX-License-Identifier: GPL-3.0-or-later
//! Storing lighting in a device's own memory, and what plugins do when HueDeck closes
//! (HueDeck extensions, docs/host-api/save-shutdown.md).
//!
//! **Save to devices.** A plugin opts in with `SaveCapabilities()` (`{static, effects}`)
//! and an exported `SaveToDevice(request)`. The host calls it only:
//!
//! * on an explicit request from a client that confirmed it (the daemon refuses
//!   requests without `confirm`), never on its own;
//! * at most once per [`SAVE_INTERVAL`] per device ([`SaveLimiter`], kept by the
//!   [`crate::manager::DeviceManager`]; previews do not count);
//! * between two `Render()` calls of the device's session, on the frame the device
//!   showed last (`request.colors`, one `[r, g, b]` per LED, `LedPositions()` order);
//! * for an SMBus device, with the plugin's `SmbusDeniedRegisters()` writable for
//!   the duration of that one call only ([`crate::smbus::GuardedBus::set_save_window`]);
//!   outside it they stay refused, whatever the plugin does.
//!
//! A preview (`request.action == "preview"`) runs the same export with every write
//! recorded and never sent (dry run for HID and SMBus alike), and returns what would
//! be stored: `{zones: [{name, color}], effect, note}` or `{unsupported: "why"}`.
//!
//! **When HueDeck closes.** `Shutdown(SystemSuspending, mode)` gets a second argument,
//! the [`ShutdownMode`] (`"restore"`, `"keep"` or `"off"`); SignalRGB plugins ignore it.
//! A plugin that implements a mode lists it in `ShutdownModes()`. For the others the
//! host falls back to what SignalRGB plugins already honour ([`shutdown_fallback`]):
//! `shutdownColor` (and `shutdownMode = "SignalRGB"` where that parameter exists).
//! "Keep" is always volatile: no plugin may use its save path for it.

use crate::meta::{ControllableParameter, PluginMeta};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Shortest time between two saves to one device (flash and EEPROM wear out).
pub const SAVE_INTERVAL: Duration = Duration::from_secs(60);

/// Effect names plugins may advertise in `SaveCapabilities().effects` (others are
/// dropped with a warning when the plugin is read).
pub const KNOWN_EFFECTS: &[&str] = &["spectrum", "breathing", "wave", "rainbow"];

/// `SaveCapabilities()` (HueDeck extension): what a plugin can store on its devices.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveCaps {
    /// The colours of the current frame (one colour per zone or LED, as the device stores them).
    #[serde(rename = "static")]
    pub static_colors: bool,
    /// Built-in hardware effects it can store instead ([`KNOWN_EFFECTS`]).
    pub effects: Vec<String>,
}

impl SaveCaps {
    /// Read `SaveCapabilities()`'s value; `None` when it stores nothing.
    pub fn from_json(v: &Value, warnings: &mut Vec<String>) -> Option<Self> {
        let o = v.as_object()?;
        let static_colors = o.get("static").and_then(Value::as_bool).unwrap_or(false);
        let mut effects = Vec::new();
        for e in o.get("effects").and_then(Value::as_array).into_iter().flatten() {
            match e.as_str() {
                Some(name) if KNOWN_EFFECTS.contains(&name) => {
                    if !effects.iter().any(|x| x == name) {
                        effects.push(name.to_string());
                    }
                }
                _ => warnings.push(format!("SaveCapabilities(): unknown effect {e}")),
            }
        }
        (static_colors || !effects.is_empty()).then_some(Self {
            static_colors,
            effects,
        })
    }

    /// Whether `choice` is one of these capabilities.
    pub fn allows(&self, choice: &SaveChoice) -> bool {
        match &choice.effect {
            None => self.static_colors,
            Some(e) => self.effects.iter().any(|x| x == e),
        }
    }
}

/// What to store on one device.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveChoice {
    /// A built-in effect the plugin advertises; `None`: the current colours (static).
    #[serde(default)]
    pub effect: Option<String>,
}

/// Preview or store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveAction {
    /// Say what would be stored; nothing is sent.
    Preview,
    /// Store it.
    Save,
}

impl SaveAction {
    /// `request.action`.
    pub fn as_str(self) -> &'static str {
        match self {
            SaveAction::Preview => "preview",
            SaveAction::Save => "save",
        }
    }
}

/// The argument of `SaveToDevice(request)`.
/// `colors`: the device's LEDs on the frame it showed last, in `LedPositions()`
/// order (subdevices and channels: the plugin samples them itself, the frame
/// stays the same for the whole call).
pub fn request_json(action: SaveAction, choice: &SaveChoice, colors: &[[u8; 3]]) -> Value {
    json!({
        "action": action.as_str(),
        "effect": choice.effect,
        "colors": colors.iter().map(|c| json!([c[0], c[1], c[2]])).collect::<Vec<_>>(),
    })
}

/// What one `SaveToDevice` call came to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOutcome {
    /// Device key.
    pub key: String,
    /// Preview or save.
    pub action: SaveAction,
    /// The plugin returned without throwing and did not call it unsupported.
    pub ok: bool,
    /// Writes were recorded, never sent (previews, `--dry-run`).
    pub dry_run: bool,
    /// What the plugin returned (`{zones, effect, note}`), or `Null`.
    pub result: Value,
    /// Why it failed or cannot be done.
    pub error: Option<String>,
}

impl SaveOutcome {
    /// A failure before or around the plugin call.
    pub fn failed(key: &str, action: SaveAction, error: impl Into<String>) -> Self {
        Self {
            key: key.to_string(),
            action,
            ok: false,
            dry_run: false,
            result: Value::Null,
            error: Some(error.into()),
        }
    }

    /// The outcome of a plugin call that returned `result`.
    pub fn from_result(key: &str, action: SaveAction, dry_run: bool, result: Value) -> Self {
        let unsupported = result
            .get("unsupported")
            .and_then(Value::as_str)
            .map(String::from);
        Self {
            key: key.to_string(),
            action,
            ok: unsupported.is_none(),
            dry_run,
            result,
            error: unsupported,
        }
    }
}

/// Why a save was not even attempted.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SaveRefused {
    /// The device has no running session.
    #[error("{0} is not running")]
    NotRunning(String),
    /// The plugin does not store this.
    #[error("{0}")]
    Unsupported(String),
    /// Saved too recently.
    #[error("saved {elapsed} s ago; wait {wait} s more (one save per device per minute)")]
    TooSoon {
        /// Seconds since the last save.
        elapsed: u64,
        /// Seconds left.
        wait: u64,
    },
}

/// One save per device per [`SAVE_INTERVAL`], in memory (not kept across restarts).
#[derive(Debug)]
pub struct SaveLimiter {
    interval: Duration,
    last: BTreeMap<String, Instant>,
}

impl Default for SaveLimiter {
    fn default() -> Self {
        Self::new(SAVE_INTERVAL)
    }
}

impl SaveLimiter {
    /// A limiter allowing one save per `interval` per device.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: BTreeMap::new(),
        }
    }

    /// Whether `key` may be saved now (does not record anything).
    pub fn check(&self, key: &str) -> Result<(), SaveRefused> {
        self.check_at(key, Instant::now())
    }

    fn check_at(&self, key: &str, now: Instant) -> Result<(), SaveRefused> {
        match self.last.get(key) {
            Some(t) if now.duration_since(*t) < self.interval => {
                let elapsed = now.duration_since(*t);
                Err(SaveRefused::TooSoon {
                    elapsed: elapsed.as_secs(),
                    wait: (self.interval - elapsed).as_secs().max(1),
                })
            }
            _ => Ok(()),
        }
    }

    /// Record a save of `key` now when allowed.
    pub fn take(&mut self, key: &str) -> Result<(), SaveRefused> {
        let now = Instant::now();
        self.check_at(key, now)?;
        self.last.insert(key.to_string(), now);
        Ok(())
    }

    /// Seconds until `key` may be saved again (0: now).
    pub fn wait_secs(&self, key: &str) -> u64 {
        match self.check(key) {
            Err(SaveRefused::TooSoon { wait, .. }) => wait,
            _ => 0,
        }
    }
}

/// What a plugin's `Shutdown()` leaves the device showing when HueDeck closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShutdownMode {
    /// The device's own (built-in or startup) lighting, as plugins did so far.
    #[default]
    Restore,
    /// The last colours HueDeck showed (volatile: never stored in the device).
    Keep,
    /// Lights off.
    Off,
}

impl ShutdownMode {
    /// The name plugins get (`Shutdown(false, name)`).
    pub fn as_str(self) -> &'static str {
        match self {
            ShutdownMode::Restore => "restore",
            ShutdownMode::Keep => "keep",
            ShutdownMode::Off => "off",
        }
    }

    /// Parse a name (`restore`, `keep`, `off`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "restore" => Some(ShutdownMode::Restore),
            "keep" => Some(ShutdownMode::Keep),
            "off" => Some(ShutdownMode::Off),
            _ => None,
        }
    }

    /// Every mode, in display order.
    pub const ALL: [ShutdownMode; 3] = [ShutdownMode::Restore, ShutdownMode::Keep, ShutdownMode::Off];
}

/// Read `ShutdownModes()` (HueDeck extension): the modes a plugin implements itself.
pub fn shutdown_modes_from_json(v: &Value, warnings: &mut Vec<String>) -> Vec<ShutdownMode> {
    let mut out = Vec::new();
    for m in v.as_array().into_iter().flatten() {
        match m.as_str().and_then(ShutdownMode::parse) {
            Some(mode) if !out.contains(&mode) => out.push(mode),
            Some(_) => {}
            None => warnings.push(format!("ShutdownModes(): unknown mode {m}")),
        }
    }
    out
}

/// How the host gets `mode` out of a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownPlan {
    /// The plugin implements it: `Shutdown(false, mode)` and nothing else.
    Native,
    /// Set these parameters (`shutdownColor`, `shutdownMode`) first, then `Shutdown(false, mode)`.
    Parameters(Map<String, Value>),
    /// Nothing the plugin understands: it does what it always does (restore).
    Unsupported,
}

/// The modes `meta`'s plugin can carry out, natively or through the fallback.
pub fn supported_modes(meta: &PluginMeta) -> Vec<ShutdownMode> {
    ShutdownMode::ALL
        .into_iter()
        .filter(|m| !matches!(shutdown_fallback(meta, *m, [0, 0, 0]), ShutdownPlan::Unsupported))
        .collect()
}

fn param<'a>(meta: &'a PluginMeta, name: &str) -> Option<&'a ControllableParameter> {
    meta.controllable_parameters.iter().find(|p| p.property == name)
}

/// The plan for `mode` (see [`ShutdownPlan`]). `last` is the colour the device showed
/// last (the average of its LEDs), used for "keep" through `shutdownColor`.
///
/// SignalRGB plugins paint `shutdownColor` in `Shutdown(false)` (a few only when their
/// `shutdownMode` parameter is `"SignalRGB"`, the others go back to a hardware mode).
/// "Restore" changes nothing: plugins keep doing what they did before this setting.
pub fn shutdown_fallback(meta: &PluginMeta, mode: ShutdownMode, last: [u8; 3]) -> ShutdownPlan {
    if mode == ShutdownMode::Restore || meta.shutdown_modes.contains(&mode) {
        return ShutdownPlan::Native;
    }
    if !meta.lifecycle.shutdown || param(meta, "shutdownColor").is_none() {
        return ShutdownPlan::Unsupported;
    }
    let color = match mode {
        ShutdownMode::Keep => last,
        _ => [0, 0, 0],
    };
    let mut values = Map::new();
    values.insert(
        "shutdownColor".into(),
        Value::String(format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])),
    );
    if let Some(p) = param(meta, "shutdownMode") {
        if p.values.iter().any(|v| v == "SignalRGB") {
            values.insert("shutdownMode".into(), Value::String("SignalRGB".into()));
        }
    }
    ShutdownPlan::Parameters(values)
}

/// The average of `colors` (black for none).
pub fn average(colors: &[[u8; 3]]) -> [u8; 3] {
    if colors.is_empty() {
        return [0, 0, 0];
    }
    let mut sum = [0u64; 3];
    for c in colors {
        for i in 0..3 {
            sum[i] += u64::from(c[i]);
        }
    }
    let n = colors.len() as u64;
    [
        ((sum[0] + n / 2) / n) as u8,
        ((sum[1] + n / 2) / n) as u8,
        ((sum[2] + n / 2) / n) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{ParamType, ParamValue};

    fn color_param(name: &str, values: &[&str]) -> ControllableParameter {
        ControllableParameter {
            property: name.into(),
            group: None,
            label: name.into(),
            description: None,
            tooltip: None,
            param_type: if values.is_empty() {
                ParamType::Color
            } else {
                ParamType::Combobox
            },
            values: values.iter().map(|s| s.to_string()).collect(),
            min: None,
            max: None,
            step: None,
            default: Some(ParamValue::Text("#000000".into())),
            live: true,
            order: None,
            filter: None,
        }
    }

    fn meta() -> PluginMeta {
        let mut m = PluginMeta::minimal("x.js", "X", 1, vec![2]);
        m.lifecycle.shutdown = true;
        m
    }

    #[test]
    fn the_limiter_allows_one_save_per_interval_per_device() {
        let mut l = SaveLimiter::new(Duration::from_millis(80));
        assert!(l.take("a").is_ok());
        assert!(matches!(l.take("a"), Err(SaveRefused::TooSoon { .. })));
        assert!(l.check("a").is_err() && l.wait_secs("a") >= 1);
        assert!(l.take("b").is_ok(), "per device");
        std::thread::sleep(Duration::from_millis(100));
        assert!(l.take("a").is_ok());
        assert_eq!(SaveLimiter::default().interval, Duration::from_secs(60));
    }

    #[test]
    fn capabilities_and_choices() {
        let mut w = Vec::new();
        let caps = SaveCaps::from_json(
            &json!({"static": true, "effects": ["spectrum", "disco", "spectrum"]}),
            &mut w,
        )
        .unwrap();
        assert_eq!(caps.effects, vec!["spectrum"]);
        assert_eq!(w.len(), 1);
        assert!(caps.allows(&SaveChoice::default()));
        assert!(caps.allows(&SaveChoice {
            effect: Some("spectrum".into())
        }));
        assert!(!caps.allows(&SaveChoice {
            effect: Some("breathing".into())
        }));
        assert_eq!(SaveCaps::from_json(&json!({"static": false}), &mut w), None);
        assert_eq!(SaveCaps::from_json(&json!(true), &mut w), None);
    }

    #[test]
    fn unsupported_results_are_failures_with_the_reason() {
        let o = SaveOutcome::from_result("k", SaveAction::Preview, true, json!({"unsupported": "no flash"}));
        assert!(!o.ok);
        assert_eq!(o.error.as_deref(), Some("no flash"));
        let o = SaveOutcome::from_result("k", SaveAction::Save, false, json!({"zones": []}));
        assert!(o.ok && o.error.is_none());
    }

    #[test]
    fn shutdown_fallback_goes_through_shutdown_color() {
        let mut m = meta();
        assert_eq!(
            shutdown_fallback(&m, ShutdownMode::Restore, [1, 2, 3]),
            ShutdownPlan::Native
        );
        assert_eq!(
            shutdown_fallback(&m, ShutdownMode::Keep, [1, 2, 3]),
            ShutdownPlan::Unsupported
        );
        m.controllable_parameters.push(color_param("shutdownColor", &[]));
        let ShutdownPlan::Parameters(p) = shutdown_fallback(&m, ShutdownMode::Keep, [0x12, 0x34, 0xAB])
        else {
            panic!()
        };
        assert_eq!(
            p,
            json!({"shutdownColor": "#1234ab"}).as_object().unwrap().clone()
        );
        m.controllable_parameters
            .push(color_param("shutdownMode", &["SignalRGB", "Hardware"]));
        let ShutdownPlan::Parameters(p) = shutdown_fallback(&m, ShutdownMode::Off, [9, 9, 9]) else {
            panic!()
        };
        assert_eq!(
            p,
            json!({"shutdownColor": "#000000", "shutdownMode": "SignalRGB"})
                .as_object()
                .unwrap()
                .clone()
        );
        assert_eq!(supported_modes(&m), ShutdownMode::ALL.to_vec());
        m.shutdown_modes = vec![ShutdownMode::Off];
        assert_eq!(
            shutdown_fallback(&m, ShutdownMode::Off, [9, 9, 9]),
            ShutdownPlan::Native
        );
        m.lifecycle.shutdown = false;
        m.shutdown_modes.clear();
        assert_eq!(supported_modes(&m), vec![ShutdownMode::Restore]);
    }

    #[test]
    fn modes_parse_and_average() {
        let mut w = Vec::new();
        assert_eq!(
            shutdown_modes_from_json(&json!(["keep", "off", "keep", "nope"]), &mut w),
            vec![ShutdownMode::Keep, ShutdownMode::Off]
        );
        assert_eq!(w.len(), 1);
        assert_eq!(average(&[[0, 0, 0], [255, 100, 1]]), [128, 50, 1]);
        assert_eq!(average(&[]), [0, 0, 0]);
        assert_eq!(
            request_json(SaveAction::Preview, &SaveChoice::default(), &[[1, 2, 3]]),
            json!({"action": "preview", "effect": null, "colors": [[1, 2, 3]]})
        );
    }
}
