// SPDX-License-Identifier: GPL-3.0-or-later
//! Device manager on the mock HAL: sessions follow devices being unplugged and
//! plugged back in, including a replug fast enough that no enumeration saw the
//! device missing (the session then holds a stale handle and is restarted).

use huedeck_engine::{Frame, FrameStore, CANVAS_HEIGHT, CANVAS_WIDTH};
use huedeck_hal::mock::MockHid;
use huedeck_hal::{HidBackend, HidEndpointInfo, ReportLengths};
use huedeck_plugin_host::catalog::{Catalog, CatalogEntry};
use huedeck_plugin_host::manager::{Change, DeviceManager, Present};
use huedeck_plugin_host::session::{SessionDevice, SessionOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn plugin() -> CatalogEntry {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x5555)
        .expect("hotplug probe")
        .clone()
}

fn info(serial: &str) -> HidEndpointInfo {
    MockHid::endpoint_info(0x1234, 0x5555, serial, 0, 0xFF00, 1, 0, ReportLengths::default())
}

fn present(mock: &MockHid, plugin: &CatalogEntry) -> Vec<Present> {
    mock.enumerate()
        .unwrap()
        .into_iter()
        .map(|e| Present {
            device: SessionDevice {
                usb: None,
                smbus: None,
                key: format!("1234:5555:{}", e.serial),
                vid: e.vid,
                pid: e.pid,
                endpoints: vec![e],
            },
            plugin: plugin.clone(),
        })
        .collect()
}

#[test]
fn sessions_follow_unplug_replug_and_stale_handles() {
    let (a, b) = (info("A"), info("B"));
    let mock = Arc::new(MockHid::new().add_endpoint(a.clone()).add_endpoint(b.clone()));
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    // Frames keep coming, as from a renderer.
    let running = Arc::new(AtomicBool::new(true));
    let publisher = {
        let (frames, running) = (Arc::clone(&frames), Arc::clone(&running));
        std::thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                frames.publish(Frame::new(CANVAS_WIDTH, CANVAS_HEIGHT));
                std::thread::sleep(Duration::from_millis(10));
            }
        })
    };
    let backend: Arc<dyn HidBackend> = mock.clone();
    let mut manager = DeviceManager::new(
        backend,
        frames,
        Box::new(|_: &SessionDevice, _: &CatalogEntry| SessionOptions::default()),
    )
    .with_restart_after(Duration::ZERO);
    let plugin = plugin();
    let frames_of = |m: &DeviceManager, key: &str| {
        m.sessions()
            .find(|h| h.key() == key)
            .map_or(0, |h| h.live().frames)
    };

    let changes = manager.reconcile(present(&mock, &plugin));
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert!(changes.iter().all(|c| matches!(c, Change::Started { .. })));
    wait_until("both sessions to render", || {
        frames_of(&manager, "1234:5555:A") > 2 && frames_of(&manager, "1234:5555:B") > 2
    });

    // A is unplugged: its writes fail, the next enumeration no longer lists it.
    mock.unplug(&a.path).unwrap();
    wait_until("A to notice", || manager.has_disconnected());
    let changes = manager.reconcile(present(&mock, &plugin));
    assert_eq!(
        changes,
        vec![Change::Removed {
            key: "1234:5555:A".into()
        }]
    );
    assert_eq!(manager.sessions().count(), 1);

    // A comes back: a new session.
    mock.replug(&a.path).unwrap();
    let changes = manager.reconcile(present(&mock, &plugin));
    assert!(matches!(&changes[..], [Change::Started { key, .. }] if key == "1234:5555:A"));

    // B is unplugged and replugged before any enumeration: stale handle, restart.
    mock.unplug(&b.path).unwrap();
    mock.replug(&b.path).unwrap();
    wait_until("B to notice", || manager.has_disconnected());
    let changes = manager.reconcile(present(&mock, &plugin));
    assert_eq!(
        changes,
        vec![Change::Restarted {
            key: "1234:5555:B".into()
        }]
    );
    wait_until("the new sessions to render", || {
        frames_of(&manager, "1234:5555:A") > 2 && frames_of(&manager, "1234:5555:B") > 2
    });

    let (reports, failed) = manager.finish();
    running.store(false, Ordering::Release);
    publisher.join().unwrap();
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(reports.len(), 4);
    let removed: Vec<_> = reports.iter().filter(|r| r.device_removed).collect();
    assert_eq!(removed.len(), 2, "A's first session and B's stale one");
    assert!(removed.iter().all(|r| r.io.disconnects > 0 && r.is_clean()));
    let current: Vec<_> = reports.iter().filter(|r| !r.device_removed).collect();
    assert!(
        current.iter().all(|r| r.is_clean() && r.io.disconnects == 0),
        "{current:#?}"
    );
}

#[test]
fn a_session_that_failed_before_removal_stays_failed() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x3333)
        .expect("failing plugin")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x3333, "F", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let mut manager = DeviceManager::new(
        backend,
        frames,
        Box::new(|_: &SessionDevice, _: &CatalogEntry| SessionOptions::default()),
    );
    let device = Present {
        device: SessionDevice {
            usb: None,
            smbus: None,
            key: "1234:3333:F".into(),
            vid: 0x1234,
            pid: 0x3333,
            endpoints: vec![info.clone()],
        },
        plugin,
    };
    manager.reconcile(vec![device]);
    wait_until("the session to end", || manager.all_finished());
    // Not restarted while plugged in; removed later.
    assert!(
        manager.reconcile(Vec::new())
            == vec![Change::Removed {
                key: "1234:3333:F".into()
            }]
    );
    let (reports, _) = manager.finish();
    assert_eq!(reports.len(), 1);
    assert!(!reports[0].device_removed, "it had ended before the removal");
    assert!(!reports[0].is_clean(), "the Initialize failure must stay visible");
}

#[test]
fn restarts_of_a_device_back_off() {
    let b = info("B");
    let mock = Arc::new(MockHid::new().add_endpoint(b.clone()));
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let running = Arc::new(AtomicBool::new(true));
    let publisher = {
        let (frames, running) = (Arc::clone(&frames), Arc::clone(&running));
        std::thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                frames.publish(Frame::new(CANVAS_WIDTH, CANVAS_HEIGHT));
                std::thread::sleep(Duration::from_millis(10));
            }
        })
    };
    let backend: Arc<dyn HidBackend> = mock.clone();
    let mut manager = DeviceManager::new(
        backend,
        frames,
        Box::new(|_: &SessionDevice, _: &CatalogEntry| SessionOptions::default()),
    )
    .with_restart_after(Duration::ZERO);
    let plugin = plugin();
    manager.reconcile(present(&mock, &plugin));
    let rendering = |m: &DeviceManager| m.sessions().all(|h| h.live().frames > 2);
    let stale = |m: &MockHid| {
        m.unplug(&b.path).unwrap();
        m.replug(&b.path).unwrap();
    };
    wait_until("B to render", || rendering(&manager));
    stale(&mock);
    wait_until("B to notice", || manager.has_disconnected());
    assert_eq!(
        manager.reconcile(present(&mock, &plugin)),
        vec![Change::Restarted {
            key: "1234:5555:B".into()
        }]
    );
    // Stale again at once: the next restart waits for the backoff (1 s).
    wait_until("the new session to render", || rendering(&manager));
    stale(&mock);
    wait_until("the new session to notice", || {
        manager.sessions().any(|h| h.live().disconnected)
    });
    assert!(!manager.has_disconnected(), "backing off");
    assert!(manager.reconcile(present(&mock, &plugin)).is_empty());
    wait_until("the backoff to pass", || manager.has_disconnected());
    assert_eq!(
        manager.reconcile(present(&mock, &plugin)),
        vec![Change::Restarted {
            key: "1234:5555:B".into()
        }]
    );
    let (reports, failed) = manager.finish();
    running.store(false, Ordering::Release);
    publisher.join().unwrap();
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(reports.len(), 3);
}

#[test]
fn smbus_devices_reconcile_by_key() {
    use huedeck_hal::mock::{MockSmbus, TransactionRecorder};
    use huedeck_hal::{PciIds, SmbusBusKind};
    use huedeck_plugin_host::discover::{scan_buses, ScanBudget};
    use huedeck_plugin_host::smbus::BusHandle;

    let rec = TransactionRecorder::new();
    let mut bus = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec)
        .with_pci(PciIds {
            vendor: 0x10DE,
            device: 0x2504,
            sub_vendor: 0x1462,
            sub_device: 0x3903,
        })
        .with_instance("pci01");
    bus.add_device(0x68, []);
    let bus = BusHandle::new(Box::new(bus));
    let catalog = Catalog::scan(
        &[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")],
        Duration::from_secs(10),
    );
    let (found, _) = scan_buses(&catalog, &[bus], &ScanBudget::default());
    assert_eq!(found.len(), 1);
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    // The manager's HID backend is never used for SMBus devices.
    let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new());
    let mut manager = DeviceManager::new(
        backend,
        frames,
        Box::new(|_: &SessionDevice, _: &CatalogEntry| SessionOptions::default()),
    );
    let key = "smbus:10de:2504:1462:3903@pci01:68".to_string();
    assert_eq!(
        manager.reconcile(found.clone()),
        vec![Change::Started {
            key: key.clone(),
            plugin: "MSI GPU".into()
        }]
    );
    assert!(manager.reconcile(found).is_empty(), "same key: nothing to do");
    assert_eq!(manager.reconcile(Vec::new()), vec![Change::Removed { key }]);
    let (reports, failed) = manager.finish();
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(reports.len(), 1);
    assert!(reports[0].smbus.is_some());
}
