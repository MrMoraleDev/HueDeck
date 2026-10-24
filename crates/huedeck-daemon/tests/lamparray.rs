// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows Dynamic Lighting devices in the engine, on the fake LampArray
//! provider: a LampArray no plugin owns gets the built-in session and shows
//! as a device; one whose product a plugin drives is left to the plugin; a
//! LampArray plugged in later is picked up (the backend's generation
//! changes); a device Windows gives to another app shows as paused; and
//! without package identity the dashboard says why.

use huedeck_api::{Backend, Snapshot};
use huedeck_daemon::config::Config;
use huedeck_daemon::engine::{self, EngineOptions, HardwareMode};
use huedeck_daemon::paths::{self, Paths};
use huedeck_hal::lamparray::{device_key, LampArrayBackend};
use huedeck_hal::mock::{MockHid, MockLampArray, MockNotifier};
use huedeck_hal::{HidBackend, ReportLengths};
use huedeck_plugin_host::session::SessionDevice;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PLUGIN: &str = r#"
export function Name() { return "Vendor mouse"; }
export function Publisher() { return "HueDeck tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x4D4D; }
export function Size() { return [1, 1]; }
export function LedNames() { return ["A"]; }
export function LedPositions() { return [[0, 0]]; }
/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() {}
export function Render() { const c = device.color(0, 0); device.write([0x01, c[0], c[1], c[2]], 4); }
export function Shutdown() {}
"#;

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

#[test]
fn lamparrays_no_plugin_owns_are_driven_and_the_others_left_to_plugins() {
    let home = std::env::temp_dir().join(format!("huedeck-lamparray-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("mouse.js"), PLUGIN).unwrap();

    let info = MockHid::endpoint_info(0x1234, 0x4D4D, "D", 0, 0xFF00, 1, 0, ReportLengths::default());
    let hid: Arc<dyn HidBackend> = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let lamps = MockLampArray::new();
    lamps.set_identity(false);
    // The same product as the plugin's mouse: the plugin wins.
    lamps.add(MockLampArray::row("owned", 0x1234, 0x4D4D, 3));
    let free = MockLampArray::row("free", 0x0b05, 0x1111, 4);
    lamps.add(free.clone());
    let free_key = device_key(&free);
    let backend: Arc<dyn LampArrayBackend> = Arc::new(lamps.clone());
    let handle = engine::start(EngineOptions {
        paths: Paths::under(&home),
        config: Config::default(),
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend: hid,
            devices: vec![SessionDevice::hid("1234:4d4d:D", 0x1234, 0x4D4D, vec![info])],
            smbus: Vec::new(),
            lamparrays: Some(backend),
        },
        ephemeral: true,
        plugins_dir: Some(plugins),
        effects_dir: paths::resource_dir("effects", "effects"),
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: huedeck_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;

    let s = wait_for(b, "the free LampArray running", |s| {
        s.devices
            .iter()
            .any(|d| d.key == free_key && d.state == "running")
            && s.devices
                .iter()
                .any(|d| d.key == "1234:4d4d:D" && d.state == "running")
    });
    let d = s.devices.iter().find(|d| d.key == free_key).unwrap();
    assert_eq!(d.plugin, huedeck_plugin_host::lamparray::BUILTIN_NAME);
    assert_eq!(d.bus.as_deref(), Some("Windows Dynamic Lighting"));
    assert_eq!(d.endpoints[0].leds, 4);
    // The owned one is not a device of its own.
    assert_eq!(s.devices.len(), 2, "{:#?}", s.devices);
    assert!(
        s.warnings.iter().any(|w| w.contains("identity package")),
        "{:?}",
        s.warnings
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while lamps.writes("free").is_empty() {
        assert!(Instant::now() < deadline, "no LampArray write");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(lamps.writes("owned").is_empty());

    // Another app takes the device: paused.
    lamps.set_available("free", false);
    wait_for(b, "paused", |s| {
        s.devices.iter().any(|d| d.key == free_key && d.state == "paused")
    });
    lamps.set_available("free", true);

    // Plugged in later: picked up after the settle time.
    let late = MockLampArray::row("late", 0x0b05, 0x2222, 2);
    let late_key = device_key(&late);
    lamps.add(late);
    wait_for(b, "the late LampArray", |s| {
        s.devices
            .iter()
            .any(|d| d.key == late_key && d.state == "running")
    });
    // Unplugged: its session stops.
    lamps.remove("late");
    wait_for(b, "the late LampArray gone", |s| {
        s.devices.iter().all(|d| d.key != late_key || d.state == "absent")
    });

    handle.shutdown();
    assert_eq!(lamps.releases("free"), 1);
    let _ = std::fs::remove_dir_all(&home);
}
