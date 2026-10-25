// SPDX-License-Identifier: GPL-3.0-or-later
//! System sleep and resume through the engine, driven by a fake power-event
//! source (`lightloom_hal::power::ManualPower`): a HID device (a probe plugin
//! that writes what its lifecycle calls see) and an SMBus GPU (the MSI GPU
//! plugin over a mock bus) get `Shutdown(true, mode)` before sleep, nothing
//! touches them while asleep, and both start over (`Initialize()`) on resume.
//! Repeated events are no-ops. Lock/unlock blacks the canvas only with the
//! `lightsOffWhenIdle` setting.

use lightloom_api::{ApiError, Backend, Request, Snapshot};
use lightloom_daemon::config::Config;
use lightloom_daemon::engine::{self, EngineOptions, HardwareMode};
use lightloom_daemon::paths::{self, Paths};
use lightloom_hal::mock::{MockHid, MockNotifier, MockSmbus, TransactionKind, TransactionRecorder};
use lightloom_hal::power::{ManualPower, PowerEvent, PowerSource};
use lightloom_hal::{HidBackend, PciIds, ReportLengths, SmbusBusKind};
use lightloom_plugin_host::session::SessionDevice;
use lightloom_plugin_host::smbus::BusHandle;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const KEY: &str = "1234:5d5d:S";
const GPU: &str = "smbus:10de:2504:1462:3903@pci01:68";

/// Writes `[0x10]` from `Initialize()`, `[0x01, r, g, b]` per frame and
/// `[0x20, systemSuspending, mode]` from `Shutdown()` (mode: 0 restore, 1 keep, 2 off).
const PLUGIN: &str = r#"
export function Name() { return "Suspend probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x5D5D; }
export function Size() { return [1, 1]; }
export function LedNames() { return ["A"]; }
export function LedPositions() { return [[0, 0]]; }
/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }
export function Initialize() { device.write([0x10, 0, 0, 0], 4); }
export function Render() {
  const c = device.color(0, 0);
  device.write([0x01, c[0], c[1], c[2]], 4);
}
export function Shutdown(suspending, mode) {
  device.write([0x20, suspending ? 1 : 0, mode === "keep" ? 1 : mode === "off" ? 2 : 0, 0], 4);
}
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

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn running(s: &Snapshot, key: &str) -> bool {
    s.devices
        .iter()
        .any(|d| d.key == key && d.state == "running" && d.frames > 2)
}

fn settings(shutdown_mode: Option<&str>, lights_off_when_idle: Option<bool>) -> Request {
    Request::SetSettings {
        signalrgb: None,
        gpu_compositing: None,
        network: None,
        smbus: None,
        shutdown_mode: shutdown_mode.map(String::from),
        battery_low: None,
        battery_critical: None,
        battery_notifications: None,
        port: None,
        openrgb_sdk: None,
        lights_off_when_idle,
    }
}

#[test]
fn sleep_shuts_every_session_down_and_resume_starts_them_over() {
    let home = std::env::temp_dir().join(format!("lightloom-engine-suspend-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let paths = Paths::under(&home);
    std::fs::create_dir_all(paths.user_plugins()).unwrap();
    std::fs::write(paths.user_plugins().join("suspend-probe.js"), PLUGIN).unwrap();

    let info = MockHid::endpoint_info(0x1234, 0x5D5D, "S", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let hid = mock.recorder();
    let backend: Arc<dyn HidBackend> = mock.clone();
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
    let handle = engine::start(EngineOptions {
        paths,
        config,
        dry_run: false,
        hardware: HardwareMode::Fixed {
            backend,
            devices: vec![SessionDevice {
                smbus: None,
                lamparray: None,
                key: KEY.into(),
                vid: 0x1234,
                pid: 0x5D5D,
                endpoints: vec![info],
                network: None,
                usb: None,
                serial: None,
            }],
            smbus: vec![BusHandle::new(Box::new(bus))],
            lamparrays: None,
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
        signalrgb_settings: None,
    })
    .unwrap();
    let shared = handle.shared();
    let b: &dyn Backend = &*shared;
    let mut power = ManualPower::new();
    power.subscribe(shared.power_sink()).unwrap();

    let hid_writes = |first: u8| {
        hid.writes()
            .into_iter()
            .filter(|w| w[0] == first)
            .collect::<Vec<_>>()
    };
    let smbus_writes = || rec.payloads(TransactionKind::SmbusWrite);
    wait_for(b, "both devices rendering", |s| {
        running(s, KEY) && running(s, GPU)
    });
    assert_eq!(hid_writes(0x10).len(), 1, "one Initialize()");
    // The exit mode from feature/bake travels with the suspend flag.
    call(b, settings(Some("off"), None)).unwrap();

    // Sleep: the sink returns once the devices are put away.
    assert!(power.fire(PowerEvent::Suspend));
    let s = b.snapshot();
    assert_eq!(s.state, "Suspending");
    assert_eq!(
        hid_writes(0x20),
        [vec![0x20, 1, 2, 0]],
        "Shutdown(true, \"off\") ran once"
    );
    assert_eq!(
        *hid.writes().last().unwrap(),
        vec![0x20, 1, 2, 0],
        "nothing after Shutdown"
    );
    let gpu_last = smbus_writes().pop().unwrap();
    assert_eq!(
        &gpu_last[..2],
        &[0x68, 0x22],
        "the GPU plugin's Shutdown() wrote its mode last"
    );
    assert!(
        s.devices.iter().all(|d| d.state != "running"),
        "{:?}",
        s.devices.iter().map(|d| (&d.key, &d.state)).collect::<Vec<_>>()
    );

    // Asleep: a second sleep, a rescan and an effect change touch nothing.
    let (hid_n, smbus_n) = (hid.writes().len(), smbus_writes().len());
    assert!(power.fire(PowerEvent::Suspend));
    call(b, Request::Rescan).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(b.snapshot().state, "Suspending");
    assert_eq!((hid.writes().len(), smbus_writes().len()), (hid_n, smbus_n));

    // Resume: enumerated again, every session starts over.
    assert!(power.fire(PowerEvent::Resume));
    wait_for(b, "both devices rendering again", |s| {
        s.state == "Running" && running(s, KEY) && running(s, GPU)
    });
    assert_eq!(hid_writes(0x10).len(), 2, "Initialize() again");
    assert!(smbus_writes().len() > smbus_n, "the GPU is driven again");
    // A second resume (Windows sends two) changes nothing.
    assert!(power.fire(PowerEvent::Resume));
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(hid_writes(0x10).len(), 2);
    assert_eq!(hid_writes(0x20).len(), 1, "no session was stopped");

    // Lock: lights stay on unless asked; with the setting the canvas goes black.
    let last_frame = || hid_writes(0x01).pop().unwrap();
    until("a lit frame", || last_frame() != [1, 0, 0, 0]);
    assert!(power.fire(PowerEvent::Lock));
    std::thread::sleep(Duration::from_millis(300));
    assert_ne!(last_frame(), [1, 0, 0, 0], "lights stay on by default");
    assert_eq!(
        call(b, settings(None, Some(true))).unwrap()["lightsOffWhenIdle"],
        true
    );
    assert!(b.snapshot().settings.lights_off_when_idle);
    until("a black frame", || last_frame() == [1, 0, 0, 0]);
    assert!(power.fire(PowerEvent::Unlock));
    until("a lit frame after unlock", || last_frame() != [1, 0, 0, 0]);
    assert!(power.fire(PowerEvent::DisplayOff));
    until("a black frame with the display off", || {
        last_frame() == [1, 0, 0, 0]
    });
    assert!(power.fire(PowerEvent::DisplayOn));
    until("a lit frame with the display on", || last_frame() != [1, 0, 0, 0]);
    assert_eq!(hid_writes(0x10).len(), 2, "locking never restarts sessions");

    // Exit: Shutdown(false, mode) as before.
    power.stop();
    handle.shutdown();
    assert_eq!(*hid.writes().last().unwrap(), vec![0x20, 0, 2, 0]);
    drop(shared);
    let _ = std::fs::remove_dir_all(&home);
}
