// SPDX-License-Identifier: GPL-3.0-or-later
//! A raw USB device Windows bound no WinUSB driver to: the engine starts no
//! session for its rawusb plugin and lists it as "needs-driver" with the
//! reason and a docs link; one with WinUSB runs.

use lightloom_api::{Backend, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineOptions, HardwareMode};
use lightloom_daemon::paths::{self, Paths};
use lightloom_hal::mock::{MockHid, MockNotifier, MockUsb};
use lightloom_hal::HidBackend;
use lightloom_plugin_host::session::{SessionDevice, UsbTarget};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PLUGIN: &str = r#"
export function Name() { return "Raw hub"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return [0x7001, 0x7002]; }
export function Type() { return "rawusb"; }
export function Size() { return [1, 1]; }
/* global device */
export function Initialize() {}
export function Render() { const c = device.color(0, 0); device.bulk_transfer(0x01, [c[0], c[1], c[2]], 3); }
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
fn a_device_without_winusb_is_listed_as_needing_it() {
    let home = std::env::temp_dir().join(format!("lightloom-winusb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let plugins = home.join("first-party");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("hub.js"), PLUGIN).unwrap();

    let bound = MockUsb::device_info(0x1234, 0x7001, "A", vec![MockUsb::winusb_interface(0, &[0x01])]);
    let unbound = MockUsb::device_info(
        0x1234,
        0x7002,
        "B",
        vec![MockUsb::foreign_interface(0, 0xFF, "", &[0x01])],
    );
    let usb = Arc::new(
        MockUsb::new()
            .add_device(bound.clone())
            .add_device(unbound.clone()),
    );
    let device = |key: &str, info: &lightloom_hal::UsbDeviceInfo| {
        SessionDevice::usb(
            key,
            UsbTarget {
                backend: usb.clone(),
                info: info.clone(),
            },
        )
    };
    let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new());
    let handle = engine::start(EngineOptions {
        paths: Paths::under(&home),
        config: Config::default(),
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend,
            devices: vec![device("1234:7001:A", &bound), device("1234:7002:B", &unbound)],
            smbus: Vec::new(),
            lamparrays: None,
        },
        ephemeral: true,
        plugins_dir: Some(plugins),
        effects_dir: paths::resource_dir("effects", "effects"),
        components_dir: None,
        renderer: None,
        plugin_cache: None,
        signalrgb_settings: None,
        notifier: Arc::new(MockNotifier::default()),
        api_port: lightloom_api::NATIVE_PORT,
        api_port_notice: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let s = wait_for(b, "the bound hub rendering", |s| {
        s.devices
            .iter()
            .any(|d| d.key == "1234:7001:A" && d.state == "running" && d.frames > 2)
    });
    let ok = s.devices.iter().find(|d| d.key == "1234:7001:A").unwrap();
    assert!(ok.driver_problem.is_none() && ok.help_url.is_none());
    let needs = s.devices.iter().find(|d| d.key == "1234:7002:B").unwrap();
    assert_eq!(needs.state, "needs-driver");
    assert_eq!(needs.frames, 0);
    assert!(needs
        .driver_problem
        .as_deref()
        .is_some_and(|p| p.starts_with(lightloom_hal::usb::NEEDS_DRIVER)));
    assert_eq!(
        needs.help_url.as_deref(),
        Some(lightloom_hal::usb::WINUSB_HELP_URL)
    );
    // The bound hub's frames reach the wire; nothing was ever opened on the other.
    assert!(!usb
        .recorder()
        .payloads(lightloom_hal::mock::TransactionKind::BulkOut)
        .is_empty());
    assert!(usb.recorder().snapshot().iter().all(|t| t.target != unbound.path));
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&home);
}
