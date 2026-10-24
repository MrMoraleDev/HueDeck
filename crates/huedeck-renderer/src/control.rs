// SPDX-License-Identifier: GPL-3.0-or-later
//! Control commands for a serving renderer (`--serve`): one JSON object per line on stdin,
//! turned into JavaScript for the effect page. End of input means the daemon is gone.
//!
//! ```text
//! {"cmd":"set","values":{"speed":5,"color":"#ff0000"}}   assign, then on<name>Changed()
//! {"cmd":"event","sender":"discord","event":"message"}   onCanvasApiEvent({sender, event})
//! {"cmd":"tap","x":120,"y":40}                           onCanvasTapped(x, y)
//! {"cmd":"fps","value":15}                               paint rate
//! {"cmd":"engine","data":{"audio":{"level":-20}}}         merged into window.engine
//! {"cmd":"quit"}                                         close and exit
//! ```

use crate::inject::{call_hooks_js, is_injectable_global};
use huedeck_effects::{model::js_string_literal, EffectMeta};
use serde::Deserialize;
use serde_json::{Map, Value};

/// Longest control line read. A `set` with a few values is far below; an
/// `engine` push of the whole 160x100 zone image as a JSON array is about
/// 250 KB (M6 moves bulk input data to shared memory).
pub const MAX_LINE: usize = 512 * 1024;

/// A command from the daemon.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// New control values: assigned, then each control's `on<name>Changed()`.
    Set {
        /// Control name -> value.
        values: Map<String, Value>,
    },
    /// `onCanvasApiEvent({sender, event})` (SignalRGB's `POST /canvas/event`).
    Event {
        /// Who sent it.
        sender: String,
        /// What happened.
        event: String,
    },
    /// `onCanvasTapped(x, y)` in canvas pixels.
    Tap {
        /// X.
        x: f64,
        /// Y.
        y: f64,
    },
    /// Input data merged into `window.engine` (`audio`, `zone`, `vision`,
    /// `sensors` only); arrays fill the existing typed arrays in place.
    Engine {
        /// `{audio: {...}, zone: {...}, ...}`.
        data: Map<String, Value>,
    },
    /// Change the paint rate, frames per second.
    Fps {
        /// `1..=240`.
        value: u32,
    },
    /// Close the browser and exit.
    Quit,
}

/// Parse one line; blank lines are `Ok(None)`.
pub fn parse_line(line: &str) -> Result<Option<Command>, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    if line.len() > MAX_LINE {
        return Err(format!(
            "control line of {} bytes is longer than {MAX_LINE}",
            line.len()
        ));
    }
    let cmd: Command = serde_json::from_str(line).map_err(|e| format!("bad control line: {e}"))?;
    if let Command::Fps { value } = cmd {
        if !(1..=240).contains(&value) {
            return Err(format!("fps {value} is outside 1..=240"));
        }
    }
    Ok(Some(cmd))
}

/// JS assigning `values` to the effect's controls, then calling their change hooks.
/// Unknown controls, names that cannot be page globals and values of the wrong type
/// are skipped and returned.
pub fn set_script(meta: &EffectMeta, values: &Map<String, Value>) -> (String, Vec<String>) {
    let mut assigns = String::new();
    let mut hooks = Vec::new();
    let mut skipped = Vec::new();
    for (name, value) in values {
        let control = meta
            .properties
            .iter()
            .find(|c| &c.property == name)
            .filter(|c| is_injectable_global(&c.property));
        match control.and_then(|c| c.coerce(value).map(|v| (c, v))) {
            Some((c, v)) => {
                assigns.push_str(&format!("  {} = {};\n", c.property, v.js_literal()));
                hooks.push(c.change_hook());
            }
            None => skipped.push(name.clone()),
        }
    }
    (
        format!("(function () {{\n{assigns}{}}})();\n", call_hooks_js(&hooks)),
        skipped,
    )
}

/// Top-level `window.engine` members an `engine` command may update.
pub const ENGINE_KEYS: &[&str] = &["audio", "zone", "vision", "sensors"];

/// Merges its argument into `window.engine` (see [`engine_script`]). Own keys
/// only, `__proto__` never assigned.
const ENGINE_MERGE_JS: &str = r#"(function (d) {
  var e = window.engine;
  if (!e) return;
  function merge(dst, src) {
    Object.keys(src).forEach(function (k) {
      if (k === "__proto__") return;
      var v = src[k], cur = dst[k];
      if (cur && ArrayBuffer.isView(cur) && Array.isArray(v)) { cur.set(v.slice(0, cur.length)); }
      else if (cur && ArrayBuffer.isView(cur) && typeof v === "string" && v.lastIndexOf("base64:", 0) === 0) {
        var b = atob(v.slice(7)), n = Math.min(b.length, cur.length);
        for (var i = 0; i < n; i++) cur[i] = b.charCodeAt(i);
      }
      else if (cur && typeof cur === "object" && !ArrayBuffer.isView(cur) && v && typeof v === "object" && !Array.isArray(v)) { merge(cur, v); }
      else { dst[k] = v; }
    });
  }
  merge(e, d);
})"#;

/// JS merging `data` into `window.engine`: objects merge recursively, arrays
/// fill the existing typed arrays in place (effects keep references to them),
/// a `"base64:…"` string fills a typed array with its decoded bytes (the
/// 64,000-byte `zone.imagedata`), other values replace. Keys outside [`ENGINE_KEYS`] are dropped and returned.
pub fn engine_script(data: &Map<String, Value>) -> (String, Vec<String>) {
    let mut kept = Map::new();
    let mut skipped = Vec::new();
    for (k, v) in data {
        if ENGINE_KEYS.contains(&k.as_str()) {
            kept.insert(k.clone(), v.clone());
        } else {
            skipped.push(k.clone());
        }
    }
    (format!("{ENGINE_MERGE_JS}({});\n", Value::Object(kept)), skipped)
}

/// JS for `onCanvasApiEvent({sender, event})`.
pub fn event_script(sender: &str, event: &str) -> String {
    format!(
        "if (typeof onCanvasApiEvent === \"function\") {{ try {{ onCanvasApiEvent({{ sender: {}, event: {} }}); }} \
         catch (err) {{ console.error(\"huedeck: onCanvasApiEvent threw: \" + err); }} }}\n",
        js_string_literal(sender),
        js_string_literal(event)
    )
}

/// JS for `onCanvasTapped(x, y)` (non-finite coordinates become 0).
pub fn tap_script(x: f64, y: f64) -> String {
    let (x, y) = (
        if x.is_finite() { x } else { 0.0 },
        if y.is_finite() { y } else { 0.0 },
    );
    format!(
        "if (typeof onCanvasTapped === \"function\") {{ try {{ onCanvasTapped({x}, {y}); }} \
         catch (err) {{ console.error(\"huedeck: onCanvasTapped threw: \" + err); }} }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> EffectMeta {
        huedeck_effects::parse_effect_meta(
            r##"<head><meta property="speed" label="S" type="number" default="1"/>
            <meta property="tint" label="T" type="color" default="#000000"/>
            <meta property="default" label="D" type="number" default="1"/></head>"##,
        )
    }

    #[test]
    fn commands_parse() {
        assert_eq!(parse_line("  ").unwrap(), None);
        assert_eq!(parse_line(r#"{"cmd":"quit"}"#).unwrap(), Some(Command::Quit));
        assert_eq!(
            parse_line(r#"{"cmd":"fps","value":15}"#).unwrap(),
            Some(Command::Fps { value: 15 })
        );
        assert!(parse_line(r#"{"cmd":"fps","value":0}"#).is_err());
        assert!(parse_line(r#"{"cmd":"nope"}"#).is_err());
        assert!(parse_line("not json").is_err());
        assert!(parse_line(&format!(
            "{{\"cmd\":\"quit\",\"x\":\"{}\"}}",
            "a".repeat(MAX_LINE)
        ))
        .is_err());
        assert!(matches!(
            parse_line(r#"{"cmd":"set","values":{"speed":3}}"#).unwrap(),
            Some(Command::Set { .. })
        ));
    }

    #[test]
    fn set_assigns_known_controls_then_calls_their_hooks() {
        let mut values = Map::new();
        values.insert("speed".into(), serde_json::json!(4));
        values.insert("tint".into(), serde_json::json!("#00ff00"));
        values.insert("default".into(), serde_json::json!(1));
        values.insert("missing".into(), serde_json::json!(1));
        values.insert("speed2".into(), serde_json::json!("x"));
        let (js, skipped) = set_script(&meta(), &values);
        assert_eq!(skipped, vec!["default", "missing", "speed2"]);
        assert!(js.contains("speed = 4;") && js.contains("tint = \"#00ff00\";"));
        assert!(js.find("speed = 4;").unwrap() < js.find("\"onspeedChanged\"").unwrap());
        let mut bad = Map::new();
        bad.insert("speed".into(), serde_json::json!("fast"));
        assert_eq!(set_script(&meta(), &bad).1, vec!["speed"]);
    }

    #[test]
    fn engine_data_is_restricted_to_input_members() {
        let mut data = Map::new();
        data.insert(
            "audio".into(),
            serde_json::json!({"level": -20, "freq": [1, 2, 3]}),
        );
        data.insert("getSensorValue".into(), serde_json::json!(1));
        let (js, skipped) = engine_script(&data);
        assert_eq!(skipped, vec!["getSensorValue"]);
        assert!(js.contains("\"level\":-20") && js.contains("cur.set("), "{js}");
        assert!(!js.contains("getSensorValue"));
        assert!(js.contains(r#"v.lastIndexOf("base64:", 0) === 0"#));
        assert!(matches!(
            parse_line(r#"{"cmd":"engine","data":{"zone":{"hue":[1]}}}"#).unwrap(),
            Some(Command::Engine { .. })
        ));
    }

    #[test]
    fn events_are_quoted() {
        let js = event_script("a\"b", "c");
        assert!(js.contains(r#"sender: "a\"b""#), "{js}");
        assert!(tap_script(f64::NAN, 3.5).contains("onCanvasTapped(0, 3.5)"));
    }
}
