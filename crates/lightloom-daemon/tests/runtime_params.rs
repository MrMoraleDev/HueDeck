// SPDX-License-Identifier: GPL-3.0-or-later
//! Run-time plugin parameters and messages end to end: what a running plugin
//! adds and removes with `device.addProperty` / `addMessage` / `notify`
//! reaches the snapshot's device view, a value set from the dashboard for a
//! run-time property is coerced, stored and applied, and the device view
//! falls back to the exports while no session runs.

use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineOptions, HardwareMode};
use lightloom_daemon::paths::Paths;
use lightloom_hal::mock::{MockHid, MockNotifier};
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::session::SessionDevice;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:5a5a:P";

const PLUGIN: &str = r#"
export function Name() { return "Runtime props"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x5A5A; }
export function Size() { return [1, 1]; }
export function ControllableParameters() {
  return [{ property: "extra", label: "Extra", type: "boolean", default: "false", group: "lighting" }];
}
export function DeviceMessages() {
  return [{ property: "fw", message: "Old firmware", tooltip: "" }];
}
/* global device, extra, speed */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
let alert;
export function onextraChanged() {
  if (extra) {
    device.addProperty({ property: "speed", label: "Speed", type: "number", min: "1", max: "10", default: "5", live: "false", order: 2 });
    alert = device.notify("Extra on", "The extra setting is on", 1);
  } else {
    device.removeProperty("speed");
    device.denotify(alert);
  }
}
export function Render() {
  device.write([0x00, typeof speed === "number" ? speed : 0xff], 2);
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

fn set(b: &dyn Backend, values: Value) {
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: None,
            parameters: Some(values.as_object().unwrap().clone()),
            shutdown_mode: None,
            blink_low_battery: None,
        },
    )
    .unwrap();
}

fn names(s: &Snapshot) -> Vec<String> {
    s.devices
        .iter()
        .find(|d| d.key == KEY)
        .map(|d| {
            d.parameters
                .iter()
                .map(|p| p["property"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn messages(s: &Snapshot) -> Vec<String> {
    s.devices
        .iter()
        .find(|d| d.key == KEY)
        .map(|d| d.messages.iter().map(|m| m.id.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn runtime_properties_and_messages_reach_the_device_view() {
    let home = std::env::temp_dir().join(format!("lightloom-runtime-params-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("runtime_props.js"), PLUGIN).unwrap();

    let info = MockHid::endpoint_info(0x1234, 0x5A5A, "P", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let handle = engine::start(EngineOptions {
        paths: Paths::under(&home),
        config: Config::default(),
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend,
            devices: vec![SessionDevice::hid(KEY, 0x1234, 0x5A5A, vec![info])],
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

    let s = wait_for(b, "the device running", |s| {
        s.devices.iter().any(|d| d.key == KEY && d.state == "running")
    });
    assert_eq!(names(&s), ["extra"]);
    assert_eq!(messages(&s), ["fw"]);
    let d = s.devices.iter().find(|d| d.key == KEY).unwrap();
    assert_eq!(
        d.parameters[0]["default"],
        json!(false),
        "the boolean string is coerced"
    );
    assert_eq!(d.messages[0].message, "Old firmware");
    assert_eq!(d.messages[0].tooltip, None);

    // A string from a client is coerced to the boolean the control expects.
    set(b, json!({ "extra": "true" }));
    let s = wait_for(b, "the run-time property", |s| names(s) == ["extra", "speed"]);
    let d = s.devices.iter().find(|d| d.key == KEY).unwrap();
    assert_eq!(d.parameters[0]["value"], json!(true));
    let speed = &d.parameters[1];
    assert_eq!(
        (
            &speed["type"],
            &speed["min"],
            &speed["max"],
            &speed["default"],
            &speed["live"],
            &speed["order"],
            &speed["value"]
        ),
        (
            &json!("number"),
            &json!(1.0),
            &json!(10.0),
            &json!(5.0),
            &json!(false),
            &json!(2),
            &json!(5.0)
        )
    );
    let s = wait_for(b, "the notification", |s| messages(s).len() == 2);
    let d = s.devices.iter().find(|d| d.key == KEY).unwrap();
    let n = &d.messages[1];
    assert_eq!(
        (n.id.as_str(), n.title.as_deref(), n.message.as_str(), n.priority),
        (
            "lightloom-alert-1",
            Some("Extra on"),
            "The extra setting is on",
            Some(1)
        )
    );

    // A value for the run-time property: coerced, stored, applied.
    set(b, json!({ "speed": "7" }));
    let s = wait_for(b, "the stored speed", |s| {
        s.devices
            .iter()
            .find(|d| d.key == KEY)
            .is_some_and(|d| d.parameters.get(1).is_some_and(|p| p["value"] == json!(7.0)))
    });
    assert_eq!(names(&s), ["extra", "speed"]);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !mock.recorder().writes().iter().any(|w| w[..] == [0x00, 7]) {
        assert!(Instant::now() < deadline, "speed 7 never written");
        std::thread::sleep(Duration::from_millis(20));
    }

    // Off again: the property and the notification go away.
    set(b, json!({ "extra": false }));
    wait_for(b, "the property removed", |s| {
        names(s) == ["extra"] && messages(s) == ["fw"]
    });

    // Disabled: no session, the exports' lists.
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
    let s = wait_for(b, "the device disabled", |s| {
        s.devices.iter().any(|d| d.key == KEY && d.state == "disabled")
    });
    assert_eq!(
        (names(&s), messages(&s)),
        (vec!["extra".to_string()], vec!["fw".to_string()])
    );

    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
