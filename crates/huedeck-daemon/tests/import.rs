// SPDX-License-Identifier: GPL-3.0-or-later
//! The SignalRGB import wizard against a settings fixture (never the registry):
//! the preview changes nothing; the import brings in the layout with its
//! brightness and per-device placement, channel components, effect values and
//! presets, the global brightness and per-device enable.

use huedeck_api::{ApiError, Backend, Request, Snapshot};
use huedeck_daemon::config::Config;
use huedeck_daemon::engine::{self, EngineOptions, HardwareMode};
use huedeck_daemon::paths::Paths;
use huedeck_hal::mock::{MockHid, MockNotifier};
use huedeck_hal::{HidBackend, ReportLengths};
use huedeck_plugin_host::session::SessionDevice;
use huedeck_runtime::signalrgb::Fixture;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:4d4d:D";

const PLUGIN: &str = r#"
export function Name() { return "Import probe"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4D4D; }
export function Size() { return [2, 1]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["A", "B"]; }
export function LedPositions() { return [[0, 0], [1, 0]]; }
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

/// QSettings' QColor `@Variant`: one character per stream byte (HSL red).
fn red() -> String {
    let bytes = [0u8, 0, 0, 0x43, 4, 0xff, 0xff, 0, 0, 0xff, 0xff, 0x80, 0, 0, 0];
    let mut s = String::from("@Variant(");
    s.extend(bytes.iter().map(|&b| char::from(b)));
    s.push(')');
    s
}

fn fixture() -> Fixture {
    let strip = json!([{
        "Brand": "Generic", "ComponentId": "0f6a3c1e-2b4d-4e5f-8a9b-1c2d3e4f5a6b",
        "DisplayName": "Strip - 4 LED", "ProductName": "Fixture Strip 4", "Type": "Strip",
        "LedCount": 4, "Width": 4, "Height": 1, "LedMapping": [0, 1, 2, 3],
        "LedCoordinates": [[0, 0], [1, 0], [2, 0], [3, 0]],
        "LedNames": ["Led 1", "Led 2", "Led 3", "Led 4"]
    }]);
    let mut f = Fixture::new();
    f.set("layouts", "brightness", "70");
    f.set("layouts", "gridSize", "4");
    f.set("lighting", "globalBrightness", "55");
    f.set(&format!(r"lighting\endpoint\{KEY}"), "enabled", "false");
    f.set(&format!(r"lighting\endpoint\{KEY}"), "brightness", "40");
    f.set(&format!(r"lighting\endpoint\{KEY}\position"), "x", "24");
    f.set(&format!(r"lighting\endpoint\{KEY}\position"), "y", "61");
    f.set(&format!(r"devices\{KEY}"), "ARGB", &strip.to_string());
    f.set(r"effects\selected", "id", "Side To Side.html");
    f.set(r"effects\Side To Side.html", "speed", "2");
    f.set(r"effects\Side To Side.html", "color1", &red());
    f.set(r"effects\Side To Side.html", "-RemoteFileByteSize", "100");
    f.set(r"effects\Side To Side.html\savedStateA", "speed", "9");
    f
}

#[test]
fn the_signalrgb_import_previews_then_brings_in_the_whole_setup() {
    let home = std::env::temp_dir().join(format!("huedeck-import-{}", std::process::id()));
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
                network: None,
                lamparray: None,
                usb: None,
                key: KEY.into(),
                vid: 0x1234,
                pid: 0x4D4D,
                endpoints: vec![info],
            }],
            smbus: Vec::new(),
            lamparrays: None,
        },
        ephemeral: true,
        plugins_dir: Some(plugins),
        effects_dir: None,
        components_dir: None,
        signalrgb_settings: Some(Arc::new(fixture())),
        renderer: None,
        plugin_cache: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: huedeck_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    wait_for(b, "the device rendering", |s| {
        s.devices
            .iter()
            .any(|d| d.key == KEY && d.state == "running" && d.frames > 1)
    });
    let device = b
        .snapshot()
        .devices
        .iter()
        .find(|d| d.key == KEY)
        .cloned()
        .unwrap();
    assert!(!device.third_party, "a first-party plugin");

    // The preview: what would come in, nothing changed.
    let preview = call(b, Request::PreviewSignalRgb { name: None }).unwrap();
    assert_eq!(preview["available"], true);
    assert_eq!(preview["layout"], "SignalRGB");
    assert_eq!(preview["placements"], 1);
    assert_eq!(preview["layoutBrightness"], 70);
    assert_eq!(preview["globalBrightness"], 55);
    assert_eq!(preview["selectedEffect"]["id"], "signalrgb:Side To Side");
    assert_eq!(preview["selectedEffect"]["available"], false);
    assert_eq!(preview["effects"][0]["values"], 2, "{preview:#}");
    assert_eq!(preview["effects"][0]["presets"], json!(["savedStateA"]));
    assert_eq!(preview["effects"][0]["thirdParty"], true);
    assert_eq!(
        preview["channelSetups"],
        json!([{ "device": KEY, "channel": "ARGB", "components": 1, "leds": 4 }])
    );
    assert_eq!(
        preview["devices"],
        json!([{ "key": KEY, "name": "Import probe", "known": true, "enabled": false, "brightness": 40 }])
    );
    let s = b.snapshot();
    assert_eq!(
        (s.lighting.brightness, s.lighting.layout.as_str()),
        (100, "Default")
    );
    let export = call(b, Request::Export).unwrap();
    assert_eq!(export["effects"], json!({}), "the preview wrote nothing");

    // The import.
    let reply = call(b, Request::ImportSignalRgb { name: None }).unwrap();
    assert_eq!(reply["imported"], true);
    assert_eq!(
        (reply["valuesImported"].clone(), reply["presetsImported"].clone()),
        (json!(2), json!(1))
    );
    assert!(
        reply["notes"].as_array().unwrap().iter().any(|n| n
            .as_str()
            .unwrap()
            .contains("signalrgb:Side To Side is not in the library")),
        "{reply:#}"
    );

    let s = wait_for(b, "the import applied", |s| {
        s.lighting.layout == "SignalRGB"
            && s.lighting.brightness == 55
            && s.devices.iter().any(|d| d.key == KEY && !d.enabled)
    });
    assert_eq!(s.lighting.layout_brightness, 70);
    let layout = call(
        b,
        Request::GetLayout {
            name: "SignalRGB".into(),
        },
    )
    .unwrap();
    assert_eq!(layout["endpoints"][KEY]["brightness"], 40, "{layout:#}");
    assert_eq!(layout["endpoints"][KEY]["enabled"], false);

    let export = call(b, Request::Export).unwrap();
    let side = &export["effects"]["signalrgb:Side To Side"];
    assert_eq!(side["params"], json!({ "color1": "#ff0000", "speed": 2 }));
    assert_eq!(side["presets"]["savedStateA"], json!({ "speed": 9 }));
    assert_eq!(
        export["channels"][KEY]["ARGB"][0]["ProductName"],
        "Fixture Strip 4"
    );

    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
