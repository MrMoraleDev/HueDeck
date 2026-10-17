// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `<title>` / `<meta>` extraction with `scraper` (html5ever).

use crate::model::{
    ControlMeta, ControlType, DisplaySize, EffectMeta, HslRanges, MeterMeta, MeterType, Range, Region,
    ResolutionOverride, Uses,
};
use scraper::{ElementRef, Html, Selector};

fn attr<'a>(el: &ElementRef<'a>, name: &str) -> Option<&'a str> {
    el.value().attr(name).map(str::trim)
}

fn attr_f64(el: &ElementRef<'_>, name: &str) -> Option<f64> {
    attr(el, name).and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn attr_range(el: &ElementRef<'_>, name: &str, warnings: &mut Vec<String>, meter: &str) -> Option<Range> {
    let raw = attr(el, name)?;
    match raw.parse::<Range>() {
        Ok(r) => Some(r),
        Err(e) => {
            warnings.push(format!("meter \"{meter}\": attribute {name}=\"{raw}\" ignored: {e}"));
            None
        }
    }
}

fn parse_control(el: &ElementRef<'_>, property: &str, warnings: &mut Vec<String>) -> Option<ControlMeta> {
    let type_raw = attr(el, "type").unwrap_or("");
    let control_type = match type_raw.parse::<ControlType>() {
        Ok(t) => t,
        Err(e) => {
            warnings.push(format!("property \"{property}\": {e}; control skipped"));
            return None;
        }
    };
    let values = attr(el, "values")
        .map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    Some(ControlMeta {
        property: property.to_string(),
        label: attr(el, "label").filter(|s| !s.is_empty()).unwrap_or(property).to_string(),
        control_type,
        default: attr(el, "default").map(str::to_string),
        min: attr_f64(el, "min"),
        max: attr_f64(el, "max"),
        step: attr_f64(el, "step"),
        values,
        tooltip: attr(el, "tooltip").map(str::to_string),
        group: attr(el, "group").filter(|s| !s.is_empty()).map(str::to_string),
    })
}

fn unit(v: Option<f64>) -> Option<f64> {
    v.filter(|v| (0.0..=1.0).contains(v))
}

/// `x`/`y`/`width` are required and must lie in `0..=1`; `height` is optional.
fn parse_region(el: &ElementRef<'_>) -> Option<Region> {
    let (x, y, width) = (unit(attr_f64(el, "x"))?, unit(attr_f64(el, "y"))?, unit(attr_f64(el, "width"))?);
    Some(Region { x, y, width, height: unit(attr_f64(el, "height")) })
}

fn hsl(el: &ElementRef<'_>, keys: [&str; 3], warnings: &mut Vec<String>, meter: &str) -> Option<HslRanges> {
    let r = HslRanges {
        h: attr_range(el, keys[0], warnings, meter),
        s: attr_range(el, keys[1], warnings, meter),
        l: attr_range(el, keys[2], warnings, meter),
    };
    (!r.is_empty()).then_some(r)
}

fn parse_meter(el: &ElementRef<'_>, name: &str, warnings: &mut Vec<String>) -> Option<MeterMeta> {
    let Some(type_raw) = attr(el, "type") else {
        warnings.push(format!("meter \"{name}\": Type not specified; Meter will be ignored"));
        return None;
    };
    let meter_type = match type_raw.parse::<MeterType>() {
        Ok(t) => t,
        Err(e) => {
            warnings.push(format!("meter \"{name}\": {e}; Meter will be ignored"));
            return None;
        }
    };
    let Some(region) = parse_region(el) else {
        warnings.push(format!("meter \"{name}\": Invalid X/Y/Width (allowed range for each is [0-1]); Meter will be ignored"));
        return None;
    };
    Some(MeterMeta {
        name: name.to_string(),
        meter_type,
        region,
        color_match: hsl(el, ["h", "s", "l"], warnings, name),
        hdr_match: hsl(el, ["hdr-h", "hdr-s", "hdr-l"], warnings, name),
        string: attr(el, "string").map(str::to_string),
        confidence: attr_f64(el, "confidence"),
        brightness: attr_f64(el, "brightness"),
        contrast: attr_f64(el, "contrast"),
        authoring_resolution: None,
        resolutions: Vec::new(),
    })
}

fn parse_display_size(s: &str) -> Option<DisplaySize> {
    let (w, h) = s.split_once(['x', 'X'])?;
    Some(DisplaySize { width: w.trim().parse().ok()?, height: h.trim().parse().ok()? })
}

/// A `<resolution>` needs `size="WxH"` or `aspect="W:H"` plus a valid region.
fn parse_resolution(el: &ElementRef<'_>) -> Option<ResolutionOverride> {
    let size = attr(el, "size").and_then(parse_display_size);
    let aspect = attr(el, "aspect").filter(|a| a.split_once(':').is_some()).map(str::to_string);
    if size.is_none() && aspect.is_none() {
        return None;
    }
    Some(ResolutionOverride { size, aspect, region: parse_region(el)? })
}

fn detect_uses(html: &str, has_meters: bool) -> Uses {
    let lower = html.to_ascii_lowercase();
    Uses {
        audio: html.contains("engine.audio"),
        video: html.contains("engine.zone"),
        meters: has_meters || html.contains("engine.vision"),
        engine: html.contains("engine."),
        webgl: lower.contains("webgl"),
        input: html.contains("onCanvasTapped") || html.contains("onKeyPressed") || html.contains("onCanvasApiEvent"),
    }
}

/// Parse an effect's HTML into its metadata. Never fails: malformed pieces
/// are dropped and described in [`EffectMeta::warnings`]. The returned `id`
/// is empty; callers set it with [`EffectMeta::with_id`].
pub fn parse_effect_meta(html: &str) -> EffectMeta {
    let doc = Html::parse_document(html);
    let mut meta = EffectMeta::default();

    if let Ok(sel) = Selector::parse("title") {
        if let Some(t) = doc.select(&sel).next() {
            meta.title = t.text().collect::<String>().trim().to_string();
        }
    }

    // `meta` and `resolution` in document order so a <resolution> attaches to
    // the meter that precedes it. (html5ever moves the unknown <resolution>
    // element into <body>, but document order is preserved.)
    let Ok(sel) = Selector::parse("meta, resolution") else {
        return meta;
    };
    for el in doc.select(&sel) {
        let tag = el.value().name();
        if tag == "resolution" {
            if let Some(r) = parse_resolution(&el) {
                if let Some(m) = meta.meters.last_mut() {
                    m.resolutions.push(r);
                } else {
                    meta.warnings.push("<resolution> without a preceding meter ignored".into());
                }
            } else {
                meta.warnings.push("Detected malformed <resolution> tag".into());
            }
            continue;
        }
        if let Some(d) = attr(&el, "description") {
            meta.description = d.to_string();
        }
        if let Some(p) = attr(&el, "publisher") {
            meta.publisher = p.to_string();
        }
        if let Some(b) = attr(&el, "background") {
            meta.background = b.eq_ignore_ascii_case("true") || b == "1";
        }
        if let Some(property) = attr(&el, "property").filter(|p| !p.is_empty()) {
            if let Some(c) = parse_control(&el, property, &mut meta.warnings) {
                meta.properties.push(c);
            }
        }
        if let Some(name) = attr(&el, "meter").filter(|m| !m.is_empty()) {
            if let Some(m) = parse_meter(&el, name, &mut meta.warnings) {
                meta.meters.push(m);
            }
        }
    }

    meta.uses = detect_uses(html, !meta.meters.is_empty());
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ControlValue;

    // Fixtures below are written for these tests; they imitate the *shape* of
    // the stock lightscripts (a single colour, a two-colour sweep, a screen
    // ambience effect) without copying any of their text or code.

    const SINGLE_COLOUR: &str = r##"<!DOCTYPE html>
<html><head>
<title>Plain Colour</title>
<meta description="Fills every device with one colour."/>
<meta publisher="HueDeck"/>
<meta property="color" label="Colour" type="color" min="0" max="360" default="#009bde"/>
</head><body style="margin:0;background:#000">
<canvas id="exCanvas" width="320" height="200"></canvas>
<script>
function oncolorChanged(){ paint(); }
function paint(){ var c = document.getElementById("exCanvas").getContext("2d"); c.fillStyle = color; c.fillRect(0,0,320,200); }
window.requestAnimationFrame(paint);
</script></body></html>"##;

    const TWO_COLOUR_SWEEP: &str = r##"<html><head>
<title>Ping Pong</title>
<meta description="A band bounces between two colours." />
<meta publisher="HueDeck" />
<meta property="color1" label="First colour" type="color" min="0" max="360" default="#ff0000" tooltip="Band colour"/>
<meta property="color2" label="Second colour" type="color" min="0" max="360" default="#0000ff"/>
<meta property="speed" label="Speed" type="number" min="1" max="20" default="5" tooltip="Pixels per frame"/>
<meta property="bounce" label="Bounce" type="boolean" default="1"/>
<meta property="width" label="Band width" type="number" min="10" max="320" step="10" default="40" group="Shape"/>
</head><body><script>
function oncolor1Changed(){}
function oncolor2Changed(){}
function onCanvasTapped(x, y){}
</script></body></html>"##;

    const AMBIENCE: &str = r##"<html><head>
<title>Desk Glow</title>
<meta description="Mirrors the screen edges onto your devices."/>
<meta publisher="HueDeck"/>
<meta property="mode" label="Picture mode" type="combobox" values="Standard,Cinema,Mono,Vivid" default="Standard"/>
<meta property="blur" label="Blur" type="number" min="0" max="50" default="12"/>
<meta property="saturate" label="Saturation boost" type="boolean" default="false"/>
</head><body><script>
function update(){ var img = engine.zone.imagedata; /* … */ window.requestAnimationFrame(update); }
function onmodeChanged(){}
</script></body></html>"##;

    const METERS: &str = r##"<html><head>
<title>HUD Reader</title>
<meta description="Reads a health bar."/>
<meta publisher="HueDeck"/>
<meta meter="health" type="linear" x="0.1" y="0.9" width="0.3" height="0.02" h="0-20" s="60-100" l="30-70" hdr-h="350-360">
  <resolution size="1920x1080" x="0.1" y="0.88" width="0.3" height="0.03"/>
  <resolution aspect="21:9" x="0.11" y="0.89" width="0.29"/>
  <resolution x="0.5" y="0.5" width="0.1"/>
<meta meter="broken" x="0.5" y="0.5" width="0.1"/>
<meta meter="offscreen" type="area" x="1.5" y="0.5" width="0.1" height="0.1"/>
<meta meter="score" type="ocr_numeric" x="0.8" y="0.05" width="0.15" height="0.05" string="SCORE" confidence="70" contrast="1.2"/>
<meta property="weird" label="?" type="dial" default="1"/>
<meta property="choice" label="Choice" type="list" values="a, b ,c" default="b"/>
</head><body><script>var v = engine.vision.health; engine.audio.level; var gl = c.getContext("webgl");</script></body></html>"##;

    #[test]
    fn single_colour_head() {
        let m = parse_effect_meta(SINGLE_COLOUR);
        assert_eq!(m.title, "Plain Colour");
        assert_eq!(m.description, "Fills every device with one colour.");
        assert_eq!(m.publisher, "HueDeck");
        assert!(!m.background);
        assert_eq!(m.properties.len(), 1);
        let c = &m.properties[0];
        assert_eq!((c.property.as_str(), c.label.as_str(), c.control_type), ("color", "Colour", ControlType::Color));
        assert_eq!((c.min, c.max), (Some(0.0), Some(360.0)));
        assert_eq!(c.default_value(), ControlValue::Text("#009bde".into()));
        assert_eq!(c.js_initializer(None), "var color = \"#009bde\";");
        assert_eq!(c.change_hook(), "oncolorChanged");
        assert!(m.uses.is_empty());
        assert!(m.warnings.is_empty());
        assert_eq!(m.id, "");
        assert_eq!(m.with_id("Plain Colour.html").id, "Plain Colour.html");
    }

    #[test]
    fn two_colour_sweep_controls_in_order() {
        let m = parse_effect_meta(TWO_COLOUR_SWEEP);
        assert_eq!(m.title, "Ping Pong");
        let props: Vec<&str> = m.properties.iter().map(|c| c.property.as_str()).collect();
        assert_eq!(props, ["color1", "color2", "speed", "bounce", "width"]);
        assert_eq!(m.control("color1").unwrap().tooltip.as_deref(), Some("Band colour"));
        assert_eq!(m.control("color2").unwrap().tooltip, None);
        let speed = m.control("speed").unwrap();
        assert_eq!(speed.control_type, ControlType::Number);
        assert_eq!(speed.default_value(), ControlValue::Number(5.0));
        assert_eq!(m.control("bounce").unwrap().default_value(), ControlValue::Bool(true));
        assert_eq!(m.control("width").unwrap().step, Some(10.0));
        assert_eq!(m.control("width").unwrap().group.as_deref(), Some("Shape"));
        assert_eq!(
            m.js_initializers(),
            "var color1 = \"#ff0000\";\nvar color2 = \"#0000ff\";\nvar speed = 5;\nvar bounce = true;\nvar width = 40;\n"
        );
        assert!(m.uses.input && !m.uses.engine);
    }

    #[test]
    fn ambience_combobox_and_video_flag() {
        let m = parse_effect_meta(AMBIENCE);
        let mode = m.control("mode").unwrap();
        assert_eq!(mode.control_type, ControlType::Combobox);
        assert_eq!(mode.values, ["Standard", "Cinema", "Mono", "Vivid"]);
        assert_eq!(mode.default_value(), ControlValue::Text("Standard".into()));
        assert_eq!(m.control("saturate").unwrap().default_value(), ControlValue::Bool(false));
        assert!(m.uses.video && m.uses.engine);
        assert!(!m.uses.audio && !m.uses.webgl);
    }

    #[test]
    fn meters_resolutions_and_warnings() {
        let m = parse_effect_meta(METERS);
        assert_eq!(m.meters.len(), 2, "broken and off-screen meters are dropped");
        let health = &m.meters[0];
        assert_eq!(health.name, "health");
        assert_eq!(health.meter_type, MeterType::Linear);
        assert_eq!(health.region, Region { x: 0.1, y: 0.9, width: 0.3, height: Some(0.02) });
        let win = health.color_match.unwrap();
        assert_eq!(win.h, Some(Range { lo: 0.0, hi: 20.0 }));
        assert_eq!(win.s, Some(Range { lo: 60.0, hi: 100.0 }));
        assert_eq!(win.l, Some(Range { lo: 30.0, hi: 70.0 }));
        let hdr = health.hdr_match.unwrap();
        assert_eq!(hdr.h, Some(Range { lo: 350.0, hi: 360.0 }));
        assert_eq!((hdr.s, hdr.l), (None, None));
        assert_eq!(health.resolutions.len(), 2, "a <resolution> without size or aspect is dropped");
        assert_eq!(health.resolutions[0].size, Some(DisplaySize { width: 1920, height: 1080 }));
        assert_eq!(health.resolutions[0].region.y, 0.88);
        assert_eq!(health.resolutions[1].aspect.as_deref(), Some("21:9"));
        assert_eq!(health.resolutions[1].region.height, None);
        let score = &m.meters[1];
        assert_eq!(score.meter_type, MeterType::OcrNumeric);
        assert_eq!(score.string.as_deref(), Some("SCORE"));
        assert_eq!(score.confidence, Some(70.0));
        assert_eq!(score.contrast, Some(1.2));
        assert!(score.color_match.is_none());
        assert!(m.uses.meters && m.uses.audio && m.uses.webgl && m.uses.engine);

        // Unknown control type dropped; `list` alias accepted; values trimmed.
        assert!(m.control("weird").is_none());
        let choice = m.control("choice").unwrap();
        assert_eq!(choice.control_type, ControlType::Combobox);
        assert_eq!(choice.values, ["a", "b", "c"]);
        assert_eq!(choice.default_value(), ControlValue::Text("b".into()));

        assert!(m.warnings.iter().any(|w| w.contains("\"broken\"") && w.contains("Type not specified")));
        assert!(m.warnings.iter().any(|w| w.contains("\"offscreen\"")));
        assert!(m.warnings.iter().any(|w| w.contains("\"weird\"")));
        assert!(m.warnings.iter().any(|w| w.contains("malformed <resolution>")));
    }

    #[test]
    fn json_matches_effect_meta_schema() {
        let m = parse_effect_meta(METERS).with_id("HUD Reader.html");
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["id"], "HUD Reader.html");
        assert_eq!(json["title"], "HUD Reader");
        assert!(json.get("warnings").is_none(), "warnings are not part of the schema");
        assert!(json.get("controls").is_none());
        let health = &json["meters"][0];
        assert_eq!(health["type"], "linear");
        assert_eq!(health["region"]["width"], 0.3);
        assert_eq!(health["match"]["h"]["lo"], 0.0);
        assert_eq!(health["hdrMatch"]["h"]["hi"], 360.0);
        assert_eq!(health["resolutions"][0]["size"]["width"], 1920);
        assert_eq!(health["resolutions"][1]["aspect"], "21:9");
        assert!(health.get("hdr_match").is_none() && health.get("x").is_none());
        let choice = &json["properties"][1];
        assert_eq!(choice["type"], "combobox");
        assert_eq!(choice["values"][1], "b");
        assert!(choice.get("control_type").is_none() && choice.get("min").is_none());
        assert_eq!(json["uses"]["meters"], true);
        assert_eq!(json["uses"]["video"], false);

        // The example object from the schema deserialises.
        let example = r##"{
          "id": "Solid Color.html", "title": "Solid Color", "description": "A single colour", "publisher": "HueDeck",
          "properties": [ { "property": "color", "label": "Color", "type": "color", "min": "0", "max": "360", "default": "#ff0000" } ],
          "meters": [ { "name": "health", "type": "linear",
                        "region": { "x": 0.05, "y": 0.9, "width": 0.2, "height": 0.02 },
                        "match": { "h": { "lo": 90, "hi": 140 }, "s": { "lo": 50, "hi": 100 }, "l": { "lo": 30, "hi": 70 } },
                        "resolutions": [ { "size": { "width": 1920, "height": 1080 },
                                           "region": { "x": 0.04, "y": 0.91, "width": 0.2, "height": 0.02 } } ] } ],
          "uses": { "meters": true }
        }"##;
        let back: EffectMeta = serde_json::from_str(example).unwrap();
        assert_eq!(back.id, "Solid Color.html");
        // `min`/`max` are numberish strings in the example.
        assert_eq!((back.properties[0].min, back.properties[0].max), (Some(0.0), Some(360.0)));
        assert_eq!(back.meters[0].color_match.unwrap().h, Some(Range { lo: 90.0, hi: 140.0 }));
        assert_eq!(back.meters[0].resolutions[0].size, Some(DisplaySize { width: 1920, height: 1080 }));
        assert!(back.uses.meters);
    }

    #[test]
    fn empty_and_background() {
        let m = parse_effect_meta("");
        assert_eq!(m, EffectMeta::default());
        let m = parse_effect_meta("<title> Clock </title><meta background=\"true\"/>");
        assert_eq!(m.title, "Clock");
        assert!(m.background);
    }
}
