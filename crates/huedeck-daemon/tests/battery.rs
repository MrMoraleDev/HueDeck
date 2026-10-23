// SPDX-License-Identifier: GPL-3.0-or-later
//! Battery warnings end to end: a plugin reports the level its parameters
//! set; the snapshot shows it, low and critical alerts become dashboard
//! warnings and system notifications (once per discharge cycle), the
//! thresholds and the notification toggle are settings, and the per-device
//! pulse overrides the device's colours while the battery is low.

use huedeck_api::{ApiError, Backend, Request, Snapshot};
use huedeck_daemon::config::Config;
use huedeck_daemon::engine::{self, EngineOptions, HardwareMode};
use huedeck_daemon::paths::Paths;
use huedeck_hal::mock::{MockHid, MockNotifier};
use huedeck_hal::{HidBackend, ReportLengths};
use huedeck_plugin_host::session::SessionDevice;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:4e4e:B";

const PLUGIN: &str = r#"
export function Name() { return "Battery mouse"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4E4E; }
export function Size() { return [1, 1]; }
export function ControllableParameters() {
  return [
    { property: "level", label: "Level", type: "number", min: "0", max: "100", default: "50" },
    { property: "charging", label: "Charging", type: "boolean", default: "false" },
  ];
}
/* global device, battery, level, charging */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() { device.addFeature("battery"); }
export function Render() {
  battery.setBatteryLevel(level);
  battery.setBatteryState(charging ? battery.charging : battery.draining);
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

fn battery(s: &Snapshot) -> Option<(Option<u8>, String, String)> {
    let b = s.devices.iter().find(|d| d.key == KEY)?.battery.as_ref()?;
    Some((b.level, b.state.clone(), b.status.clone()))
}

fn set(b: &dyn Backend, values: Value) {
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: None,
            parameters: Some(values.as_object().unwrap().clone()),
            blink_low_battery: None,
        },
    )
    .unwrap();
}

fn settings(
    b: &dyn Backend,
    low: Option<u8>,
    critical: Option<u8>,
    notify: Option<bool>,
) -> Result<Value, ApiError> {
    call(
        b,
        Request::SetSettings {
            signalrgb: None,
            gpu_compositing: None,
            smbus: None,
            battery_low: low,
            battery_critical: critical,
            battery_notifications: notify,
            port: None,
            openrgb_sdk: None,
        },
    )
}

#[test]
fn battery_levels_raise_warnings_notifications_and_the_pulse() {
    let home = std::env::temp_dir().join(format!("huedeck-battery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("battery.js"), PLUGIN).unwrap();

    let info = MockHid::endpoint_info(0x1234, 0x4E4E, "B", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let notifier = Arc::new(MockNotifier::default());
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
                pid: 0x4E4E,
                endpoints: vec![info],
            }],
            smbus: Vec::new(),
        },
        ephemeral: true,
        plugins_dir: Some(plugins),
        effects_dir: None,
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        notifier: notifier.clone(),
        api_port: huedeck_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let titles = || notifier.shown().into_iter().map(|n| n.title).collect::<Vec<_>>();

    let s = wait_for(b, "the first battery report", |s| battery(s).is_some());
    assert_eq!(battery(&s), Some((Some(50), "discharging".into(), "ok".into())));
    let st = &s.settings;
    assert_eq!(
        (st.battery_low, st.battery_critical, st.battery_notifications),
        (15, 5, true)
    );
    assert!(!s.devices[0].blink_low_battery);
    assert!(s.devices[0].battery.as_ref().unwrap().updated_ms > 0);

    // Low: a dashboard warning and one notification; the device pulses red.
    call(
        b,
        Request::SetDevice {
            key: KEY.into(),
            enabled: None,
            parameters: None,
            blink_low_battery: Some(true),
        },
    )
    .unwrap();
    set(b, json!({ "level": 12 }));
    let s = wait_for(b, "low", |s| battery(s).is_some_and(|b| b.2 == "low"));
    assert!(s.devices[0].blink_low_battery);
    assert!(
        s.warnings
            .iter()
            .any(|w| w.contains("Battery mouse: battery low (12 %)")),
        "{:?}",
        s.warnings
    );
    wait_for(b, "the low notification", |_| titles().len() == 1);
    assert_eq!(titles(), vec!["Battery mouse: battery low"]);
    let pulsing = |n: usize| {
        let w = mock.recorder().writes();
        w.len() > n
            && w[w.len() - n..]
                .iter()
                .all(|w| w[1] > 0 && w[2] == 0 && w[3] == 0)
    };
    wait_for(b, "the red pulse", |_| pulsing(5));

    // Critical, then lower again: one more notification only.
    set(b, json!({ "level": 3 }));
    wait_for(b, "critical", |s| battery(s).is_some_and(|b| b.2 == "critical"));
    wait_for(b, "the critical notification", |_| titles().len() == 2);
    set(b, json!({ "level": 2 }));
    wait_for(b, "level 2", |s| battery(s).is_some_and(|b| b.0 == Some(2)));
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(titles().len(), 2, "{:?}", titles());

    // Charging re-arms; the pulse and the warning end.
    set(b, json!({ "charging": true }));
    let s = wait_for(b, "charging", |s| {
        battery(s).is_some_and(|b| b.1 == "charging" && b.2 == "ok")
    });
    assert!(!s.warnings.iter().any(|w| w.contains("Battery mouse")));
    wait_for(b, "the canvas back", |_| {
        let w = mock.recorder().writes();
        w.len() > 3 && w[w.len() - 3..].iter().any(|w| w[2] != 0 || w[3] != 0)
    });

    // Notifications off: the next critical alert is a warning only.
    settings(b, None, None, Some(false)).unwrap();
    set(b, json!({ "charging": false }));
    wait_for(b, "critical again", |s| {
        battery(s).is_some_and(|b| b.2 == "critical")
    });
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(titles().len(), 2, "{:?}", titles());

    // Thresholds: checked, stored, applied.
    assert_eq!(settings(b, Some(5), Some(10), None).unwrap_err().status, 400);
    assert_eq!(settings(b, Some(0), None, None).unwrap_err().status, 400);
    settings(b, Some(30), Some(1), Some(true)).unwrap();
    let s = wait_for(b, "low under the new thresholds", |s| {
        battery(s).is_some_and(|b| b.2 == "low")
    });
    assert_eq!((s.settings.battery_low, s.settings.battery_critical), (30, 1));

    handle.shutdown();
    drop(shared);
    let _ = std::fs::remove_dir_all(&home);
}
