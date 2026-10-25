// SPDX-License-Identifier: GPL-3.0-or-later
//! A macro script's `this.Options` (docs/host-api/macros-system.md §1.2):
//! descriptors `{property, label, type, default, values, filter, min, max,
//! step, description}` and the values written onto the instance as
//! `this.<property>` before every call.
//!
//! Values are coerced strictly when the user sets them ([`coerce`]: a number
//! outside `min..max`, a combobox key it does not list, a colour that is not
//! `#rrggbb[aa]` is refused) and leniently for the script's own defaults
//! (`"true"` / `"false"` strings become booleans). Combobox values are stored
//! by `key`. An unknown type is ignored with a warning, as SignalRGB does.

use serde::Serialize;
use serde_json::{Map, Value};

/// Every option type the runtime knows.
pub const TYPES: &[&str] = &[
    "boolean",
    "number",
    "textfield",
    "combobox",
    "keycode",
    "mousecode",
    "effectname",
    "layoutname",
    "effectpreset",
    "device",
    "playlist",
    "sensor",
    "color",
    "hue",
    "list",
];

/// One combobox entry.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Choice {
    /// Stored value.
    pub key: String,
    /// Shown text.
    pub label: String,
}

/// One option descriptor.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionSpec {
    /// `this.<property>`.
    pub property: String,
    /// Shown name.
    pub label: String,
    /// One of [`TYPES`].
    #[serde(rename = "type")]
    pub kind: String,
    /// The coerced default.
    pub default: Value,
    /// Combobox entries.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<Choice>,
    /// Textfield pattern (a regular expression the dashboard checks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// Number minimum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Number maximum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Number step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// Help text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Most options one script may declare.
pub const MAX_OPTIONS: usize = 64;
/// Longest string value kept.
pub const MAX_TEXT: usize = 4096;

fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|f: &f64| f.is_finite())
}

/// Parse `this.Options` (as JSON); unusable descriptors become warnings.
pub fn parse(options: &Value, warnings: &mut Vec<String>) -> Vec<OptionSpec> {
    let Some(list) = options.as_array() else {
        if !options.is_null() {
            warnings.push("Options is not an array (ignored)".into());
        }
        return Vec::new();
    };
    let mut out: Vec<OptionSpec> = Vec::new();
    for d in list.iter().take(MAX_OPTIONS) {
        let Some(property) = text(d.get("property")).filter(|p| !p.is_empty()) else {
            warnings.push("an option without a property (ignored)".into());
            continue;
        };
        let kind = text(d.get("type")).unwrap_or_default().to_ascii_lowercase();
        if !TYPES.contains(&kind.as_str()) {
            warnings.push(format!(
                "Unknown Property Type: [{kind}] for property [{property}]. Property will be ignored."
            ));
            continue;
        }
        if out.iter().any(|o| o.property == property) {
            warnings.push(format!("option {property} declared twice (the first one counts)"));
            continue;
        }
        let values = d
            .get("values")
            .and_then(Value::as_array)
            .map(|vs| {
                vs.iter()
                    .filter_map(|v| match v {
                        Value::Object(o) => {
                            let key = text(o.get("key"))?;
                            let label = text(o.get("label")).unwrap_or_else(|| key.clone());
                            Some(Choice { key, label })
                        }
                        other => text(Some(other)).map(|k| Choice {
                            key: k.clone(),
                            label: k,
                        }),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut spec = OptionSpec {
            label: text(d.get("label")).unwrap_or_else(|| property.clone()),
            property,
            kind,
            default: Value::Null,
            values,
            filter: text(d.get("filter")),
            min: num(d.get("min")),
            max: num(d.get("max")),
            step: num(d.get("step")),
            description: text(d.get("description")),
        };
        spec.default = d
            .get("default")
            .and_then(|v| lenient(&spec, v))
            .unwrap_or_else(|| empty(&spec));
        out.push(spec);
    }
    out
}

/// The value of a type when nothing is set.
fn empty(spec: &OptionSpec) -> Value {
    match spec.kind.as_str() {
        "boolean" => Value::Bool(false),
        "number" | "hue" => number(spec.min.unwrap_or(0.0)),
        "combobox" => spec
            .values
            .first()
            .map_or(Value::String(String::new()), |c| Value::String(c.key.clone())),
        "keycode" | "mousecode" => Value::String("None".into()),
        "effectpreset" => serde_json::json!({ "effect": "", "preset": "" }),
        "list" => Value::Array(Vec::new()),
        "color" => Value::String("#ff0000".into()),
        _ => Value::String(String::new()),
    }
}

/// A script default: like [`coerce`], but `None` (use the empty value) where strict coercion fails.
fn lenient(spec: &OptionSpec, v: &Value) -> Option<Value> {
    coerce(spec, v).ok().or_else(|| match spec.kind.as_str() {
        // A combobox default not among its values is still what the script asked for.
        "combobox" => text(Some(v)).map(Value::String),
        "number" | "hue" => num(Some(v)).map(number),
        _ => None,
    })
}

/// A whole number as an integer, else a float.
fn number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

fn is_colour(s: &str) -> bool {
    (s.len() == 7 || s.len() == 9) && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// `value` as the value of option `spec`, strictly (the dashboard's input).
pub fn coerce(spec: &OptionSpec, value: &Value) -> Result<Value, String> {
    let p = &spec.property;
    let string = || -> Result<String, String> {
        let s = text(Some(value)).ok_or_else(|| format!("{p} must be text"))?;
        if s.len() > MAX_TEXT {
            return Err(format!("{p} is longer than {MAX_TEXT} bytes"));
        }
        Ok(s)
    };
    match spec.kind.as_str() {
        "boolean" => match value {
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::String(s) if s.eq_ignore_ascii_case("true") => Ok(Value::Bool(true)),
            Value::String(s) if s.eq_ignore_ascii_case("false") => Ok(Value::Bool(false)),
            _ => Err(format!("{p} must be true or false")),
        },
        "number" | "hue" => {
            let n = num(Some(value)).ok_or_else(|| format!("{p} must be a number"))?;
            let (lo, hi) = if spec.kind == "hue" {
                (Some(0.0), Some(360.0))
            } else {
                (spec.min, spec.max)
            };
            if lo.is_some_and(|lo| n < lo) || hi.is_some_and(|hi| n > hi) {
                return Err(format!(
                    "{p} must be within {}..{} (got {n})",
                    lo.map_or(String::new(), |x| x.to_string()),
                    hi.map_or(String::new(), |x| x.to_string())
                ));
            }
            Ok(number(n))
        }
        "combobox" => {
            let s = string()?;
            if spec.values.is_empty() || spec.values.iter().any(|c| c.key == s) {
                Ok(Value::String(s))
            } else {
                Err(format!(
                    "{p} must be one of {}",
                    spec.values
                        .iter()
                        .map(|c| c.key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        "color" => {
            let s = string()?;
            if is_colour(&s) {
                Ok(Value::String(s.to_ascii_lowercase()))
            } else {
                Err(format!("{p} must be a colour #rrggbb"))
            }
        }
        "keycode" => {
            let s = string()?;
            if s == "None" || s.is_empty() || crate::keys::combo(&s).is_ok() {
                Ok(Value::String(s))
            } else {
                Err(format!("{p}: unknown key combination {s:?}"))
            }
        }
        "mousecode" => {
            let s = string()?;
            if s == "None" || s.is_empty() || crate::keys::click(&s).is_ok() {
                Ok(Value::String(s))
            } else {
                Err(format!("{p}: unknown mouse button {s:?}"))
            }
        }
        "effectpreset" => {
            let o = value
                .as_object()
                .ok_or_else(|| format!("{p} must be {{effect, preset}}"))?;
            let field = |k: &str| text(o.get(k)).unwrap_or_default();
            Ok(serde_json::json!({ "effect": field("effect"), "preset": field("preset") }))
        }
        "list" => match value {
            Value::Array(a) if a.len() <= MAX_OPTIONS * 4 => Ok(value.clone()),
            _ => Err(format!("{p} must be a list")),
        },
        // textfield, effectname, layoutname, device, playlist, sensor
        _ => string().map(Value::String),
    }
}

/// The values a script instance gets: each option's stored value when it
/// still fits the descriptor, else its default.
pub fn effective(specs: &[OptionSpec], stored: &Map<String, Value>) -> Map<String, Value> {
    specs
        .iter()
        .map(|s| {
            let v = stored
                .get(&s.property)
                .and_then(|v| coerce(s, v).ok())
                .unwrap_or_else(|| s.default.clone());
            (s.property.clone(), v)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn specs(v: Value) -> (Vec<OptionSpec>, Vec<String>) {
        let mut w = Vec::new();
        (parse(&v, &mut w), w)
    }

    #[test]
    fn descriptors_parse_with_defaults_coerced() {
        let (s, w) = specs(json!([
            {"property": "Flag", "label": "Flag", "type": "boolean", "default": "true"},
            {"property": "Level", "label": "Level", "type": "number", "min": 0, "max": 100, "default": 100},
            {"property": "Mode", "label": "Mode", "type": "combobox", "values": [{"key": "a", "label": "A"}, {"key": "b", "label": "B"}], "default": "b"},
            {"property": "Dir", "label": "Dir", "type": "combobox", "values": ["Next", "Previous"], "default": "Next"},
            {"property": "Text", "label": "Text", "type": "textfield", "filter": "^\\d+$", "default": "5"},
            {"property": "Preset", "label": "Preset", "type": "effectpreset"},
            {"property": "Odd", "label": "Odd", "type": "weird"},
            {"label": "No property", "type": "boolean"},
            {"property": "Flag", "label": "Again", "type": "boolean"}
        ]));
        assert_eq!(s.len(), 6);
        assert_eq!(w.len(), 3, "{w:?}");
        assert_eq!(s[0].default, json!(true));
        assert_eq!(s[1].default, json!(100));
        assert_eq!(s[2].default, json!("b"));
        assert_eq!(
            s[3].values[1],
            Choice {
                key: "Previous".into(),
                label: "Previous".into()
            }
        );
        assert_eq!(s[4].filter.as_deref(), Some("^\\d+$"));
        assert_eq!(s[5].default, json!({"effect": "", "preset": ""}));
        assert!(w[0].contains("Unknown Property Type: [weird]"));
    }

    #[test]
    fn values_are_coerced_strictly() {
        let (s, _) = specs(json!([
            {"property": "Flag", "label": "F", "type": "boolean", "default": false},
            {"property": "Level", "label": "L", "type": "number", "min": 0, "max": 100},
            {"property": "Mode", "label": "M", "type": "combobox", "values": [{"key": "a", "label": "A"}]},
            {"property": "Tint", "label": "T", "type": "color"},
            {"property": "Combo", "label": "K", "type": "keycode", "default": "None"},
            {"property": "Btn", "label": "B", "type": "mousecode", "default": "None"}
        ]));
        assert_eq!(coerce(&s[0], &json!("TRUE")), Ok(json!(true)));
        assert!(coerce(&s[0], &json!(1)).is_err());
        assert_eq!(coerce(&s[1], &json!("42")), Ok(json!(42)));
        assert!(coerce(&s[1], &json!(101)).is_err());
        assert!(coerce(&s[2], &json!("z")).is_err());
        assert_eq!(coerce(&s[3], &json!("#FF00AA")), Ok(json!("#ff00aa")));
        assert!(coerce(&s[3], &json!("red")).is_err());
        assert_eq!(
            coerce(&s[4], &json!("Ctrl + Shift + F1")),
            Ok(json!("Ctrl + Shift + F1"))
        );
        assert!(coerce(&s[4], &json!("Ctrl + Nope")).is_err());
        assert!(coerce(&s[5], &json!("Right Click")).is_ok());
        let mut stored = Map::new();
        stored.insert("Level".into(), json!(500));
        stored.insert("Flag".into(), json!(true));
        let e = effective(&s, &stored);
        assert_eq!(
            e["Level"],
            json!(0),
            "an out-of-range stored value falls back to the default"
        );
        assert_eq!(e["Flag"], json!(true));
    }
}
