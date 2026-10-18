// SPDX-License-Identifier: Apache-2.0 OR MIT
//! The engine end to end on a scripted device: a small plugin drives a mock
//! HID device while requests change the effect, brightness, placements,
//! channel components, plugin parameters, layouts and the database, and the
//! snapshot follows.

use huedeck_api::{ApiError, Backend, Request, Snapshot};
use huedeck_daemon::config::Config;
use huedeck_daemon::engine::{self, EngineOptions, HardwareMode};
use huedeck_daemon::paths::{self, Paths};
use huedeck_engine::{Placement, Point};
use huedeck_hal::mock::MockHid;
use huedeck_hal::{HidBackend, ReportLengths};
use huedeck_plugin_host::session::SessionDevice;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:4d4d:D";

const PLUGIN: &str = r#"
export function Name() { return "Daemon probe"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4D4D; }
export function Size() { return [2, 1]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["A", "B"]; }
export function LedPositions() { return [[0, 0], [1, 0]]; }
export function ControllableParameters() {
  return [{ property: "mode", label: "Mode", type: "combobox", values: ["Canvas", "Forced"], default: "Canvas" }];
}
/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() { device.addChannel("ARGB", 40, 4); }
export function Render() {
  const c = device.color(0, 0);
  device.write([0x01, c[0], c[1], c[2]], 4);
}
export function Shutdown() {}
"#;

fn call(backend: &dyn Backend, request: Request) -> Result<Value, ApiError> {
    backend.call(request).blocking_recv().expect("engine answers")
}

fn wait_for(backend: &dyn Backend, what: &str, mut done: impl FnMut(&Snapshot) -> bool) -> Arc<Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let s = backend.snapshot();
        if done(&s) {
            return s;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}: {s:#?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn device(s: &Snapshot) -> Option<&huedeck_api::model::DeviceView> {
    s.devices.iter().find(|d| d.key == KEY)
}

#[test]
fn the_engine_drives_a_device_and_follows_requests() {
    let home = std::env::temp_dir().join(format!("huedeck-engine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("probe.js"), PLUGIN).unwrap();

    let info = MockHid::endpoint_info(0x1234, 0x4D4D, "D", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let handle = engine::start(EngineOptions {
        paths: Paths::under(&home),
        config: Config::default(),
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend,
            devices: vec![SessionDevice {
                key: KEY.into(),
                vid: 0x1234,
                pid: 0x4D4D,
                endpoints: vec![info],
            }],
        },
        ephemeral: true,
        plugins_dir: Some(plugins),
        effects_dir: paths::resource_dir("effects", "effects"),
        components_dir: None,
        renderer: None,
        plugin_cache: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let mut events = b.subscribe();

    // Boot: the device runs its plugin; the effect falls back to a rainbow without the renderer.
    let s = wait_for(b, "the device rendering", |s| {
        s.state == "Running" && device(s).is_some_and(|d| d.state == "running" && d.frames > 2)
    });
    let d = device(&s).unwrap();
    assert_eq!((d.name.as_str(), d.endpoints[0].size), ("Daemon probe", [2, 1]));
    assert_eq!(d.parameters[0]["value"], "Canvas");
    assert_eq!(s.lighting.effect.as_deref(), Some(engine::DEFAULT_EFFECT));
    assert_eq!(s.lighting.renderer.kind, "null");
    assert!(
        s.warnings.iter().any(|w| w.contains("huedeck-renderer")),
        "{:?}",
        s.warnings
    );
    assert!(mock.recorder().writes().iter().any(|w| w[0] == 0x01));
    let s = wait_for(b, "the channel", |s| {
        device(s).is_some_and(|d| !d.channels.is_empty())
    });
    assert_eq!(device(&s).unwrap().channels[0].name, "ARGB");

    // Lighting.
    call(
        b,
        Request::SetLighting {
            brightness: Some(50),
            enabled: None,
        },
    )
    .unwrap();
    assert_eq!(b.snapshot().lighting.brightness, 50);
    call(
        b,
        Request::ApplyEffect {
            id: "huedeck:plasma".into(),
            values: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(b.snapshot().lighting.effect_name.as_deref(), Some("Plasma"));
    assert_eq!(
        call(
            b,
            Request::ApplyEffect {
                id: "nope".into(),
                values: Default::default()
            }
        )
        .unwrap_err()
        .status,
        404
    );
    call(
        b,
        Request::SetEffectValues {
            id: "huedeck:plasma".into(),
            values: json!({"speed": 70}).as_object().unwrap().clone(),
        },
    )
    .unwrap();
    call(
        b,
        Request::SavePreset {
            effect: "huedeck:plasma".into(),
            name: "Fast".into(),
        },
    )
    .unwrap();
    let s = b.snapshot();
    let plasma = s.effects.iter().find(|e| e.id == "huedeck:plasma").unwrap();
    assert_eq!(
        (plasma.values["speed"].clone(), plasma.presets.clone()),
        (json!(70), vec!["Fast".to_string()])
    );
    call(b, Request::StepEffect { previous: false }).unwrap();
    assert_ne!(b.snapshot().lighting.effect.as_deref(), Some("huedeck:plasma"));
    call(
        b,
        Request::ApplyPreset {
            effect: "huedeck:plasma".into(),
            name: "Fast".into(),
        },
    )
    .unwrap();
    assert_eq!(b.snapshot().lighting.effect.as_deref(), Some("huedeck:plasma"));

    // Placement reaches the running session.
    let moved = Placement {
        position: Point { x: 100.0, y: 50.0 },
        ..Placement::default()
    };
    call(
        b,
        Request::SetPlacement {
            uid: KEY.into(),
            placement: moved.clone(),
        },
    )
    .unwrap();
    wait_for(b, "the placement", |s| {
        device(s).is_some_and(|d| d.endpoints.first().is_some_and(|e| e.placement == moved))
    });

    // Channel components restart the session with the new chain.
    let r = call(
        b,
        Request::SetChannel {
            device: KEY.into(),
            channel: "ARGB".into(),
            components: vec![json!("Default Strip - 7")],
        },
    )
    .unwrap();
    assert_eq!(r["components"][0]["LedCount"], 7);
    wait_for(b, "the new chain", |s| {
        device(s).is_some_and(|d| {
            d.channels
                .first()
                .is_some_and(|c| c.components.first().is_some_and(|x| x["LedCount"] == 7))
        })
    });
    assert_eq!(
        call(
            b,
            Request::SetChannel {
                device: KEY.into(),
                channel: "ARGB".into(),
                components: vec![json!("Nope")]
            }
        )
        .unwrap_err()
        .status,
        404
    );

    // Plugin parameters are stored and shown.
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: None,
            parameters: Some(json!({"mode": "Forced"}).as_object().unwrap().clone()),
        },
    )
    .unwrap();
    assert_eq!(device(&b.snapshot()).unwrap().parameters[0]["value"], "Forced");

    // Disable and enable.
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: Some(false),
            parameters: None,
        },
    )
    .unwrap();
    wait_for(b, "the device disabled", |s| {
        device(s).is_some_and(|d| d.state == "disabled")
    });
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: Some(true),
            parameters: None,
        },
    )
    .unwrap();
    wait_for(b, "the device back", |s| {
        device(s).is_some_and(|d| d.state == "running")
    });

    // Layouts.
    call(b, Request::SaveLayoutAs { name: "Desk".into() }).unwrap();
    let s = b.snapshot();
    assert_eq!(s.lighting.layout, "Desk");
    assert!(s.layouts.contains(&"Desk".to_string()));
    let desk = call(b, Request::GetLayout { name: "Desk".into() }).unwrap();
    assert_eq!(desk["endpoints"][KEY]["position"]["x"], 100.0);
    assert_eq!(
        call(b, Request::DeleteLayout { name: "Desk".into() })
            .unwrap_err()
            .status,
        409
    );
    call(
        b,
        Request::ApplyLayout {
            name: "Default".into(),
        },
    )
    .unwrap();
    call(b, Request::DeleteLayout { name: "Desk".into() }).unwrap();
    assert!(!b.snapshot().layouts.contains(&"Desk".to_string()));

    // The database round-trips.
    let exported = call(b, Request::Export).unwrap();
    assert_eq!(exported["format"], "huedeck-state");
    call(
        b,
        Request::Import {
            state: exported.clone(),
        },
    )
    .unwrap();
    // The import rescans, so the device's lastSeen moves on; everything else is identical.
    let without_seen = |mut v: Value| {
        for d in v["devices"].as_array_mut().unwrap() {
            d.as_object_mut().unwrap().remove("lastSeen");
        }
        v
    };
    assert_eq!(
        without_seen(call(b, Request::Export).unwrap()),
        without_seen(exported)
    );
    assert_eq!(
        call(
            b,
            Request::Import {
                state: json!({"format": "x"})
            }
        )
        .unwrap_err()
        .status,
        400
    );
    wait_for(b, "the device after the import", |s| {
        device(s).is_some_and(|d| d.state == "running")
    });

    // Clients were told about changes.
    let mut kinds = Vec::new();
    while let Ok(e) = events.try_recv() {
        kinds.push(e.kind);
    }
    assert!(
        kinds.iter().any(|k| k == "state") && kinds.iter().any(|k| k == "log"),
        "{kinds:?}"
    );

    handle.shutdown();
    drop(shared);
    let _ = std::fs::remove_dir_all(&home);
    let _: PathBuf = PathBuf::new();
}
