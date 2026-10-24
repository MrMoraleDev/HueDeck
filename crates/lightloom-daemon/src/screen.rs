// SPDX-License-Identifier: GPL-3.0-or-later
//! `engine.zone` and `engine.vision` for the active effect: display capture
//! runs only while the effect reads `engine.zone` (its parsed `uses.video`) or
//! declares `<meta meter>` tags, and `[screen] enabled` is on; each snapshot
//! goes to the renderer as an `engine` command (docs/screen.md).

use crate::config::ScreenConfig;
use lightloom_effects::{EffectMeta, HslRanges, MeterMeta, MeterType, Region};
use lightloom_inputs::screen::{
    desktop, displays, HslWindow, Meter, MeterKind, Rect, RectOverride, ScreenInput, ScreenSettings,
    ScreenSnapshot, VisionValue,
};
use lightloom_render::EngineDataSink;
use serde_json::{json, Map, Value};

/// Start capture for `effect` when it reads the screen, pushing into `sink`;
/// `None` when it does not, capture is off, or there is no renderer to feed.
pub fn feed(
    cfg: &ScreenConfig,
    effect: Option<&EffectMeta>,
    sink: Option<EngineDataSink>,
) -> Option<ScreenInput> {
    let settings = settings(cfg, effect?)?;
    let sink = sink?;
    if settings.meters.iter().any(|m| m.kind == MeterKind::Unsupported) {
        tracing::info!("screen: OCR meters are not supported and read 0");
    }
    tracing::debug!("screen: displays {:?}", displays());
    let push = Box::new(move |s: &ScreenSnapshot| {
        sink.push(engine_data(s));
    });
    match ScreenInput::start(settings, desktop(cfg.display.clone()), push) {
        Ok(input) => Some(input),
        Err(e) => {
            tracing::warn!("screen: the capture thread did not start: {e}");
            None
        }
    }
}

/// What to compute for `effect`; `None` when it needs no capture.
fn settings(cfg: &ScreenConfig, effect: &EffectMeta) -> Option<ScreenSettings> {
    let zone = effect.uses.video;
    let meters: Vec<Meter> = effect.meters.iter().map(meter).collect();
    (cfg.enabled && (zone || !meters.is_empty())).then(|| ScreenSettings {
        fps: cfg.fps.clamp(10, 60),
        zone,
        meters,
    })
}

fn rect(r: &Region) -> Rect {
    Rect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

fn window(r: &HslRanges) -> HslWindow {
    let w = |r: Option<lightloom_effects::Range>| r.map(|r| (r.lo as f32, r.hi as f32));
    HslWindow {
        h: w(r.h),
        s: w(r.s),
        l: w(r.l),
    }
}

/// `"16:9"` → `(16, 9)`.
fn aspect(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(':')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// A parsed `<meta meter>` as the capture evaluates it. Capture is SDR (an
/// HDR desktop is converted by Windows), so the SDR window applies; the HDR
/// one only when no SDR window is given.
pub fn meter(m: &MeterMeta) -> Meter {
    Meter {
        name: m.name.clone(),
        kind: match m.meter_type {
            MeterType::Linear => MeterKind::Linear,
            MeterType::Area => MeterKind::Area,
            MeterType::Colormean => MeterKind::ColorMean,
            MeterType::OcrTextmatch | MeterType::OcrNumeric | MeterType::Ocr => MeterKind::Unsupported,
        },
        rect: rect(&m.region),
        color: m
            .color_match
            .as_ref()
            .or(m.hdr_match.as_ref())
            .map(window)
            .unwrap_or_default(),
        overrides: m
            .resolutions
            .iter()
            .map(|o| RectOverride {
                size: o.size.map(|s| (s.width, s.height)),
                aspect: o.aspect.as_deref().and_then(aspect),
                rect: rect(&o.region),
            })
            .collect(),
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= c.len() {
                char::from(B64[(n >> shift) as usize & 63])
            } else {
                '='
            });
        }
    }
    out
}

/// `{"zone": {hue, saturation, lightness, imagedata}, "vision": {name: value}}`
/// for the renderer. `imagedata` travels as `"base64:…"` (85 KB instead of
/// ~230 KB of JSON numbers); the renderer decodes it into the typed array.
pub fn engine_data(s: &ScreenSnapshot) -> Map<String, Value> {
    let r = |x: f64| (x * 1000.0).round() / 1000.0;
    let mut data = Map::new();
    if let Some(z) = &s.zone {
        data.insert(
            "zone".into(),
            json!({
                "hue": z.hue,
                "saturation": z.saturation,
                "lightness": z.lightness,
                "imagedata": format!("base64:{}", base64(&z.imagedata)),
            }),
        );
    }
    if !s.vision.is_empty() {
        let vision: Map<String, Value> = s
            .vision
            .iter()
            .map(|(k, v)| {
                let v = match *v {
                    VisionValue::Number(x) => json!(r(x)),
                    VisionValue::Triple(t) => json!(t.map(r)),
                };
                (k.clone(), v)
            })
            .collect();
        data.insert("vision".into(), Value::Object(vision));
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_inputs::ZoneSnapshot;

    #[test]
    fn base64_matches_rfc4648() {
        for (raw, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn engine_data_matches_the_contract() {
        let mut zone = ZoneSnapshot::default();
        zone.hue[0] = 359;
        zone.saturation[1] = 100;
        let s = ScreenSnapshot {
            zone: Some(zone),
            vision: vec![
                ("health".into(), VisionValue::Number(0.123_456)),
                ("tint".into(), VisionValue::Triple([210.0, 50.0, 50.0])),
            ],
        };
        let data = engine_data(&s);
        let z = data["zone"].as_object().unwrap();
        assert_eq!(z["hue"].as_array().unwrap().len(), 560);
        assert_eq!(z["hue"][0], json!(359));
        assert_eq!(z["saturation"][1], json!(100));
        let img = z["imagedata"].as_str().unwrap();
        assert!(img.starts_with("base64:AAAA"));
        assert_eq!(img.len(), 7 + 64_000_usize.div_ceil(3) * 4);
        assert_eq!(
            data["vision"],
            json!({"health": 0.123, "tint": [210.0, 50.0, 50.0]})
        );
        // Under the renderer's 512 KiB control-line limit with room to spare.
        assert!(serde_json::to_string(&data).unwrap().len() < 100_000);
        let only_meters = ScreenSnapshot {
            zone: None,
            vision: vec![("hp".into(), VisionValue::Number(1.0))],
        };
        assert!(!engine_data(&only_meters).contains_key("zone"));
    }

    fn parse(html: &str) -> EffectMeta {
        lightloom_effects::parse_effect_meta(html)
    }

    #[test]
    fn capture_only_for_screen_effects() {
        let cfg = ScreenConfig::default();
        assert!(settings(&cfg, &parse("<head><title>x</title></head>")).is_none());
        let zone = settings(&cfg, &parse("<script>var z = engine.zone.hue;</script>")).unwrap();
        assert!(zone.zone && zone.meters.is_empty());
        assert_eq!(zone.fps, 30);
        let off = ScreenConfig {
            enabled: false,
            ..ScreenConfig::default()
        };
        assert!(settings(&off, &parse("<script>engine.zone</script>")).is_none());
        // No renderer: nothing to feed.
        assert!(feed(&cfg, Some(&parse("<script>engine.zone</script>")), None).is_none());
        assert!(feed(&cfg, None, None).is_none());
    }

    #[test]
    fn meters_convert_from_the_effect_meta() {
        let meta = parse(
            r#"<head>
            <meta meter="health" type="linear" x="0.1" y="0.9" width="0.3" height="0.02" h="340-20" s="60-100" l="30-70">
              <resolution size="1920x1080" x="0.2" y="0.9" width="0.3" height="0.02"/>
            </meta>
            <meta meter="ammo" type="ocr_numeric" x="0.8" y="0.9" width="0.1" height="0.05" string="1">
            </head>"#,
        );
        let s = settings(&ScreenConfig::default(), &meta).unwrap();
        assert!(!s.zone, "meters only: no zones");
        assert_eq!(s.meters.len(), 2, "{:?}", meta.warnings);
        let hp = &s.meters[0];
        assert_eq!((hp.name.as_str(), hp.kind), ("health", MeterKind::Linear));
        assert_eq!(hp.color.h, Some((340.0, 20.0)));
        assert_eq!(hp.rect.height, Some(0.02));
        assert_eq!(hp.rect_for(1920, 1080).x, 0.2);
        assert_eq!(hp.rect_for(2560, 1440).x, 0.1);
        assert_eq!(s.meters[1].kind, MeterKind::Unsupported);
        assert_eq!(aspect("21:9"), Some((21, 9)));
        assert_eq!(aspect("wide"), None);
    }
}
