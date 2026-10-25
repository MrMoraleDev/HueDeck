// SPDX-License-Identifier: GPL-3.0-or-later
//! LCD faces end to end, without a renderer (the mock path): a plugin initializes its LCD,
//! the device view shows it with the default face, `getFrame` streams the face to the
//! device, `SetLcd` changes face, frame rate and face control values (validated, stored),
//! the preview endpoint's PNG is the face frame, and the face library holds the starter faces.

use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineOptions, HardwareMode};
use lightloom_daemon::paths::Paths;
use lightloom_hal::mock::{MockHid, MockNotifier};
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::session::SessionDevice;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:1cd1:L";

const PLUGIN: &str = r#"
import LCD from "@SignalRGB/lcd";
export function Name() { return "LCD cooler"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x1CD1; }
export function Size() { return [1, 1]; }
/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() { LCD.initialize({ width: 8, height: 4, circular: true }); }
export function Render() {
  const frame = LCD.getFrame({ format: "RGB" });
  device.write([0x01].concat(frame.slice(0, 12)), 13);
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

fn set_lcd(b: &dyn Backend, face: Option<&str>, framerate: Option<u32>) -> Result<Value, ApiError> {
    call(
        b,
        Request::SetLcd {
            key: KEY.into(),
            face: face.map(String::from),
            framerate,
            values: None,
        },
    )
}

fn set_values(b: &dyn Backend, values: Value) -> Result<Value, ApiError> {
    call(
        b,
        Request::SetLcd {
            key: KEY.into(),
            face: None,
            framerate: None,
            values: values.as_object().cloned(),
        },
    )
}

#[test]
fn lcd_faces_attach_stream_and_change() {
    let home = std::env::temp_dir().join(format!("lightloom-lcd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("lcd.js"), PLUGIN).unwrap();

    let info = MockHid::endpoint_info(0x1234, 0x1CD1, "L", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let handle = engine::start(EngineOptions {
        paths: Paths::under(&home),
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
                pid: 0x1CD1,
                endpoints: vec![info],
                network: None,
            }],
            smbus: Vec::new(),
            lamparrays: None,
        },
        ephemeral: true,
        plugins_dir: Some(plugins),
        effects_dir: None,
        components_dir: None,
        signalrgb_settings: None,
        renderer: None,
        plugin_cache: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;

    let s = wait_for(b, "the LCD", |s| {
        s.devices.iter().any(|d| d.key == KEY && d.lcd.is_some())
    });
    let faces: Vec<&str> = s.lcd_faces.iter().map(|f| f.id.as_str()).collect();
    assert!(
        faces.contains(&"lightloom:clock") && faces.contains(&"lightloom:sensor"),
        "{faces:?}"
    );
    assert!(s
        .lcd_faces
        .iter()
        .any(|f| f.id == "lightloom:sensor" && f.uses_sensors));
    let lcd = s
        .devices
        .iter()
        .find(|d| d.key == KEY)
        .unwrap()
        .lcd
        .clone()
        .unwrap();
    assert_eq!((lcd.width, lcd.height, lcd.circular), (8, 4, true));
    assert_eq!(
        (lcd.face.as_deref(), lcd.framerate),
        (Some("lightloom:clock"), 30)
    );
    assert_eq!(lcd.renderer, "mock", "no renderer: the test pattern stands in");

    // The face (a rainbow test pattern here) reaches the device: 4 RGB pixels, not black.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !mock
        .recorder()
        .writes()
        .iter()
        .any(|w| w.len() == 13 && w[1..].iter().any(|&b| b > 0))
    {
        assert!(Instant::now() < deadline, "no face frames written");
        std::thread::sleep(Duration::from_millis(20));
    }

    assert_eq!(
        set_lcd(b, Some("lightloom:nope"), None).unwrap_err().code,
        "not_found"
    );
    assert_eq!(set_lcd(b, None, Some(0)).unwrap_err().code, "bad_request");
    let r = set_lcd(b, Some("lightloom:sensor"), Some(12)).unwrap();
    assert_eq!(r["face"], "lightloom:sensor");
    assert_eq!(r["framerate"], 12);
    let s = wait_for(b, "the new face", |s| {
        s.devices
            .iter()
            .find(|d| d.key == KEY)
            .and_then(|d| d.lcd.as_ref())
            .is_some_and(|l| l.face.as_deref() == Some("lightloom:sensor") && l.framerate == 12)
    });
    assert!(s.devices[0].lcd.as_ref().unwrap().running);
    let r = set_lcd(b, Some(""), None).unwrap();
    assert_eq!(
        (r["face"].clone(), r["framerate"].clone()),
        (Value::Null, Value::from(12))
    );

    // Control values of the face shown, checked against its controls and kept per face.
    let clock = s.lcd_faces.iter().find(|f| f.id == "lightloom:clock").unwrap();
    assert!(clock.controls.iter().any(|c| c["property"] == "hour24"));
    assert_eq!(
        set_values(b, serde_json::json!({"nope": 1})).unwrap_err().code,
        "bad_request"
    );
    let r = set_values(b, serde_json::json!({"hour24": false, "textColor": "#ff8800"})).unwrap();
    assert_eq!(
        r["values"],
        serde_json::json!({"hour24": false, "textColor": "#ff8800"})
    );
    wait_for(b, "the values in the view", |s| {
        s.devices
            .iter()
            .find(|d| d.key == KEY)
            .and_then(|d| d.lcd.as_ref())
            .is_some_and(|l| l.values.get("hour24") == Some(&Value::Bool(false)))
    });

    // The dashboard preview: the face frame as a PNG at the LCD's size.
    let png = b.lcd_preview(KEY).expect("a preview while the LCD runs");
    assert_eq!(&png[1..4], b"PNG");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 8, "width");
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 4, "height");
    assert!(b.lcd_preview("no:such:device").is_none());
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
