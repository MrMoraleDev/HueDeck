// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Device manager on the mock HAL: sessions follow devices being unplugged and
//! plugged back in, including a replug fast enough that no enumeration saw the
//! device missing (the session then holds a stale handle and is restarted).

use lightloom_engine::{Frame, FrameStore, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HidBackend, HidEndpointInfo, ReportLengths};
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry};
use lightloom_plugin_host::manager::{Change, DeviceManager, Present};
use lightloom_plugin_host::session::{SessionDevice, SessionOptions};
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
