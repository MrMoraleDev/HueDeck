// SPDX-License-Identifier: GPL-3.0-or-later
//! The audio output device and display chosen in the dashboard (`PATCH /v1/inputs`): they win
//! over config.toml, survive a restart, and `""` goes back to config.toml's choice. No device
//! is opened: without a renderer nothing captures.

use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineHandle, EngineOptions, HardwareMode};
use lightloom_daemon::paths::Paths;
use lightloom_hal::mock::MockNotifier;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn call(backend: &dyn Backend, request: Request) -> Result<Value, ApiError> {
    backend.call(request).blocking_recv().expect("engine answers")
}

fn wait_for(backend: &dyn Backend, what: &str, done: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let s = backend.snapshot();
        if done(&s) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: {:#?}",
            s.settings
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn start(home: &Path) -> EngineHandle {
    let mut config = Config::default();
    config.audio.device = Some("Speakers (config)".into());
    engine::start(EngineOptions {
        paths: Paths::under(home),
        config,
        dry_run: true,
        hardware: HardwareMode::None,
        ephemeral: false,
        plugins_dir: None,
        effects_dir: Some(home.join("effects")),
        components_dir: None,
        signalrgb_settings: None,
        renderer: None,
        plugin_cache: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap()
}

fn chosen(s: &Snapshot) -> (Option<&str>, Option<&str>) {
    (s.settings.audio_device.as_deref(), s.settings.display.as_deref())
}

#[test]
fn the_dashboard_choice_wins_and_persists() {
    let home = std::env::temp_dir().join(format!("lightloom-inputs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("effects")).unwrap();

    let handle = start(&home);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    wait_for(b, "boot", |s| s.state == "Running");
    assert_eq!(chosen(&b.snapshot()), (Some("Speakers (config)"), None));

    let r = call(
        b,
        Request::SetInputs {
            audio_device: Some("Headphones".into()),
            display: Some(r"\\.\DISPLAY2".into()),
        },
    )
    .unwrap();
    assert_eq!(
        r,
        json!({ "audioDevice": "Headphones", "display": r"\\.\DISPLAY2" })
    );
    wait_for(b, "the choice", |s| {
        chosen(s) == (Some("Headphones"), Some(r"\\.\DISPLAY2"))
    });
    drop(shared);
    handle.shutdown();

    // After a restart the saved choice still wins over config.toml.
    let handle = start(&home);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    wait_for(b, "the saved choice", |s| {
        s.state == "Running" && chosen(s) == (Some("Headphones"), Some(r"\\.\DISPLAY2"))
    });

    // "" goes back to config.toml (the audio device) or the default (the display); a field
    // left out does not change.
    call(
        b,
        Request::SetInputs {
            audio_device: Some(String::new()),
            display: None,
        },
    )
    .unwrap();
    wait_for(b, "config's device", |s| {
        chosen(s) == (Some("Speakers (config)"), Some(r"\\.\DISPLAY2"))
    });
    call(
        b,
        Request::SetInputs {
            audio_device: None,
            display: Some(String::new()),
        },
    )
    .unwrap();
    wait_for(b, "the primary display", |s| {
        chosen(s) == (Some("Speakers (config)"), None)
    });
    drop(shared);
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
