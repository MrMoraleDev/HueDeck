// SPDX-License-Identifier: GPL-3.0-or-later
//! The engine end to end on a scripted device: a small plugin drives a mock
//! HID device while requests change the effect, brightness, placements,
//! channel components, plugin parameters, layouts and the database, and the
//! snapshot follows.

use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineOptions, HardwareMode};
use lightloom_daemon::paths::{self, Paths};
use lightloom_engine::{Placement, Point};
use lightloom_hal::mock::{MockHid, MockNotifier};
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::session::SessionDevice;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:4d4d:D";

const PLUGIN: &str = r#"
export function Name() { return "Daemon probe"; }
export function Publisher() { return "Lightloom tests"; }
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

fn d_type(b: &dyn Backend) -> String {
    device(&b.snapshot()).unwrap().device_type.clone()
}

fn device(s: &Snapshot) -> Option<&lightloom_api::model::DeviceView> {
    s.devices.iter().find(|d| d.key == KEY)
}

#[test]
fn the_engine_drives_a_device_and_follows_requests() {
    let home = std::env::temp_dir().join(format!("lightloom-engine-{}", std::process::id()));
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
                smbus: None,
                key: KEY.into(),
                vid: 0x1234,
                pid: 0x4D4D,
                endpoints: vec![info],
            }],
            smbus: Vec::new(),
        },
        ephemeral: true,
        plugins_dir: Some(plugins),
        effects_dir: paths::resource_dir("effects", "effects"),
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
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
        s.warnings.iter().any(|w| w.contains("lightloom-renderer")),
        "{:?}",
        s.warnings
    );
    assert_eq!(
        (s.settings.port, s.settings.port_notice.as_deref()),
        (53000, None)
    );

    // The port from the dashboard is saved and shown (the API side moves the listener).
    let port = |p| Request::SetSettings {
        signalrgb: None,
        gpu_compositing: None,
        smbus: None,
        shutdown_mode: None,
        battery_low: None,
        battery_critical: None,
        battery_notifications: None,
        port: Some(p),
        openrgb_sdk: None,
    };
    assert_eq!(call(b, port(17000)).unwrap()["port"], 17000);
    wait_for(b, "the new port in the settings", |s| s.settings.port == 17000);
    assert_eq!(call(b, port(80)).unwrap_err().status, 400);
    assert!(mock.recorder().writes().iter().any(|w| w[0] == 0x01));

    // OpenRGB SDK: off by default, saved when turned on; direct colours win
    // over the canvas while they live, then the canvas is back.
    assert_eq!(d_type(b), "other");
    assert!(!b.snapshot().settings.openrgb_sdk);
    let on = Request::SetSettings {
        signalrgb: None,
        gpu_compositing: None,
        smbus: None,
        battery_low: None,
        battery_critical: None,
        battery_notifications: None,
        port: None,
        openrgb_sdk: Some(true),
        shutdown_mode: None,
    };
    assert_eq!(call(b, on).unwrap()["openrgbSdk"], true);
    assert!(b.snapshot().settings.openrgb_sdk);
    let direct = b.direct().expect("the daemon supports direct colours");
    let owner = direct.new_owner();
    let probe = lightloom_engine::Rgb::new(7, 77, 177);
    direct.write(owner, KEY, 2, 0, &[probe, probe], Duration::from_secs(30));
    let lit = |w: &Vec<u8>| w.len() >= 4 && w[..4] == [0x01, 7, 77, 177];
    let deadline = Instant::now() + Duration::from_secs(30);
    while !mock.recorder().writes().iter().any(lit) {
        assert!(
            Instant::now() < deadline,
            "the direct colour never reached the device"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    direct.release(owner);
    let seen = mock.recorder().writes().len();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let writes = mock.recorder().writes();
        if writes.len() > seen + 2 && !lit(writes.last().unwrap()) {
            break;
        }
        assert!(Instant::now() < deadline, "the canvas never came back");
        std::thread::sleep(Duration::from_millis(20));
    }
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
            id: "lightloom:plasma".into(),
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
            id: "lightloom:plasma".into(),
            values: json!({"speed": 70}).as_object().unwrap().clone(),
        },
    )
    .unwrap();
    call(
        b,
        Request::SavePreset {
            effect: "lightloom:plasma".into(),
            name: "Fast".into(),
        },
    )
    .unwrap();
    let s = b.snapshot();
    let plasma = s.effects.iter().find(|e| e.id == "lightloom:plasma").unwrap();
    assert_eq!(
        (plasma.values["speed"].clone(), plasma.presets.clone()),
        (json!(70), vec!["Fast".to_string()])
    );
    call(b, Request::StepEffect { previous: false }).unwrap();
    assert_ne!(b.snapshot().lighting.effect.as_deref(), Some("lightloom:plasma"));
    call(
        b,
        Request::ApplyPreset {
            effect: "lightloom:plasma".into(),
            name: "Fast".into(),
        },
    )
    .unwrap();
    assert_eq!(b.snapshot().lighting.effect.as_deref(), Some("lightloom:plasma"));

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
            shutdown_mode: None,
            blink_low_battery: None,
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
            shutdown_mode: None,
            blink_low_battery: None,
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
            shutdown_mode: None,
            blink_low_battery: None,
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
    assert_eq!(exported["format"], "lightloom-state");
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

#[test]
fn fixed_smbus_device_runs_and_is_listed() {
    use lightloom_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
    use lightloom_hal::{PciIds, SmbusBusKind};
    use lightloom_plugin_host::smbus::BusHandle;

    let home = std::env::temp_dir().join(format!("lightloom-engine-smbus-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let rec = TransactionRecorder::new();
    let mut bus = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec.clone())
        .with_pci(PciIds {
            vendor: 0x10DE,
            device: 0x2504,
            sub_vendor: 0x1462,
            sub_device: 0x3903,
        })
        .with_instance("pci01");
    bus.add_device(0x68, []);
    let mut config = Config::default();
    config.smbus.enabled = true;
    let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new());
    let handle = engine::start(EngineOptions {
        paths: Paths::under(&home),
        config,
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend,
            devices: Vec::new(),
            smbus: vec![BusHandle::new(Box::new(bus))],
        },
        ephemeral: true,
        plugins_dir: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")),
        effects_dir: paths::resource_dir("effects", "effects"),
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    const GPU: &str = "smbus:10de:2504:1462:3903@pci01:68";
    let s = wait_for(b, "the GPU rendering", |s| {
        s.devices
            .iter()
            .any(|d| d.key == GPU && d.state == "running" && d.frames > 2)
    });
    let d = s.devices.iter().find(|d| d.key == GPU).unwrap();
    assert_eq!(d.bus.as_deref(), Some("NVIDIA GeForce RTX 3060 · 0x68"));
    assert_eq!((d.vid, d.pid, d.io_errors), (0x10DE, 0x2504, 0));
    assert!(s.settings.smbus);
    assert!(!rec.payloads(TransactionKind::SmbusWrite).is_empty());

    // A rescan stops the SMBus session first (Shutdown() puts the speed register 0x38
    // back; Initialize() never writes it), then scans and starts it again.
    let speed_writes = || {
        rec.payloads(TransactionKind::SmbusWrite)
            .iter()
            .filter(|w| w[..2] == [0x68, 0x38])
            .count()
    };
    assert_eq!(speed_writes(), 0);
    call(b, Request::Rescan).unwrap();
    assert_eq!(speed_writes(), 1, "the session was shut down before the scan");
    let frames_before = rec.payloads(TransactionKind::SmbusWrite).len();
    wait_for(b, "the GPU rendering again", |s| {
        s.devices
            .iter()
            .any(|d| d.key == GPU && d.state == "running" && d.frames > 2)
    });
    assert!(rec.payloads(TransactionKind::SmbusWrite).len() > frames_before);

    // Turned off: the device goes away (its plugin restores it first).
    call(
        b,
        Request::SetSettings {
            signalrgb: None,
            gpu_compositing: None,
            smbus: Some(false),
            shutdown_mode: None,
            battery_low: None,
            battery_critical: None,
            battery_notifications: None,
            port: None,
            openrgb_sdk: None,
        },
    )
    .unwrap();
    wait_for(b, "the GPU stopped", |s| {
        s.devices.iter().all(|d| d.key != GPU || d.state == "absent")
    });
    let last = rec.payloads(TransactionKind::SmbusWrite).pop().unwrap();
    assert_eq!(&last[..2], &[0x68, 0x22], "Shutdown() writes the mode last");
    handle.shutdown();
    drop(shared);
    let _ = std::fs::remove_dir_all(&home);
}

/// "Save to devices" and the exit modes through the engine, on the MSI GPU plugin
/// over a mock bus: a preview, the confirm requirement, the save (with its audit
/// line), the per-device minute, and the exit mode reaching the plugin.
#[test]
fn save_to_devices_and_exit_modes() {
    use lightloom_api::model::DeviceSaveChoice;
    use lightloom_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
    use lightloom_hal::{PciIds, SmbusBusKind};
    use lightloom_plugin_host::smbus::BusHandle;
    use std::collections::BTreeMap;

    let home = std::env::temp_dir().join(format!("lightloom-engine-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let rec = TransactionRecorder::new();
    let mut bus = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec.clone())
        .with_pci(PciIds {
            vendor: 0x10DE,
            device: 0x2504,
            sub_vendor: 0x1462,
            sub_device: 0x3903,
        })
        .with_instance("pci01");
    bus.add_device(0x68, []);
    let mut config = Config::default();
    config.smbus.enabled = true;
    let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new());
    let handle = engine::start(EngineOptions {
        paths: Paths::under(&home),
        config,
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend,
            devices: Vec::new(),
            smbus: vec![BusHandle::new(Box::new(bus))],
        },
        ephemeral: true,
        plugins_dir: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")),
        effects_dir: None,
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
        notifier: Arc::new(MockNotifier::default()),
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    const GPU: &str = "smbus:10de:2504:1462:3903@pci01:68";
    let s = wait_for(b, "the GPU rendering", |s| {
        s.devices
            .iter()
            .any(|d| d.key == GPU && d.state == "running" && d.frames > 2)
    });
    let d = s.devices.iter().find(|d| d.key == GPU).unwrap();
    let save = d.save.as_ref().expect("the MSI GPU can save");
    assert!(save.static_colors && save.effects == ["spectrum", "breathing"]);
    assert_eq!(d.shutdown_modes, ["restore", "keep", "off"]);
    assert_eq!(s.settings.shutdown_mode, "restore");
    let commits = || {
        rec.payloads(TransactionKind::SmbusWrite)
            .iter()
            .filter(|w| w[..2] == [0x68, 0x3F])
            .count()
    };

    // A preview of every device writes nothing.
    let preview = call(
        b,
        Request::SaveToDevices {
            preview: true,
            confirm: false,
            devices: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(preview["devices"][0]["key"], GPU);
    assert_eq!(preview["devices"][0]["ok"], true, "{preview}");
    assert_eq!(preview["devices"][0]["result"]["zones"][0]["name"], "GPU");
    assert_eq!(commits(), 0);

    // A save needs the user's confirmation and named devices.
    let one = |effect: Option<&str>| {
        BTreeMap::from([(
            GPU.to_string(),
            DeviceSaveChoice {
                effect: effect.map(String::from),
            },
        )])
    };
    let refused = call(
        b,
        Request::SaveToDevices {
            preview: false,
            confirm: false,
            devices: one(None),
        },
    )
    .unwrap_err();
    assert_eq!(refused.status, 400);
    assert_eq!(commits(), 0);
    let saved = call(
        b,
        Request::SaveToDevices {
            preview: false,
            confirm: true,
            devices: one(Some("spectrum")),
        },
    )
    .unwrap();
    assert_eq!(saved["devices"][0]["ok"], true, "{saved}");
    assert_eq!(commits(), 1);
    let audit = std::fs::read_to_string(Paths::under(&home).save_audit_log()).unwrap();
    assert_eq!(audit.lines().count(), 1);
    assert!(
        audit.contains(GPU) && audit.contains("spectrum") && audit.contains("stored"),
        "{audit}"
    );

    // Once a minute per device; the refusal is in the answer and in the audit log.
    let again = call(
        b,
        Request::SaveToDevices {
            preview: false,
            confirm: true,
            devices: one(None),
        },
    )
    .unwrap();
    assert_eq!(again["devices"][0]["ok"], false);
    assert!(
        again["devices"][0]["error"].as_str().unwrap().contains("wait"),
        "{again}"
    );
    let audit = std::fs::read_to_string(Paths::under(&home).save_audit_log()).unwrap();
    assert_eq!(audit.lines().count(), 2, "the refusal is audited too: {audit}");
    assert_eq!(commits(), 1);
    let s = b.snapshot();
    let d = s.devices.iter().find(|d| d.key == GPU).unwrap();
    assert!(d.save.as_ref().unwrap().wait_secs > 0);
    // An effect the device does not offer.
    let wave = call(
        b,
        Request::SaveToDevices {
            preview: true,
            confirm: false,
            devices: one(Some("wave")),
        },
    )
    .unwrap();
    assert_eq!(wave["devices"][0]["ok"], false);

    // Exit modes: the setting, a per-device override, validation.
    call(
        b,
        Request::SetSettings {
            signalrgb: None,
            gpu_compositing: None,
            smbus: None,
            shutdown_mode: Some("keep".into()),
            battery_low: None,
            battery_critical: None,
            battery_notifications: None,
            port: None,
            openrgb_sdk: None,
        },
    )
    .unwrap();
    assert_eq!(b.snapshot().settings.shutdown_mode, "keep");
    assert_eq!(
        call(
            b,
            Request::SetSettings {
                signalrgb: None,
                gpu_compositing: None,
                smbus: None,
                shutdown_mode: Some("explode".into()),
                battery_low: None,
                battery_critical: None,
                battery_notifications: None,
                port: None,
                openrgb_sdk: None,
            },
        )
        .unwrap_err()
        .status,
        400
    );
    call(
        b,
        Request::SetDevice {
            key: GPU.into(),
            enabled: None,
            parameters: None,
            shutdown_mode: Some("off".into()),
            blink_low_battery: None,
        },
    )
    .unwrap();
    let s = b.snapshot();
    assert_eq!(
        s.devices
            .iter()
            .find(|d| d.key == GPU)
            .unwrap()
            .shutdown_mode
            .as_deref(),
        Some("off")
    );
    // Give the session a frame to take the new mode, then stop: the GPU goes static black.
    let frames = s.devices.iter().find(|d| d.key == GPU).unwrap().frames;
    wait_for(b, "a frame after the mode change", |s| {
        s.devices.iter().any(|d| d.key == GPU && d.frames > frames + 1)
    });
    handle.shutdown();
    let sent = rec.payloads(TransactionKind::SmbusWrite);
    let tail: Vec<[u8; 2]> = sent[sent.len() - 5..].iter().map(|w| [w[1], w[2]]).collect();
    assert_eq!(
        tail,
        [[0x26, 0], [0x30, 0], [0x31, 0], [0x32, 0], [0x22, 0x13]],
        "Shutdown(false, \"off\")"
    );
    assert_eq!(commits(), 1, "exit modes never store anything");
    drop(shared);
    let _ = std::fs::remove_dir_all(&home);
}
