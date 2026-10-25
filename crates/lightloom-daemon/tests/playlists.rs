// SPDX-License-Identifier: GPL-3.0-or-later
//! Playlists, the auto-next timer, shuffle and the developer's `loadEffect`
//! through the engine, without devices: the snapshot follows, entries switch
//! by themselves after their duration, presets and values load, and the
//! playback survives a restart.

use lightloom_api::model::{Playlist, PlaylistEntry};
use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineHandle, EngineOptions, HardwareMode};
use lightloom_daemon::paths::Paths;
use lightloom_hal::mock::MockNotifier;
use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn call(backend: &dyn Backend, request: Request) -> Result<Value, ApiError> {
    backend.call(request).blocking_recv().expect("engine answers")
}

fn wait_for(backend: &dyn Backend, what: &str, mut done: impl FnMut(&Snapshot) -> bool) -> Arc<Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let s = backend.snapshot();
        if done(&s) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: {:#?}",
            s.lighting
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn page(title: &str, extra: &str) -> String {
    format!(
        "<head><title>{title}</title><meta property=\"speed\" label=\"Speed\" type=\"number\" default=\"1\"/>{extra}</head><body></body>"
    )
}

fn start(home: &Path) -> EngineHandle {
    engine::start(EngineOptions {
        paths: Paths::under(home),
        config: Config::default(),
        dry_run: true,
        hardware: HardwareMode::None,
        ephemeral: false,
        plugins_dir: None,
        effects_dir: Some(home.join("effects")),
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap()
}

fn entry(effect: &str, duration: Option<u32>) -> PlaylistEntry {
    PlaylistEntry {
        effect: effect.into(),
        duration,
        ..PlaylistEntry::default()
    }
}

fn effect(s: &Snapshot) -> &str {
    s.lighting.effect.as_deref().unwrap_or("")
}

#[test]
fn playlists_play_switch_and_resume() {
    let home = std::env::temp_dir().join(format!("lightloom-playlists-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let effects = home.join("effects");
    std::fs::create_dir_all(&effects).unwrap();
    std::fs::write(effects.join("alpha.html"), page("Alpha", "")).unwrap();
    std::fs::write(effects.join("beta.html"), page("Beta", "")).unwrap();
    std::fs::write(
        effects.join("gamer.html"),
        page(
            "Gamer",
            r#"<meta meter="health" type="linear" x="0.1" y="0.9" width="0.3" height="0.02" h="0-20" s="60-100" l="30-70">"#,
        ),
    )
    .unwrap();

    let handle = start(&home);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let s = wait_for(b, "running", |s| s.state == "Running");
    assert!(!s.lighting.timer.enabled && s.lighting.timer.interval == 300);
    assert!(s.lighting.playlist.is_none() && s.lighting.next_switch_ms.is_none());
    let gamer = s.effects.iter().find(|e| e.id == "lightloom:gamer").unwrap();
    assert_eq!(gamer.uses, ["meters"]);

    // Shuffle never picks the game integration.
    call(
        b,
        Request::ApplyEffect {
            id: "lightloom:alpha".into(),
            values: Map::new(),
        },
    )
    .unwrap();
    for _ in 0..12 {
        let v = call(b, Request::ShuffleEffect).unwrap();
        assert_ne!(v["effect"], "lightloom:gamer");
    }

    // A preset for the playlist to load.
    call(
        b,
        Request::SetEffectValues {
            id: "lightloom:beta".into(),
            values: json!({"speed": 7}).as_object().unwrap().clone(),
        },
    )
    .unwrap();
    call(
        b,
        Request::SavePreset {
            effect: "lightloom:beta".into(),
            name: "Fast".into(),
        },
    )
    .unwrap();
    call(
        b,
        Request::SetEffectValues {
            id: "lightloom:beta".into(),
            values: json!({"speed": 2}).as_object().unwrap().clone(),
        },
    )
    .unwrap();

    // Create: the id comes from the name; bad durations and names are refused.
    let list = Playlist {
        name: "Evening Mix".into(),
        entries: vec![
            entry("lightloom:alpha", Some(1)),
            entry("lightloom:missing", Some(1)),
            PlaylistEntry {
                preset: Some("Fast".into()),
                values: json!({"extra": true}).as_object().unwrap().clone(),
                ..entry("lightloom:beta", Some(1))
            },
        ],
        ..Playlist::default()
    };
    let saved = call(
        b,
        Request::SavePlaylist {
            id: None,
            playlist: list.clone(),
        },
    )
    .unwrap();
    assert_eq!(saved["id"], "evening-mix");
    let again = call(
        b,
        Request::SavePlaylist {
            id: None,
            playlist: list.clone(),
        },
    )
    .unwrap();
    assert_eq!(again["id"], "evening-mix-2");
    call(
        b,
        Request::DeletePlaylist {
            id: "evening-mix-2".into(),
        },
    )
    .unwrap();
    let mut bad = list.clone();
    bad.entries[0].duration = Some(0);
    assert_eq!(
        call(
            b,
            Request::SavePlaylist {
                id: None,
                playlist: bad
            }
        )
        .unwrap_err()
        .status,
        400
    );
    let mut bad = list.clone();
    bad.name = " ".into();
    assert_eq!(
        call(
            b,
            Request::SavePlaylist {
                id: None,
                playlist: bad
            }
        )
        .unwrap_err()
        .status,
        400
    );
    let s = wait_for(b, "the playlist listed", |s| s.playlists.len() == 1);
    assert_eq!(s.playlists[0].entries.len(), 3);

    // Play: alpha, then (missing skipped) beta with its preset and values, then alpha again.
    assert_eq!(
        call(
            b,
            Request::PlayPlaylist {
                id: "nope".into(),
                start: None
            }
        )
        .unwrap_err()
        .status,
        404
    );
    call(
        b,
        Request::PlayPlaylist {
            id: "evening-mix".into(),
            start: None,
        },
    )
    .unwrap();
    let s = b.snapshot();
    assert_eq!(
        (effect(&s), s.lighting.playlist.as_ref().unwrap().entry),
        ("lightloom:alpha", 0)
    );
    assert!(s.lighting.next_switch_ms.is_some());
    let s = wait_for(b, "beta by itself", |s| effect(s) == "lightloom:beta");
    assert_eq!(s.lighting.playlist.as_ref().unwrap().entry, 2);
    let beta = s.effects.iter().find(|e| e.id == "lightloom:beta").unwrap();
    assert_eq!(
        (&beta.values["speed"], &beta.values["extra"]),
        (&json!(7), &json!(true))
    );
    wait_for(b, "back to alpha", |s| effect(s) == "lightloom:alpha");

    // Next and previous move within the playlist; starting at an entry works.
    call(
        b,
        Request::PlayPlaylist {
            id: "evening-mix".into(),
            start: Some("lightloom:beta".into()),
        },
    )
    .unwrap();
    assert_eq!(effect(&b.snapshot()), "lightloom:beta");
    call(b, Request::StepEffect { previous: true }).unwrap();
    assert_eq!(
        effect(&b.snapshot()),
        "lightloom:alpha",
        "previous skips the missing entry"
    );
    assert_eq!(
        call(
            b,
            Request::PlayPlaylist {
                id: "evening-mix".into(),
                start: Some("lightloom:gamer".into())
            }
        )
        .unwrap_err()
        .status,
        404
    );

    // Choosing an effect ends the playlist.
    call(
        b,
        Request::ApplyEffect {
            id: "lightloom:gamer".into(),
            values: Map::new(),
        },
    )
    .unwrap();
    let s = b.snapshot();
    assert!(s.lighting.playlist.is_none() && s.lighting.next_switch_ms.is_none());

    // The timer without a playlist: next eligible effect (gamer is skipped, alpha follows it).
    assert_eq!(
        call(
            b,
            Request::SetEffectTimer {
                enabled: Some(true),
                interval: Some(0)
            }
        )
        .unwrap_err()
        .status,
        400
    );
    let v = call(
        b,
        Request::SetEffectTimer {
            enabled: Some(true),
            interval: Some(1),
        },
    )
    .unwrap();
    assert_eq!(v, json!({"enabled": true, "interval": 1}));
    wait_for(b, "the timer's switch", |s| effect(s) == "lightloom:alpha");
    wait_for(b, "and the next", |s| effect(s) == "lightloom:beta");
    call(
        b,
        Request::SetEffectTimer {
            enabled: Some(false),
            interval: None,
        },
    )
    .unwrap();

    // loadEffect: a developer effect, shown and listed; the uid must be a plain name.
    assert_eq!(
        call(
            b,
            Request::LoadEffect {
                uid: "../x".into(),
                html: page("X", "")
            }
        )
        .unwrap_err()
        .status,
        400
    );
    call(
        b,
        Request::LoadEffect {
            uid: "dev one".into(),
            html: page("Dev", ""),
        },
    )
    .unwrap();
    let s = b.snapshot();
    assert_eq!(effect(&s), "developer:dev one");
    assert_eq!(
        s.effects
            .iter()
            .find(|e| e.id == "developer:dev one")
            .unwrap()
            .source,
        "developer"
    );
    assert_eq!(
        call(
            b,
            Request::EffectImage {
                id: "lightloom:alpha".into()
            }
        )
        .unwrap_err()
        .status,
        404
    );
    std::fs::write(effects.join("alpha.png"), b"png").unwrap();
    let v = call(
        b,
        Request::EffectImage {
            id: "lightloom:alpha".into(),
        },
    )
    .unwrap();
    assert_eq!(v["contentType"], "image/png");

    // The playback, the timer and the playlists survive a restart.
    call(
        b,
        Request::SetEffectTimer {
            enabled: None,
            interval: Some(120),
        },
    )
    .unwrap();
    let mut long = list.clone();
    for e in &mut long.entries {
        e.duration = Some(3600);
    }
    call(
        b,
        Request::SavePlaylist {
            id: Some("evening-mix".into()),
            playlist: long,
        },
    )
    .unwrap();
    call(
        b,
        Request::PlayPlaylist {
            id: "evening-mix".into(),
            start: Some("lightloom:beta".into()),
        },
    )
    .unwrap();
    drop(handle);
    let handle = start(&home);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let s = wait_for(b, "running again", |s| s.state == "Running");
    let p = s.lighting.playlist.as_ref().expect("still playing");
    assert_eq!(
        (p.id.as_str(), p.entry, effect(&s)),
        ("evening-mix", 2, "lightloom:beta")
    );
    assert_eq!(s.lighting.timer.interval, 120);

    // Deleting the playing playlist stops it; stop answers whether one played.
    call(
        b,
        Request::DeletePlaylist {
            id: "evening-mix".into(),
        },
    )
    .unwrap();
    assert!(b.snapshot().lighting.playlist.is_none());
    assert_eq!(call(b, Request::StopPlaylist).unwrap(), json!({"stopped": false}));
    drop(handle);
    let _ = std::fs::remove_dir_all(&home);
}
