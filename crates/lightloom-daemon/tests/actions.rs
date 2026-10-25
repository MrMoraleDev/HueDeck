// SPDX-License-Identifier: GPL-3.0-or-later
//! The action router through the engine, on a scripted device and starter
//! effects: `lightloom://` URLs change the effect (with coerced properties), the
//! layout (and back), a device, a playlist, the brightness and the canvas
//! pause; dashboard actions arrive as events; unknown paths are dropped,
//! marketplace paths refused, malformed URLs rejected. The URL-scheme setting
//! writes and removes its keys in an in-memory registry only.

use lightloom_api::model::{Playlist, PlaylistEntry};
use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineHandle, EngineOptions, HardwareMode};
use lightloom_daemon::paths::Paths;
use lightloom_daemon::scheme::UrlScheme;
use lightloom_hal::mock::{MockHid, MockNotifier, MockRegistry};
use lightloom_hal::{HidBackend, ReportLengths, UserRegistry};
use lightloom_plugin_host::session::SessionDevice;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:4d4e:A";

const PLUGIN: &str = r#"
export function Name() { return "Action probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4D4E; }
export function Size() { return [1, 1]; }
export function LedNames() { return ["A"]; }
export function LedPositions() { return [[0, 0]]; }
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
export function Render() { const c = device.color(0, 0); device.write([0x01, c[0], c[1], c[2]], 4); }
export function Shutdown() {}
"#;

const PAGE: &str = r##"<head><title>Rainbow Rise</title>
<meta property="speed" label="Speed" type="number" min="1" max="10" default="3"/>
<meta property="reverse" label="Reverse" type="boolean" default="false"/>
<meta property="tint" label="Tint" type="color" default="#ffffff"/>
<meta property="dir" label="Direction" type="combobox" values="Left,Right" default="Left"/>
</head><body></body>"##;

fn call(backend: &dyn Backend, request: Request) -> Result<Value, ApiError> {
    backend.call(request).blocking_recv().expect("engine answers")
}

fn action(b: &dyn Backend, url: &str) -> Result<Value, ApiError> {
    call(b, Request::Action { url: url.into() })
}

fn wait_for(backend: &dyn Backend, what: &str, mut done: impl FnMut(&Snapshot) -> bool) -> Arc<Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(30);
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

fn options(home: &Path, notifier: Arc<MockNotifier>, with_device: bool) -> EngineOptions {
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("probe.js"), PLUGIN).unwrap();
    let hardware = if with_device {
        let info = MockHid::endpoint_info(0x1234, 0x4D4E, "A", 0, 0xFF00, 1, 0, ReportLengths::default());
        let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new().add_endpoint(info.clone()));
        HardwareMode::Fixed {
            backend,
            devices: vec![SessionDevice {
                smbus: None,
                lamparray: None,
                usb: None,
                key: KEY.into(),
                vid: 0x1234,
                pid: 0x4D4E,
                endpoints: vec![info],
                network: None,
                serial: None,
            }],
            smbus: Vec::new(),
            lamparrays: None,
        }
    } else {
        HardwareMode::None
    };
    EngineOptions {
        paths: Paths::under(home),
        config: Config::default(),
        dry_run: true,
        hardware,
        ephemeral: false,
        plugins_dir: Some(plugins),
        effects_dir: Some(home.join("effects")),
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        notifier,
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
        signalrgb_settings: None,
    }
}

fn home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("lightloom-actions-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("effects")).unwrap();
    std::fs::write(home.join("effects").join("rainbow-rise.html"), PAGE).unwrap();
    std::fs::write(
        home.join("effects").join("plain.html"),
        "<head><title>Plain</title></head><body></body>",
    )
    .unwrap();
    home
}

fn code(r: Result<Value, ApiError>) -> (u16, String) {
    let e = r.expect_err("an error");
    (e.status, e.code)
}

#[test]
fn actions_drive_the_engine() {
    let home = home("drive");
    let notifier = Arc::new(MockNotifier::default());
    let handle: EngineHandle = engine::start(options(&home, notifier.clone(), true)).unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let mut events = b.subscribe();
    wait_for(b, "the device running", |s| {
        s.state == "Running" && s.devices.iter().any(|d| d.key == KEY && d.state == "running")
    });

    // effect/apply by title, properties coerced; _location and unknown properties left out.
    let r = action(
        b,
        "lightloom://effect/apply/Rainbow%20Rise?speed=7&reverse=true&tint=%23FF0000&dir=Right&xPos=3&_location=Tour",
    )
    .unwrap();
    assert_eq!(r["action"], "effect/apply");
    assert_eq!(r["result"]["effect"], "lightloom:rainbow-rise");
    assert_eq!(r["result"]["ignored"], serde_json::json!(["xPos"]));
    let s = wait_for(b, "the effect", |s| {
        s.lighting.effect.as_deref() == Some("lightloom:rainbow-rise")
    });
    let view = s.effects.iter().find(|e| e.id == "lightloom:rainbow-rise").unwrap();
    assert_eq!(view.values["speed"], 7);
    assert_eq!(view.values["reverse"], true);
    assert_eq!(view.values["tint"], "#ff0000");
    assert_eq!(view.values["dir"], "Right");
    // By id without its source, and by full id.
    action(b, "effect/apply/plain").unwrap();
    wait_for(b, "plain", |s| {
        s.lighting.effect.as_deref() == Some("lightloom:plain")
    });
    action(b, "lightloom://effect/apply/lightloom:rainbow-rise").unwrap();
    // Strict values.
    assert_eq!(
        code(action(b, "lightloom://effect/apply/Rainbow%20Rise?speed=11")).1,
        "invalid_action"
    );
    assert_eq!(
        code(action(b, "lightloom://effect/apply/Rainbow%20Rise?dir=Up")).1,
        "invalid_action"
    );
    assert_eq!(code(action(b, "lightloom://effect/apply/Nope")).0, 404);

    // Brightness and the canvas pause.
    action(b, "lightloom://brightness/40").unwrap();
    action(b, "lightloom://canvas/pause").unwrap();
    let s = wait_for(b, "paused", |s| !s.lighting.enabled);
    assert_eq!(s.lighting.brightness, 40);
    action(b, "lightloom://canvas/resume").unwrap();
    wait_for(b, "resumed", |s| s.lighting.enabled);
    assert_eq!(code(action(b, "lightloom://brightness/101")).1, "invalid_action");

    // Layouts: apply, then back and forth with applyprevious.
    assert_eq!(code(action(b, "lightloom://layout/applyprevious")).0, 409);
    let first = b.snapshot().lighting.layout.clone();
    call(b, Request::SaveLayoutAs { name: "Desk".into() }).unwrap();
    action(b, &format!("lightloom://layout/apply/{first}")).unwrap();
    wait_for(b, "first layout", |s| s.lighting.layout == first);
    action(b, "lightloom://layout/applyprevious").unwrap();
    wait_for(b, "Desk", |s| s.lighting.layout == "Desk");
    action(b, "lightloom://layout/applyprevious").unwrap();
    wait_for(b, "first again", |s| s.lighting.layout == first);
    assert_eq!(code(action(b, "lightloom://layout/apply/Missing")).0, 404);

    // A device off and on (its key matched case-insensitively).
    action(b, &format!("lightloom://device/{}/disable", KEY.to_uppercase())).unwrap();
    wait_for(b, "disabled", |s| {
        s.devices.iter().any(|d| d.key == KEY && !d.enabled)
    });
    action(b, &format!("lightloom://device/{KEY}/enable")).unwrap();
    wait_for(b, "enabled", |s| {
        s.devices.iter().any(|d| d.key == KEY && d.enabled)
    });
    assert_eq!(code(action(b, "lightloom://device/0000:0000:X/enable")).0, 404);

    // A playlist plays and stops.
    let playlist = Playlist {
        name: "Evening".into(),
        entries: vec![PlaylistEntry {
            effect: "lightloom:plain".into(),
            ..PlaylistEntry::default()
        }],
        ..Playlist::default()
    };
    let saved = call(b, Request::SavePlaylist { id: None, playlist }).unwrap();
    let id = saved["id"].as_str().unwrap().to_string();
    action(b, &format!("lightloom://playlist/{id}/play")).unwrap();
    wait_for(b, "playing", |s| {
        s.lighting.playlist.as_ref().is_some_and(|p| p.id == id)
    });
    action(b, &format!("lightloom://playlist/{id}/stop")).unwrap();
    wait_for(b, "stopped", |s| s.lighting.playlist.is_none());
    let r = action(b, &format!("lightloom://playlist/{id}/stop")).unwrap();
    assert_eq!(r["result"]["stopped"], false);
    assert_eq!(code(action(b, "lightloom://playlist/none/play")).0, 404);
    assert_eq!(code(action(b, "lightloom://playlist/none/stop")).0, 404);

    // Dashboard actions arrive as `action` events; a toast is a notification too.
    action(b, "lightloom://view/showdevice?uid=K&_location=x").unwrap();
    action(b, "lightloom://app/raiseWindow").unwrap();
    let r = action(b, "lightloom://app/toast?title=Hi&message=There").unwrap();
    assert_eq!(r["result"]["notified"], true);
    assert_eq!(notifier.shown().last().map(|n| n.title.as_str()), Some("Hi"));
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while seen.len() < 3 && Instant::now() < deadline {
        match events.try_recv() {
            Ok(e) if e.kind == "action" => seen.push(e.data),
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert_eq!(
        (seen[0]["action"].as_str(), seen[0]["tab"].as_str()),
        (Some("view"), Some("devices"))
    );
    assert_eq!(seen[0]["uid"], "K");
    assert_eq!(seen[1]["action"], "raiseWindow");
    assert_eq!(
        (seen[2]["action"].as_str(), seen[2]["message"].as_str()),
        (Some("toast"), Some("There"))
    );

    // Unknown paths are dropped with a warning; marketplace paths refused; bad URLs rejected.
    let r = action(b, "lightloom://addon/install?repo=x").unwrap();
    assert_eq!(r["dropped"], true);
    assert!(r["warning"].as_str().unwrap().contains("addon"));
    assert_eq!(action(b, "lightloom://view/cooling").unwrap()["dropped"], true);
    assert_eq!(
        code(action(
            b,
            "lightloom://effect/install/Side%20to%20Side?_location=LayoutTour"
        )),
        (409, "action_refused".into())
    );
    assert_eq!(
        code(action(b, "https://example.com/app/raiseWindow")).1,
        "invalid_action"
    );

    // app/restart reloads in place; the engine keeps answering.
    let r = action(b, "lightloom://app/restart").unwrap();
    assert_eq!(r["result"]["restarted"], true);
    wait_for(b, "running again", |s| {
        s.state == "Running" && s.devices.iter().any(|d| d.key == KEY && d.state == "running")
    });
    action(b, "brightness/60").unwrap();
    wait_for(b, "brightness 60", |s| s.lighting.brightness == 60);

    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_url_scheme_setting_writes_only_a_fake_registry() {
    let home = home("scheme");
    let notifier = Arc::new(MockNotifier::default());
    let reg = Arc::new(MockRegistry::default());
    let registry = || -> Option<Arc<dyn UserRegistry>> { Some(reg.clone()) };

    // Without a registry (engine::start), the setting is unavailable and nothing is written.
    let handle = engine::start(options(&home, notifier.clone(), false)).unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let s = wait_for(b, "running", |s| s.state == "Running");
    assert!(!s.settings.url_scheme && !s.settings.url_scheme_available);
    assert_eq!(code(call(b, Request::SetUrlScheme { enabled: true })).0, 409);
    assert!(call(b, Request::SetUrlScheme { enabled: false }).is_ok());
    handle.shutdown();

    // Off by default: nothing is written at startup.
    let client = PathBuf::from(r"C:\Lightloom\lightloom.exe");
    let handle = engine::start_with(
        options(&home, notifier.clone(), false),
        UrlScheme::new(registry(), Some(client.clone())),
    )
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let s = wait_for(b, "running", |s| s.state == "Running");
    assert!(!s.settings.url_scheme && s.settings.url_scheme_available);
    assert!(reg.dump().is_empty());

    // Turned on: the four values are written.
    assert_eq!(
        call(b, Request::SetUrlScheme { enabled: true }).unwrap()["urlScheme"],
        true
    );
    wait_for(b, "on", |s| s.settings.url_scheme);
    assert_eq!(reg.dump().len(), 4);
    assert_eq!(
        reg.read_string(r"Software\Classes\lightloom\shell\open\command", "")
            .unwrap()
            .as_deref(),
        Some(r#""C:\Lightloom\lightloom.exe" action "%1""#)
    );
    handle.shutdown();

    // A moved copy re-registers at startup while the setting is on.
    let handle = engine::start_with(
        options(&home, notifier.clone(), false),
        UrlScheme::new(registry(), Some(PathBuf::from(r"D:\Portable\lightloom.exe"))),
    )
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let s = wait_for(b, "running", |s| s.state == "Running");
    assert!(s.settings.url_scheme && s.settings.url_scheme_notice.is_none());
    assert_eq!(
        reg.read_string(r"Software\Classes\lightloom\shell\open\command", "")
            .unwrap()
            .as_deref(),
        Some(r#""D:\Portable\lightloom.exe" action "%1""#)
    );

    // Turned off: the key is gone.
    call(b, Request::SetUrlScheme { enabled: false }).unwrap();
    wait_for(b, "off", |s| !s.settings.url_scheme);
    assert!(reg.dump().is_empty());
    handle.shutdown();

    // A registry that refuses writes: an error, the setting stays off.
    reg.fail_writes.store(true, std::sync::atomic::Ordering::SeqCst);
    let handle = engine::start_with(
        options(&home, notifier, false),
        UrlScheme::new(registry(), Some(client)),
    )
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    wait_for(b, "running", |s| s.state == "Running");
    assert_eq!(
        code(call(b, Request::SetUrlScheme { enabled: true })),
        (500, "url_scheme_failed".into())
    );
    assert!(!b.snapshot().settings.url_scheme);
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
