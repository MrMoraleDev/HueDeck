// SPDX-License-Identifier: GPL-3.0-or-later
//! `engine.audio` for the active effect: loopback capture runs only while the
//! effect reads `engine.audio` (its parsed `uses.audio`) and `[audio] enabled`
//! is on; each snapshot goes to the renderer as an `engine` command at the
//! paint rate (docs/audio.md).

use crate::config::AudioConfig;
use lightloom_inputs::audio::{loopback, AudioInput, BandScale};
use lightloom_inputs::AudioSnapshot;
use lightloom_render::EngineDataSink;
use serde_json::{json, Map, Value};

/// Analysis/push rate: the paint rate, at least 30 Hz (input-to-paint
/// latency) and at most 60 Hz.
fn rate(fps: u32) -> u32 {
    fps.clamp(30, 60)
}

/// Start capture for an effect that `uses_audio`, pushing into `sink`;
/// `None` when the effect does not read audio, capture is off, or there is no
/// renderer to feed.
pub fn feed(
    cfg: &AudioConfig,
    fps: u32,
    uses_audio: bool,
    sink: Option<EngineDataSink>,
) -> Option<AudioInput> {
    let sink = sink.filter(|_| uses_audio && cfg.enabled)?;
    let scale = BandScale::parse(&cfg.scale).unwrap_or_else(|| {
        tracing::warn!(
            "[audio] scale {:?} is neither \"log\" nor \"linear\"; using log",
            cfg.scale
        );
        BandScale::Log
    });
    let push = Box::new(move |s: &AudioSnapshot| {
        sink.push(engine_data(s));
    });
    match AudioInput::start(rate(fps), scale, loopback(cfg.device.clone()), push) {
        Ok(input) => Some(input),
        Err(e) => {
            tracing::warn!("audio: the capture thread did not start: {e}");
            None
        }
    }
}

/// `{"audio": {freq, level, rawlevel, width, density}}` for the renderer.
pub fn engine_data(s: &AudioSnapshot) -> Map<String, Value> {
    // Three decimals keep the line short; effects never need more.
    let r = |x: f32| (f64::from(x) * 1000.0).round() / 1000.0;
    let mut data = Map::new();
    data.insert(
        "audio".into(),
        json!({
            "freq": s.freq,
            "level": r(s.level),
            "rawlevel": r(s.rawlevel),
            "width": r(s.width),
            "density": r(s.density),
        }),
    );
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_data_matches_the_contract() {
        let mut s = AudioSnapshot::default();
        s.freq[3] = 127;
        s.level = -9.031_234;
        let data = engine_data(&s);
        let audio = data["audio"].as_object().unwrap();
        let freq = audio["freq"].as_array().unwrap();
        assert_eq!(freq.len(), 200);
        assert_eq!(freq[3], json!(127));
        assert_eq!(audio["level"], json!(-9.031));
        for k in ["rawlevel", "width", "density"] {
            assert_eq!(audio[k], json!(0.0), "{k}");
        }
        // One push is well under a kilobyte and a half.
        assert!(serde_json::to_string(&data).unwrap().len() < 1500);
    }

    #[test]
    fn capture_starts_only_for_audio_effects() {
        let cfg = AudioConfig::default();
        assert!(feed(&cfg, 30, false, None).is_none());
        assert!(feed(&cfg, 30, true, None).is_none(), "no renderer, no capture");
        // A renderer that never started: its sink exists but capture is off.
        let renderer = lightloom_render::CefRenderer::new(lightloom_render::CefRendererConfig {
            renderer: "unused".into(),
            effect: "unused.html".into(),
            fps: 30,
            properties: Map::new(),
            software_compositing: false,
            work_dir: std::env::temp_dir(),
            face: None,
        });
        let off = AudioConfig {
            enabled: false,
            ..AudioConfig::default()
        };
        assert!(feed(&off, 30, true, Some(renderer.engine_sink())).is_none());
        assert!(!renderer
            .engine_sink()
            .push(engine_data(&AudioSnapshot::default())));
        assert_eq!((rate(15), rate(30), rate(120)), (30, 30, 60));
    }
}
