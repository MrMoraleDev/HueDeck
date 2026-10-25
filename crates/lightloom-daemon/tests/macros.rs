// SPDX-License-Identifier: GPL-3.0-or-later
//! The macro runtime through the engine, without hardware: a user Action in
//! `<config>/macroscripts/Actions` is listed, refused while macros are off,
//! then run: `signalrgb.SetGlobalBrightness` reaches the engine, `keys.press`
//! reaches the recording desktop (never the OS), `system.action` the action
//! router, and the settings survive a restart.

use lightloom_api::{ApiError, Backend, Request};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineOptions, HardwareMode};
use lightloom_daemon::paths::Paths;
use lightloom_daemon::scheme::UrlScheme;
use lightloom_hal::input::{RecordingDesktop, SynthInput};
use lightloom_hal::mock::MockNotifier;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ACTION: &str = r#"
export function Action() {
  this.Name = "Dim";
  this.Description = "Dims and presses a key";
  this.Options = [{ property: "Level", label: "Level", type: "number", min: 0, max: 100, default: 30 }];
  this.onExecute = function () {
    signalrgb.SetGlobalBrightness(this.Level);
    keys.press("Ctrl + Shift + Esc");
    system.action("canvas/pause");
  };
}
"#;

fn call(b: &dyn Backend, r: Request) -> Result<Value, ApiError> {
    b.call(r).blocking_recv().expect("engine answers")
}

fn options(home: &Path) -> EngineOptions {
    EngineOptions {
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
        signalrgb_settings: None,
    }
}

#[test]
fn user_actions_run_through_the_engine() {
    let home = std::env::temp_dir().join(format!("lightloom-macros-daemon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("effects")).unwrap();
    let actions = Paths::under(&home).config.join("macroscripts").join("Actions");
    std::fs::create_dir_all(&actions).unwrap();
    std::fs::write(actions.join("Dim.js"), ACTION).unwrap();
    let id = "user:action:Dim";

    let desktop = RecordingDesktop::new();
    let handle =
        engine::start_with_desktop(options(&home), UrlScheme::unavailable(), Some(desktop.clone())).unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let view = call(b, Request::Macros).unwrap();
    assert_eq!(view["enabled"], false);
    assert_eq!(view["hooksAvailable"], false);
    assert_eq!(view["scripts"][0]["id"], id);
    assert_eq!(view["scripts"][0]["options"][0]["value"], 30);
    assert!(
        Paths::under(&home)
            .config
            .join("macroscripts")
            .join("Inputs")
            .is_dir(),
        "the folders are created"
    );

    let e = call(b, Request::RunMacro { id: id.into() }).unwrap_err();
    assert_eq!(e.status, 409, "macros are off");
    let e = call(
        b,
        Request::SetMacro {
            id: id.into(),
            enabled: None,
            options: Some(json!({"Level": 300}).as_object().cloned().unwrap()),
            actions: None,
        },
    )
    .unwrap_err();
    assert_eq!(e.status, 400);
    assert_eq!(
        call(
            b,
            Request::RunMacro {
                id: "user:action:Nope".into()
            }
        )
        .unwrap_err()
        .status,
        404
    );

    call(
        b,
        Request::SetMacros {
            enabled: Some(true),
            keyboard_hook: Some(true),
            mouse_hook: None,
            allow_launch: None,
        },
    )
    .unwrap();
    call(
        b,
        Request::SetMacro {
            id: id.into(),
            enabled: None,
            options: Some(json!({"Level": 25}).as_object().cloned().unwrap()),
            actions: None,
        },
    )
    .unwrap();
    call(b, Request::RunMacro { id: id.into() }).unwrap();
    let end = Instant::now() + Duration::from_secs(20);
    while !(b.snapshot().lighting.brightness == 25
        && !b.snapshot().lighting.enabled
        && desktop.inputs().len() == 6)
    {
        assert!(Instant::now() < end, "timed out: {:?}", desktop.calls());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        desktop.inputs()[..3],
        [
            SynthInput::Key {
                vk: 0x11,
                down: true,
                extended: false
            },
            SynthInput::Key {
                vk: 0x10,
                down: true,
                extended: false
            },
            SynthInput::Key {
                vk: 0x1B,
                down: true,
                extended: false
            },
        ]
    );
    handle.shutdown();

    // The settings are persisted.
    let handle = engine::start_with_desktop(options(&home), UrlScheme::unavailable(), Some(desktop)).unwrap();
    let shared = handle.shared();
    let view = call(&*shared, Request::Macros).unwrap();
    assert_eq!(view["enabled"], true);
    assert_eq!(view["keyboardHook"], true);
    assert_eq!(view["scripts"][0]["options"][0]["value"], 25);
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}

const WATCH: &str = r#"
export function Input() {
  this.Name = "Watch";
  this.Description = "Logs log-off and sensor updates";
  this.Options = [];
  this.onApplication = function (event) {
    if (event.EventType === "System Logoff") console.log(`logoff ${event.Sender}/${event.SenderType}`);
  };
  this.onSensorUpdate = function (event) {
    console.log(`sensors ${typeof event.data.sensors}`);
  };
}
"#;

/// `macros` events follow the runtime; a log-off and the sensor poller reach Inputs.
#[test]
fn macro_events_logoff_and_sensors_reach_inputs() {
    use lightloom_hal::PowerEvent;
    let home = std::env::temp_dir().join(format!("lightloom-macros-events-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("effects")).unwrap();
    let inputs = Paths::under(&home).config.join("macroscripts").join("Inputs");
    std::fs::create_dir_all(&inputs).unwrap();
    std::fs::write(inputs.join("Watch.js"), WATCH).unwrap();
    let id = "user:input:Watch";

    let handle = engine::start_with_desktop(
        options(&home),
        UrlScheme::unavailable(),
        Some(RecordingDesktop::new()),
    )
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let mut events = b.subscribe();
    call(
        b,
        Request::SetMacros {
            enabled: Some(true),
            keyboard_hook: None,
            mouse_hook: None,
            allow_launch: None,
        },
    )
    .unwrap();
    call(
        b,
        Request::SetMacro {
            id: id.into(),
            enabled: Some(true),
            options: None,
            actions: None,
        },
    )
    .unwrap();
    let power = shared.power_sink();
    power(PowerEvent::Logoff);
    power(PowerEvent::Logoff);

    let (mut running, mut logoffs, mut sensors) = (false, 0, false);
    let end = Instant::now() + Duration::from_secs(20);
    while !(running && logoffs >= 1 && sensors) {
        assert!(
            Instant::now() < end,
            "timed out: running {running}, logoffs {logoffs}, sensors {sensors}"
        );
        match events.try_recv() {
            Ok(e) if e.kind == "macros" => {
                running |= e.data["scripts"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| s["id"] == id && s["running"] == true));
            }
            Ok(e) if e.kind == "log" => {
                let line = e.data.as_str().unwrap_or_default();
                if line == "[macro Watch] logoff SignalRGB/System" {
                    logoffs += 1;
                }
                sensors |= line == "[macro Watch] sensors object";
            }
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(e) = events.try_recv() {
        if e.kind == "log" && e.data == "[macro Watch] logoff SignalRGB/System" {
            logoffs += 1;
        }
    }
    assert_eq!(logoffs, 1, "a repeated log-off is the same one");
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
