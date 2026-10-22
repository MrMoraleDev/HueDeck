// SPDX-License-Identifier: GPL-3.0-or-later
//! Effect metadata model.
//!
//! The serde shape mirrors `schemas/effect-meta.schema.json` one to one:
//! `properties[]` (`property, label, type, default, min, max, step, values,
//! tooltip, group`), `meters[]` (`name, type, region, match, hdrMatch, string,
//! confidence, brightness, contrast, authoringResolution, resolutions[]`) and
//! the `uses` capability flags. [`EffectMeta::warnings`] is diagnostic only
//! and never serialised (the schema forbids extra keys).

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// `<meta property … type="…">` control types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlType {
    /// Numeric slider.
    Number,
    /// Hue `0..=360`.
    Hue,
    /// Checkbox.
    Boolean,
    /// `#RRGGBB` colour.
    Color,
    /// Free text.
    Textfield,
    /// One of `values` (`type="combobox"`; the docs' `list` is accepted as an alias).
    Combobox,
    /// Sensor display name (LCD faces).
    Sensor,
}

impl FromStr for ControlType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "number" => Ok(ControlType::Number),
            "hue" => Ok(ControlType::Hue),
            "boolean" => Ok(ControlType::Boolean),
            "color" => Ok(ControlType::Color),
            "textfield" => Ok(ControlType::Textfield),
            "combobox" | "list" => Ok(ControlType::Combobox),
            "sensor" => Ok(ControlType::Sensor),
            other => Err(format!("unknown control type \"{other}\"")),
        }
    }
}

impl ControlType {
    /// Whether values of this type are injected as JS numbers.
    pub fn is_numeric(self) -> bool {
        matches!(self, ControlType::Number | ControlType::Hue)
    }
}

/// A typed control value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ControlValue {
    /// number / hue.
    Number(f64),
    /// boolean.
    Bool(bool),
    /// color / textfield / combobox / sensor.
    Text(String),
}

impl ControlValue {
    /// JS literal for `var <prop> = <literal>;`.
    pub fn js_literal(&self) -> String {
        match self {
            ControlValue::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            ControlValue::Bool(b) => b.to_string(),
            ControlValue::Text(s) => js_string_literal(s),
        }
    }
}

/// Quote `s` as a double-quoted JS string literal.
pub fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '<' => out.push_str("\\x3c"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Deserialiser for the schema's `numberish` (number | numeric string → `Option<f64>`).
mod numberish {
    use serde::{Deserialize, Deserializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Num(f64),
        Str(String),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        Ok(match Option::<Raw>::deserialize(d)? {
            None => None,
            Some(Raw::Num(n)) => Some(n),
            Some(Raw::Str(s)) => s.trim().parse::<f64>().ok().filter(|v| v.is_finite()),
        })
    }
}

/// One `<meta property="…">` control (schema `property`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlMeta {
    /// JS global name (also the persistence key).
    pub property: String,
    /// UI label (defaults to `property` when absent).
    pub label: String,
    /// Control type.
    #[serde(rename = "type")]
    pub control_type: ControlType,
    /// Raw `default` attribute as written in HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// `min` (schema `numberish`: a number or a numeric string is accepted on input).
    #[serde(
        default,
        deserialize_with = "numberish::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub min: Option<f64>,
    /// `max`.
    #[serde(
        default,
        deserialize_with = "numberish::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub max: Option<f64>,
    /// `step`.
    #[serde(
        default,
        deserialize_with = "numberish::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub step: Option<f64>,
    /// `values`, comma-separated in HTML.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// `tooltip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    /// `group` (UI section).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

impl ControlMeta {
    /// Typed default: numbers parsed (0 when unparsable), booleans accept
    /// `true/false/1/0`, everything else is text. Missing defaults become
    /// `0`, `false`, `"#000000"` or `""` / first combobox value.
    pub fn default_value(&self) -> ControlValue {
        let raw = self.default.as_deref().map(str::trim);
        match self.control_type {
            ControlType::Number | ControlType::Hue => ControlValue::Number(
                raw.and_then(|s| s.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .unwrap_or(0.0),
            ),
            ControlType::Boolean => ControlValue::Bool(matches!(
                raw.map(|s| s.to_ascii_lowercase()).as_deref(),
                Some("true") | Some("1")
            )),
            ControlType::Color => ControlValue::Text(raw.unwrap_or("#000000").to_string()),
            ControlType::Combobox => ControlValue::Text(
                raw.map(str::to_string)
                    .or_else(|| self.values.first().cloned())
                    .unwrap_or_default(),
            ),
            ControlType::Textfield | ControlType::Sensor => ControlValue::Text(raw.unwrap_or("").to_string()),
        }
    }

    /// A value set from outside (UI, CLI `--prop`, daemon) as this control's
    /// type: numbers from numbers or numeric strings, booleans from booleans,
    /// `0`/`1` or `"true"`/`"false"`, everything else as text (non-string JSON
    /// as its JSON text). `None` when it cannot be a number or boolean.
    pub fn coerce(&self, value: &serde_json::Value) -> Option<ControlValue> {
        use serde_json::Value;
        match self.control_type {
            ControlType::Number | ControlType::Hue => match value {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => s.trim().parse::<f64>().ok(),
                _ => None,
            }
            .filter(|v| v.is_finite())
            .map(ControlValue::Number),
            ControlType::Boolean => match value {
                Value::Bool(b) => Some(*b),
                Value::Number(n) => n.as_f64().map(|v| v != 0.0),
                Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" => Some(true),
                    "false" | "0" => Some(false),
                    _ => None,
                },
                _ => None,
            }
            .map(ControlValue::Bool),
            _ => Some(ControlValue::Text(match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })),
        }
    }

    /// `var <property> = <literal>;` for `value` (or the default when `None`).
    pub fn js_initializer(&self, value: Option<&ControlValue>) -> String {
        let v = value.cloned().unwrap_or_else(|| self.default_value());
        format!("var {} = {};", self.property, v.js_literal())
    }

    /// Name of the change hook the engine calls: `on<property>Changed`.
    pub fn change_hook(&self) -> String {
        format!("on{}Changed", self.property)
    }
}

/// `<meta meter … type="…">` kinds (schema `meter.type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeterType {
    /// Fill fraction `0..=1` along the long axis of the region.
    Linear,
    /// Fraction of matching pixels `0..=1`.
    Area,
    /// OCR text match (1 / 0 / -1).
    OcrTextmatch,
    /// OCR numeric read.
    OcrNumeric,
    /// Generic OCR text (engine-defined).
    Ocr,
    /// Mean colour `[h, s, l]`.
    Colormean,
}

impl FromStr for MeterType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "linear" => Ok(MeterType::Linear),
            "area" => Ok(MeterType::Area),
            "ocr_textmatch" => Ok(MeterType::OcrTextmatch),
            "ocr_numeric" => Ok(MeterType::OcrNumeric),
            "ocr" => Ok(MeterType::Ocr),
            "colormean" => Ok(MeterType::Colormean),
            other => Err(format!("unknown meter type \"{other}\"")),
        }
    }
}

/// Inclusive numeric range written `lo-hi` in HTML (schema `range`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Range {
    /// Lower bound.
    pub lo: f64,
    /// Upper bound.
    pub hi: f64,
}

impl FromStr for Range {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        // Split on the last '-' that is not the leading sign (char-aware: the
        // attribute text is untrusted and may start with a multi-byte char).
        let idx = s
            .char_indices()
            .skip(1)
            .filter(|(_, c)| *c == '-')
            .map(|(i, _)| i)
            .last()
            .ok_or_else(|| format!("range \"{s}\" needs lo-hi"))?;
        let lo = s[..idx]
            .trim()
            .parse::<f64>()
            .map_err(|e| format!("range lo: {e}"))?;
        let hi = s[idx + 1..]
            .trim()
            .parse::<f64>()
            .map_err(|e| format!("range hi: {e}"))?;
        Ok(Range { lo, hi })
    }
}

/// Normalised rectangle on the captured display (schema `region`), all in `0..=1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Horizontal extent.
    pub width: f64,
    /// Vertical extent; optional for linear meters (a thin band).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

/// Colour match window (schema `hslRanges`): hue `0..=360`, saturation and lightness `0..=100`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HslRanges {
    /// Hue window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<Range>,
    /// Saturation window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<Range>,
    /// Lightness window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l: Option<Range>,
}

impl HslRanges {
    /// Whether no window is set.
    pub fn is_empty(&self) -> bool {
        self.h.is_none() && self.s.is_none() && self.l.is_none()
    }
}

/// A display size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// `<resolution size="WxH" | aspect="W:H" x y width height>` child of a meter
/// (schema `resolutionOverride`): replaces the meter's region on matching displays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolutionOverride {
    /// Display size this override applies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<DisplaySize>,
    /// Alternative selector: display aspect ratio such as `16:9`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect: Option<String>,
    /// Replacement region.
    pub region: Region,
}

/// One screen meter exposed as `engine.vision.<name>` (schema `meter`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterMeta {
    /// `meter` attribute.
    pub name: String,
    /// `type`.
    #[serde(rename = "type")]
    pub meter_type: MeterType,
    /// Normalised region (`x`, `y`, `width`, `height` attributes).
    pub region: Region,
    /// SDR colour window (`h`, `s`, `l` attributes).
    #[serde(rename = "match", default, skip_serializing_if = "Option::is_none")]
    pub color_match: Option<HslRanges>,
    /// HDR colour window (`hdr-h`, `hdr-s`, `hdr-l` attributes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr_match: Option<HslRanges>,
    /// OCR match string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string: Option<String>,
    /// OCR confidence threshold (percent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// OCR pre-processing brightness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f64>,
    /// OCR pre-processing contrast.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f64>,
    /// Display size the base region was authored against (default 2560x1440 when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring_resolution: Option<DisplaySize>,
    /// Per-display-size region overrides.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolutions: Vec<ResolutionOverride>,
}

/// Capability flags detected from the page (schema `uses`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Uses {
    /// References `engine.audio`.
    pub audio: bool,
    /// References `engine.zone` (screen capture).
    pub video: bool,
    /// Declares meters or references `engine.vision`.
    pub meters: bool,
    /// References any `engine.*` global (sensors, now-playing, …).
    pub engine: bool,
    /// Requests a WebGL context.
    pub webgl: bool,
    /// Implements `onCanvasTapped` / `onKeyPressed` / `onCanvasApiEvent`.
    pub input: bool,
}

impl Uses {
    /// Whether every flag is off.
    pub fn is_empty(&self) -> bool {
        *self == Uses::default()
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Parsed effect metadata (schema root).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectMeta {
    /// Stable effect id (bundled: file name). Empty until the caller sets it.
    pub id: String,
    /// `<title>`.
    pub title: String,
    /// `<meta description>`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// `<meta publisher>`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    /// `<meta background="true">` (LCD faces drawn over a background layer).
    #[serde(skip_serializing_if = "is_false")]
    pub background: bool,
    /// Controls in document order.
    pub properties: Vec<ControlMeta>,
    /// Meters in document order.
    pub meters: Vec<MeterMeta>,
    /// Capability flags.
    #[serde(skip_serializing_if = "Uses::is_empty")]
    pub uses: Uses,
    /// Non-fatal problems found while parsing (malformed meters, unknown
    /// types). Diagnostic only; never serialised.
    #[serde(skip)]
    pub warnings: Vec<String>,
}

impl EffectMeta {
    /// Set the persistence id (builder style).
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Control by property name.
    pub fn control(&self, property: &str) -> Option<&ControlMeta> {
        self.properties.iter().find(|c| c.property == property)
    }

    /// The `var … ;` block the engine evaluates after the page scripts, one line per control.
    pub fn js_initializers(&self) -> String {
        let mut out = String::new();
        for c in &self.properties {
            out.push_str(&c.js_initializer(None));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_values_take_the_control_type() {
        use serde_json::json;
        let html = r##"<head><meta property="speed" label="S" type="number" default="1"/>
            <meta property="on" label="O" type="boolean" default="0"/>
            <meta property="tint" label="T" type="color" default="#000000"/></head>"##;
        let meta = crate::parse_effect_meta(html);
        let (speed, on, tint) = (&meta.properties[0], &meta.properties[1], &meta.properties[2]);
        assert_eq!(speed.coerce(&json!(5)), Some(ControlValue::Number(5.0)));
        assert_eq!(speed.coerce(&json!(" 2.5 ")), Some(ControlValue::Number(2.5)));
        assert_eq!(speed.coerce(&json!("fast")), None);
        assert_eq!(on.coerce(&json!("1")), Some(ControlValue::Bool(true)));
        assert_eq!(on.coerce(&json!(0)), Some(ControlValue::Bool(false)));
        assert_eq!(on.coerce(&json!("maybe")), None);
        assert_eq!(
            tint.coerce(&json!("#ff0000")),
            Some(ControlValue::Text("#ff0000".into()))
        );
        assert_eq!(tint.coerce(&json!(7)), Some(ControlValue::Text("7".into())));
    }

    #[test]
    fn js_literals_are_safe() {
        assert_eq!(ControlValue::Number(3.0).js_literal(), "3");
        assert_eq!(ControlValue::Number(2.5).js_literal(), "2.5");
        assert_eq!(ControlValue::Bool(true).js_literal(), "true");
        assert_eq!(ControlValue::Text("#009bde".into()).js_literal(), "\"#009bde\"");
        assert_eq!(
            js_string_literal("a\"b\\c\n</script>"),
            "\"a\\\"b\\\\c\\n\\x3c/script>\""
        );
    }

    #[test]
    fn range_parsing() {
        assert_eq!("0-20".parse::<Range>().unwrap(), Range { lo: 0.0, hi: 20.0 });
        assert_eq!(
            " 60 - 100 ".parse::<Range>().unwrap(),
            Range { lo: 60.0, hi: 100.0 }
        );
        assert_eq!("-5-5".parse::<Range>().unwrap(), Range { lo: -5.0, hi: 5.0 });
        assert!("abc".parse::<Range>().is_err());
        assert!("".parse::<Range>().is_err());
        assert!("-".parse::<Range>().is_err());
        // Multi-byte first char must not panic.
        assert!("é-5".parse::<Range>().is_err());
    }

    #[test]
    fn typed_defaults() {
        let mk = |t: ControlType, d: Option<&str>| ControlMeta {
            property: "p".into(),
            label: "P".into(),
            control_type: t,
            default: d.map(str::to_string),
            min: None,
            max: None,
            step: None,
            values: vec!["Standard".into(), "HD".into()],
            tooltip: None,
            group: None,
        };
        assert_eq!(
            mk(ControlType::Number, Some("3")).default_value(),
            ControlValue::Number(3.0)
        );
        assert_eq!(
            mk(ControlType::Number, Some("x")).default_value(),
            ControlValue::Number(0.0)
        );
        assert_eq!(
            mk(ControlType::Boolean, Some("1")).default_value(),
            ControlValue::Bool(true)
        );
        assert_eq!(
            mk(ControlType::Boolean, Some("false")).default_value(),
            ControlValue::Bool(false)
        );
        assert_eq!(
            mk(ControlType::Boolean, None).default_value(),
            ControlValue::Bool(false)
        );
        assert_eq!(
            mk(ControlType::Color, None).default_value(),
            ControlValue::Text("#000000".into())
        );
        assert_eq!(
            mk(ControlType::Combobox, None).default_value(),
            ControlValue::Text("Standard".into())
        );
        assert_eq!(
            mk(ControlType::Number, Some("3")).js_initializer(None),
            "var p = 3;"
        );
        assert_eq!(mk(ControlType::Number, Some("3")).change_hook(), "onpChanged");
    }

    #[test]
    fn meter_type_names_match_the_schema_enum() {
        for (t, name) in [
            (MeterType::Linear, "linear"),
            (MeterType::Area, "area"),
            (MeterType::OcrTextmatch, "ocr_textmatch"),
            (MeterType::OcrNumeric, "ocr_numeric"),
            (MeterType::Ocr, "ocr"),
            (MeterType::Colormean, "colormean"),
        ] {
            assert_eq!(serde_json::to_value(t).unwrap(), name);
            assert_eq!(name.parse::<MeterType>().unwrap(), t);
        }
        assert_eq!(serde_json::to_value(ControlType::Textfield).unwrap(), "textfield");
    }
}
