// SPDX-License-Identifier: GPL-3.0-or-later
//! Profiles and live hardware modes through the engine: save the whole setup,
//! change it, apply the profile back; rename, duplicate, delete, export and import;
//! the default profile at the next start; `profile/apply` (and `layout/apply` of a
//! profile name) in the action router; and on the MSI GPU plugin over a mock bus,
//! a hardware mode chosen, kept in a profile and dropped again, without a single
//! write of the save register.

use lightloom_api::model::{DeviceHardwareMode, LedEdit};
use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineHandle, EngineOptions, HardwareMode};
use lightloom_daemon::paths::{self, Paths};
use lightloom_engine::{Placement, Point};
use lightloom_hal::mock::{MockHid, MockNotifier};
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::session::SessionDevice;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:4d4d:D";

const PLUGIN: &str = r#"
export function Name() { return "Profile probe"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4D4D; }
export function Size() { return [2, 1]; }
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

fn start(home: &Path) -> EngineHandle {
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("probe.js"), PLUGIN).unwrap();
    let info = MockHid::endpoint_info(0x1234, 0x4D4D, "D", 0, 0xFF00, 1, 0, ReportLengths::default());
    let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new().add_endpoint(info.clone()));
    engine::start(EngineOptions {
        paths: Paths::under(home),
        config: Config::default(),
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend,
            devices: vec![SessionDevice {
                usb: None,
                smbus: None,
                lamparray: None,
                serial: None,
                key: KEY.into(),
                vid: 0x1234,
                pid: 0x4D4D,
                endpoints: vec![info],
                network: None,
            }],
            smbus: Vec::new(),
            lamparrays: None,
        },
        // A file database: the second start reads what the first one saved.
        ephemeral: false,
        plugins_dir: Some(plugins),
        effects_dir: paths::resource_dir("effects", "effects"),
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

fn running(b: &dyn Backend) -> Arc<Snapshot> {
    wait_for(b, "the device rendering", |s| {
        s.state == "Running"
            && s.devices
                .iter()
                .any(|d| d.key == KEY && d.state == "running" && d.frames > 2)
    })
}

fn param(s: &Snapshot) -> Value {
    s.devices.iter().find(|d| d.key == KEY).unwrap().parameters[0]["value"].clone()
}

fn set_brightness(b: &dyn Backend, v: u8) {
    call(
        b,
        Request::SetLighting {
            brightness: Some(v),
            enabled: None,
        },
    )
    .unwrap();
}

fn set_mode(b: &dyn Backend, v: &str) {
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: None,
            parameters: Some(json!({ "mode": v }).as_object().unwrap().clone()),
            shutdown_mode: None,
            blink_low_battery: None,
        },
    )
    .unwrap();
}

fn place(b: &dyn Backend, x: f32) {
    call(
        b,
        Request::SetPlacement {
            uid: KEY.into(),
            placement: Placement {
                position: Point { x, y: 10.0 },
                ..Placement::default()
            },
        },
    )
    .unwrap();
}

fn leds(b: &dyn Backend, fill: Option<&str>, mode: &str) {
    // A restarted session publishes its endpoints after Initialize().
    wait_for(b, "the endpoints", |s| {
        s.devices
            .iter()
            .any(|d| d.key == KEY && d.state == "running" && !d.endpoints.is_empty())
    });
    call(
        b,
        Request::SetLeds {
            key: KEY.into(),
            uid: KEY.into(),
            edit: LedEdit {
                fill: fill.map(String::from),
                mode: Some(mode.into()),
                ..LedEdit::default()
            },
        },
    )
    .unwrap();
}

fn custom_flag(b: &dyn Backend) -> bool {
    b.snapshot()
        .devices
        .iter()
        .find(|d| d.key == KEY)
        .and_then(|d| d.endpoints.first())
        .is_some_and(|e| e.custom_leds)
}

fn placement_x(b: &dyn Backend) -> f64 {
    let layout = b.snapshot().lighting.layout.clone();
    let l = call(b, Request::GetLayout { name: layout }).unwrap();
    l["endpoints"][KEY]["position"]["x"].as_f64().unwrap()
}

#[test]
fn profiles_save_apply_and_manage_the_whole_setup() {
    let home = std::env::temp_dir().join(format!("lightloom-profiles-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let handle = start(&home);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    running(b);

    // The setup: an effect with a value, brightness, a plugin parameter, a placement,
    // a channel component and an exit mode.
    call(
        b,
        Request::ApplyEffect {
            id: "lightloom:plasma".into(),
            values: json!({"speed": 70}).as_object().unwrap().clone(),
        },
    )
    .unwrap();
    set_brightness(b, 40);
    set_mode(b, "Forced");
    place(b, 33.0);
    call(
        b,
        Request::SetChannel {
            device: KEY.into(),
            channel: "ARGB".into(),
            components: vec![json!("Default Strip - 5")],
        },
    )
    .unwrap();
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: None,
            parameters: None,
            shutdown_mode: Some("off".into()),
            blink_low_battery: None,
        },
    )
    .unwrap();
    leds(b, Some("#00ff00"), "custom");
    assert_eq!(
        call(
            b,
            Request::SaveProfile {
                name: " Evening ".into()
            }
        )
        .unwrap()["profile"],
        "Evening"
    );
    let s = b.snapshot();
    assert_eq!(s.lighting.profile.as_deref(), Some("Evening"));
    assert_eq!(s.profiles.len(), 1);
    assert_eq!(s.profiles[0].effect.as_deref(), Some("lightloom:plasma"));
    assert_eq!(s.profiles[0].devices, 1);
    let doc = call(
        b,
        Request::GetProfile {
            name: "Evening".into(),
        },
    )
    .unwrap();
    assert_eq!(doc["format"], "lightloom-profile");
    assert_eq!(doc["lighting"]["brightness"], 40);
    assert_eq!(doc["lighting"]["values"]["speed"], 70);
    assert_eq!(doc["devices"][KEY]["parameters"]["mode"], "Forced");
    assert_eq!(doc["devices"][KEY]["shutdownMode"], "off");
    assert_eq!(
        doc["devices"][KEY]["channels"]["ARGB"][0]["ProductName"],
        "Default Strip - 5"
    );
    assert_eq!(doc["layout"]["endpoints"][KEY]["position"]["x"], 33.0);
    assert_eq!(doc["devices"][KEY]["customLeds"][KEY]["enabled"], true);
    assert_eq!(doc["devices"][KEY]["customLeds"][KEY]["colors"][0], "#00ff00");

    // Change everything, then apply the profile back.
    call(
        b,
        Request::ApplyEffect {
            id: engine::DEFAULT_EFFECT.into(),
            values: Default::default(),
        },
    )
    .unwrap();
    set_brightness(b, 90);
    set_mode(b, "Canvas");
    place(b, 77.0);
    leds(b, Some("#000000"), "effect");
    assert!(!custom_flag(b));
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: None,
            parameters: None,
            shutdown_mode: Some("default".into()),
            blink_low_battery: None,
        },
    )
    .unwrap();
    let r = call(
        b,
        Request::ApplyProfile {
            name: "Evening".into(),
        },
    )
    .unwrap();
    assert_eq!(r["notes"], json!([]), "{r}");
    let s = b.snapshot();
    assert_eq!(s.lighting.effect.as_deref(), Some("lightloom:plasma"));
    assert_eq!(s.lighting.brightness, 40);
    assert_eq!(param(&s), "Forced");
    let d = s.devices.iter().find(|d| d.key == KEY).unwrap();
    assert_eq!(d.shutdown_mode.as_deref(), Some("off"));
    assert_eq!(placement_x(b), 33.0);
    assert!(custom_flag(b), "the custom LEDs are back");
    assert!(s
        .effects
        .iter()
        .any(|e| e.id == "lightloom:plasma" && e.values["speed"] == 70));
    running(b);

    // A device the profile names but Lightloom has never seen is skipped and reported;
    // unknown per-device keys travel through import and export unchanged.
    let mut imported = doc.clone();
    imported["devices"]["dead:beef:00"] = json!({"enabled": true});
    imported["devices"][KEY]["laterSetting"] = json!({"0": "#ff0000"});
    let r = call(
        b,
        Request::ImportProfile {
            profile: imported,
            name: None,
        },
    )
    .unwrap();
    assert_eq!(r["profile"], "Evening (2)");
    let copy = call(
        b,
        Request::GetProfile {
            name: "Evening (2)".into(),
        },
    )
    .unwrap();
    assert_eq!(copy["devices"][KEY]["laterSetting"]["0"], "#ff0000");
    let r = call(
        b,
        Request::ApplyProfile {
            name: "Evening (2)".into(),
        },
    )
    .unwrap();
    assert!(r["notes"][0].as_str().unwrap().contains("dead:beef:00"), "{r}");
    let bad = call(
        b,
        Request::ImportProfile {
            profile: json!({"format": "something", "version": 1, "name": "x", "lighting": {"brightness": 1}, "layout": {"name": "x"}}),
            name: None,
        },
    )
    .unwrap_err();
    assert_eq!(bad.status, 400);

    // Rename, duplicate, delete; the current and default names follow.
    assert_eq!(
        call(
            b,
            Request::DuplicateProfile {
                name: "Evening".into(),
                to: "Evening (2)".into()
            }
        )
        .unwrap_err()
        .status,
        409
    );
    call(
        b,
        Request::DuplicateProfile {
            name: "Evening".into(),
            to: "Night".into(),
        },
    )
    .unwrap();
    call(
        b,
        Request::SetDefaultProfile {
            name: Some("Evening".into()),
        },
    )
    .unwrap();
    call(
        b,
        Request::RenameProfile {
            name: "Evening".into(),
            to: "Dusk".into(),
        },
    )
    .unwrap();
    let s = b.snapshot();
    assert_eq!(s.settings.default_profile.as_deref(), Some("Dusk"));
    let names: Vec<&str> = s.profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Dusk", "Evening (2)", "Night"]);
    assert_eq!(
        call(
            b,
            Request::RenameProfile {
                name: "Night".into(),
                to: "Dusk".into()
            }
        )
        .unwrap_err()
        .status,
        409
    );
    assert_eq!(
        call(b, Request::DeleteProfile { name: "Nope".into() })
            .unwrap_err()
            .status,
        404
    );
    assert_eq!(
        call(b, Request::ApplyProfile { name: "Nope".into() })
            .unwrap_err()
            .status,
        404
    );
    assert_eq!(
        call(
            b,
            Request::SetDefaultProfile {
                name: Some("Nope".into())
            }
        )
        .unwrap_err()
        .status,
        404
    );
    call(
        b,
        Request::DeleteProfile {
            name: "Evening (2)".into(),
        },
    )
    .unwrap();
    assert_eq!(
        b.snapshot().lighting.profile,
        None,
        "the current profile was deleted"
    );

    // The action router: profile/apply, and layout/apply of a name that is only a profile.
    set_brightness(b, 90);
    let r = call(
        b,
        Request::Action {
            url: "lightloom://profile/apply/Night".into(),
        },
    )
    .unwrap();
    assert_eq!(r["action"], "profile/apply", "{r}");
    assert_eq!(b.snapshot().lighting.brightness, 40);
    set_brightness(b, 90);
    call(
        b,
        Request::Action {
            url: "lightloom://layout/apply/Dusk".into(),
        },
    )
    .unwrap();
    assert_eq!(b.snapshot().lighting.profile.as_deref(), Some("Dusk"));
    assert_eq!(b.snapshot().lighting.brightness, 40);
    assert_eq!(
        call(
            b,
            Request::Action {
                url: "lightloom://profile/apply/Nope".into()
            }
        )
        .unwrap_err()
        .status,
        404
    );

    // The whole-state export carries the profiles.
    let state = call(b, Request::Export).unwrap();
    assert_eq!(state["profiles"].as_array().unwrap().len(), 2);

    // The default profile is applied at the next start.
    set_brightness(b, 90);
    set_mode(b, "Canvas");
    handle.shutdown();
    drop(shared);
    let handle = start(&home);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let s = running(b);
    assert_eq!(s.lighting.brightness, 40, "Dusk's brightness");
    assert_eq!(s.lighting.profile.as_deref(), Some("Dusk"));
    assert_eq!(param(&s), "Forced");
    handle.shutdown();
    drop(shared);
    let _ = std::fs::remove_dir_all(&home);
}

/// A hardware mode on the MSI GPU plugin over a mock bus: chosen, refused when it
/// does not fit, kept in a profile, dropped and applied back; 0x3F never written.
#[test]
fn hardware_modes_reach_the_device_and_travel_in_profiles() {
    use lightloom_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
    use lightloom_hal::{PciIds, SmbusBusKind};
    use lightloom_plugin_host::smbus::BusHandle;

    let home = std::env::temp_dir().join(format!("lightloom-hwmode-{}", std::process::id()));
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
            lamparrays: None,
        },
        ephemeral: true,
        plugins_dir: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")),
        effects_dir: None,
        components_dir: None,
        signalrgb_settings: None,
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
    let gpu = |s: &Snapshot| s.devices.iter().find(|d| d.key == GPU).cloned();
    let s = wait_for(b, "the GPU rendering", |s| {
        gpu(s).is_some_and(|d| d.state == "running" && d.frames > 2)
    });
    let d = gpu(&s).unwrap();
    assert_eq!(d.hardware_modes.len(), 19);
    assert!(d.hardware_mode.is_none());
    let mode_writes = || {
        rec.payloads(TransactionKind::SmbusWrite)
            .iter()
            .filter(|w| w[..2] == [0x68, 0x22])
            .map(|w| w[2])
            .collect::<Vec<u8>>()
    };

    // Refused: no such mode, a bad colour, an unknown device.
    let set = |mode: Option<DeviceHardwareMode>| {
        call(
            b,
            Request::SetHardwareMode {
                key: GPU.into(),
                mode,
            },
        )
    };
    let e = set(Some(DeviceHardwareMode {
        id: "disco".into(),
        ..Default::default()
    }))
    .unwrap_err();
    assert_eq!(e.status, 400, "{e}");
    let e = set(Some(DeviceHardwareMode {
        id: "breathing".into(),
        colors: vec!["blue".into()],
        ..Default::default()
    }))
    .unwrap_err();
    assert_eq!(e.status, 400, "{e}");
    let e = call(
        b,
        Request::SetHardwareMode {
            key: "nope".into(),
            mode: None,
        },
    )
    .unwrap_err();
    assert_eq!(e.status, 404);

    // Rainbow: the mode reaches the card, frames stop, the choice shows (completed).
    let r = set(Some(DeviceHardwareMode {
        id: "rainbow".into(),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(r["applied"], true, "{r}");
    assert_eq!(*mode_writes().last().unwrap(), 0x08);
    let s = b.snapshot();
    let chosen = gpu(&s).unwrap().hardware_mode.unwrap();
    assert_eq!((chosen.speed, chosen.brightness), (Some(50), Some(100)));
    // The counters refresh once a second.
    std::thread::sleep(Duration::from_millis(1300));
    let frames = gpu(&b.snapshot()).unwrap().frames;
    std::thread::sleep(Duration::from_millis(1300));
    assert_eq!(
        gpu(&b.snapshot()).unwrap().frames,
        frames,
        "no frames in a hardware mode"
    );

    // In a profile; Follow effect; the profile brings the mode back.
    call(
        b,
        Request::SaveProfile {
            name: "Rainbow GPU".into(),
        },
    )
    .unwrap();
    let doc = call(
        b,
        Request::GetProfile {
            name: "Rainbow GPU".into(),
        },
    )
    .unwrap();
    assert_eq!(doc["devices"][GPU]["hardwareMode"]["id"], "rainbow");
    set(None).unwrap();
    assert_eq!(*mode_writes().last().unwrap(), 0x13, "Direct again");
    wait_for(b, "frames again", |s| gpu(s).is_some_and(|d| d.frames > frames));
    assert!(gpu(&b.snapshot()).unwrap().hardware_mode.is_none());
    call(
        b,
        Request::ApplyProfile {
            name: "Rainbow GPU".into(),
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while *mode_writes().last().unwrap() != 0x08 {
        assert!(
            Instant::now() < deadline,
            "the profile's mode never reached the card"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(gpu(&b.snapshot()).unwrap().hardware_mode.unwrap().id, "rainbow");

    handle.shutdown();
    drop(shared);
    assert!(
        rec.payloads(TransactionKind::SmbusWrite)
            .iter()
            .all(|w| w[1] != 0x3F),
        "the save register is never written"
    );
    let _ = std::fs::remove_dir_all(&home);
}
