// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-LED custom colours end to end: a scripted keyboard on a mock HID
//! device is put in "custom LEDs" mode, painted, given a zone, dimmed by the
//! global brightness, and keeps its colours across a daemon restart.

use lightloom_api::model::LedEdit;
use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineHandle, EngineOptions, HardwareMode};
use lightloom_daemon::paths::{self, Paths};
use lightloom_hal::mock::{MockHid, MockNotifier};
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::session::SessionDevice;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:4b4c:K";

const PLUGIN: &str = r#"
export function Name() { return "Custom LED probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4B4C; }
export function DeviceType() { return "keyboard"; }
export function Size() { return [3, 1]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Esc", "F1", "F2"]; }
export function LedPositions() { return [[0, 0], [1, 0], [2, 0]]; }
/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
export function Render() {
  const out = [0x01];
  for (let x = 0; x < 3; x++) out.push(...device.color(x, 0));
  device.write(out, 10);
}
export function Shutdown() {}
"#;

fn call(backend: &dyn Backend, request: Request) -> Result<Value, ApiError> {
    backend.call(request).blocking_recv().expect("engine answers")
}

fn wait_for(backend: &dyn Backend, what: &str, mut done: impl FnMut(&Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done(&backend.snapshot()) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Wait until the device was last sent `colors` (three LEDs).
fn wait_for_wire(mock: &MockHid, what: &str, colors: [u8; 9]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let writes = mock.recorder().writes();
        if writes.last().is_some_and(|w| w.len() >= 10 && w[1..10] == colors) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{what}: the device never got {colors:?} (last {:?})",
            writes.last()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn start(home: &Path, mock: &Arc<MockHid>) -> EngineHandle {
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("probe.js"), PLUGIN).unwrap();
    let info = MockHid::endpoint_info(0x1234, 0x4B4C, "K", 0, 0xFF00, 1, 0, ReportLengths::default());
    let backend: Arc<dyn HidBackend> = mock.clone();
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
                pid: 0x4B4C,
                endpoints: vec![info],
                network: None,
            }],
            smbus: Vec::new(),
            lamparrays: None,
        },
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

fn mock() -> Arc<MockHid> {
    let info = MockHid::endpoint_info(0x1234, 0x4B4C, "K", 0, 0xFF00, 1, 0, ReportLengths::default());
    Arc::new(MockHid::new().add_endpoint(info))
}

fn running(b: &dyn Backend) {
    wait_for(b, "the keyboard rendering", |s| {
        s.devices
            .iter()
            .any(|d| d.key == KEY && d.state == "running" && d.frames > 2)
    });
}

fn edit(f: impl FnOnce(&mut LedEdit)) -> Request {
    let mut e = LedEdit::default();
    f(&mut e);
    Request::SetLeds {
        key: KEY.into(),
        uid: KEY.into(),
        edit: e,
    }
}

fn custom_flag(b: &dyn Backend) -> bool {
    b.snapshot()
        .devices
        .iter()
        .find(|d| d.key == KEY)
        .and_then(|d| d.endpoints.first())
        .is_some_and(|e| e.custom_leds)
}

#[test]
fn custom_leds_paint_the_device_and_survive_a_restart() {
    let home = std::env::temp_dir().join(format!("lightloom-custom-leds-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);

    let mock1 = mock();
    let handle = start(&home, &mock1);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    running(b);

    // The painter's view: the LEDs at their cells with their names, following the effect.
    let map = call(b, Request::LedMap { key: KEY.into() }).unwrap();
    assert_eq!(map["deviceType"], "keyboard");
    let ep = &map["endpoints"][0];
    assert_eq!(
        (ep["uid"].as_str(), ep["kind"].as_str()),
        (Some(KEY), Some("device"))
    );
    assert_eq!(
        (ep["slots"].as_u64(), ep["custom"].as_bool()),
        (Some(3), Some(false))
    );
    assert_eq!(
        ep["leds"][1],
        serde_json::json!({"slot": 1, "x": 1, "y": 0, "name": "F1"})
    );
    assert_eq!(ep["colors"], serde_json::json!(["#000000", "#000000", "#000000"]));
    assert!(!custom_flag(b));

    // Fill, one key, custom mode: the plugin's device.color answers from the map.
    let r = call(
        b,
        edit(|e| {
            e.fill = Some("#ff0000".into());
            e.set = BTreeMap::from([(1, "#0000ff".into())]);
            e.mode = Some("custom".into());
        }),
    )
    .unwrap();
    assert_eq!(
        (r["custom"].as_bool(), r["changed"].as_bool()),
        (Some(true), Some(true))
    );
    wait_for_wire(&mock1, "painted", [255, 0, 0, 0, 0, 255, 255, 0, 0]);
    assert!(custom_flag(b), "the snapshot shows the mode");
    let r = call(b, edit(|e| e.set = BTreeMap::from([(1, "#0000ff".into())]))).unwrap();
    assert_eq!(r["changed"], false);

    // Brightness still applies (global x device).
    call(
        b,
        Request::SetLighting {
            brightness: Some(50),
            enabled: None,
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let last = mock1.recorder().writes().last().cloned().unwrap_or_default();
        if last.len() >= 10
            && (127..=128).contains(&last[1])
            && last[2] == 0
            && (127..=128).contains(&last[6])
        {
            break;
        }
        assert!(Instant::now() < deadline, "never dimmed: {last:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
    call(
        b,
        Request::SetLighting {
            brightness: Some(100),
            enabled: None,
        },
    )
    .unwrap();

    // A zone, coloured at once.
    let z = call(
        b,
        Request::SaveLedZone {
            key: KEY.into(),
            uid: KEY.into(),
            name: "F keys".into(),
            leds: vec![2, 1],
        },
    )
    .unwrap();
    assert_eq!(z["zones"]["F keys"], serde_json::json!([1, 2]));
    call(
        b,
        edit(|e| e.zones = BTreeMap::from([("F keys".into(), "#00ff00".into())])),
    )
    .unwrap();
    wait_for_wire(&mock1, "zone", [255, 0, 0, 0, 255, 0, 0, 255, 0]);

    // Refusals.
    let status = |r: Request| call(b, r).unwrap_err().status;
    assert_eq!(
        status(edit(|e| e.set = BTreeMap::from([(3, "#ffffff".into())]))),
        400
    );
    assert_eq!(status(edit(|e| e.fill = Some("red".into()))), 400);
    assert_eq!(status(edit(|e| e.mode = Some("rainbow".into()))), 400);
    assert_eq!(
        status(edit(
            |e| e.zones = BTreeMap::from([("nope".into(), "#ffffff".into())])
        )),
        404
    );
    assert_eq!(
        status(Request::SetLeds {
            key: KEY.into(),
            uid: format!("{KEY}:nope"),
            edit: LedEdit::default(),
        }),
        404
    );
    assert_eq!(
        status(Request::LedMap {
            key: "0:0:none".into()
        }),
        409
    );
    assert_eq!(
        status(Request::DeleteLedZone {
            key: KEY.into(),
            uid: KEY.into(),
            name: "nope".into(),
        }),
        404
    );
    // The export carries them (pending edits are saved first).
    let export = call(b, Request::Export).unwrap();
    assert_eq!(export["customLeds"][KEY]["colors"][1], "#00ff00");
    handle.shutdown();

    // A restart: the colours, the mode and the zone come back from the database.
    let mock2 = mock();
    let handle = start(&home, &mock2);
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    running(b);
    wait_for_wire(&mock2, "restored", [255, 0, 0, 0, 255, 0, 0, 255, 0]);
    let map = call(b, Request::LedMap { key: KEY.into() }).unwrap();
    let ep = &map["endpoints"][0];
    assert_eq!(ep["custom"], true);
    assert_eq!(ep["zones"]["F keys"], serde_json::json!([1, 2]));

    // Back to the effect: the canvas drives the keyboard again, the colours are kept.
    call(b, edit(|e| e.mode = Some("effect".into()))).unwrap();
    assert!(!custom_flag(b));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let last = mock2.recorder().writes().last().cloned().unwrap_or_default();
        if last.len() >= 10 && last[1..10] != [255, 0, 0, 0, 255, 0, 0, 255, 0] {
            break;
        }
        assert!(Instant::now() < deadline, "the effect never came back");
        std::thread::sleep(Duration::from_millis(10));
    }
    call(
        b,
        Request::DeleteLedZone {
            key: KEY.into(),
            uid: KEY.into(),
            name: "F keys".into(),
        },
    )
    .unwrap();
    let map = call(b, Request::LedMap { key: KEY.into() }).unwrap();
    assert_eq!(map["endpoints"][0]["colors"][0], "#ff0000");
    assert!(map["endpoints"][0]["zones"].as_object().unwrap().is_empty());
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
