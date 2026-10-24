// SPDX-License-Identifier: GPL-3.0-or-later
//! The built-in LampArray session (Windows Dynamic Lighting backend) against
//! the fake LampArray provider: it samples the canvas at the lamp positions
//! like a plugin device, follows brightness, skips writes while Windows gives
//! the device to another app and resends a full frame when control comes
//! back, releases the device on stop, writes nothing in dry-run mode, and
//! runs under the device manager like any plugin session.

use lightloom_engine::{Frame, FrameStore, Placement, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::lamparray::LampArrayBackend;
use lightloom_hal::mock::{MockHid, MockLampArray};
use lightloom_hal::HidBackend;
use lightloom_plugin_host::device::{Brightness, WriteMode};
use lightloom_plugin_host::lamparray::{builtin_entry, BUILTIN_NAME};
use lightloom_plugin_host::manager::{Change, DeviceManager, Present};
use lightloom_plugin_host::session::{
    self, Control, SessionDevice, SessionHandle, SessionOptions, SessionState,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RED: Rgb = Rgb::new(200, 0, 0);

fn wait_until(frames: &FrameStore, color: Rgb, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, color));
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn start(mock: &MockLampArray, opts: SessionOptions) -> (SessionHandle, Arc<FrameStore>) {
    let backend: Arc<dyn LampArrayBackend> = Arc::new(mock.clone());
    let info = backend.enumerate().unwrap().remove(0);
    let entry = builtin_entry(&info);
    let device = SessionDevice::lamparray(Arc::clone(&backend), info);
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let hid: Arc<dyn HidBackend> = Arc::new(MockHid::new());
    let handle = session::spawn(device, entry, hid, Arc::clone(&frames), opts).unwrap();
    (handle, frames)
}

/// The whole canvas under the device (a 4-lamp row placed over the canvas).
fn options() -> SessionOptions {
    SessionOptions {
        placement: Some(Placement::fit([32, 8], Default::default())),
        ..SessionOptions::default()
    }
}

#[test]
fn samples_the_canvas_and_follows_brightness() {
    let mock = MockLampArray::new();
    mock.add(MockLampArray::row("lamp-a", 0x1234, 0x5678, 4));
    let (h, frames) = start(&mock, options());
    assert_eq!(
        h.key(),
        lightloom_hal::lamparray::device_key(&MockLampArray::row("lamp-a", 0x1234, 0x5678, 4))
    );
    wait_until(&frames, RED, || {
        mock.writes("lamp-a").iter().any(|w| w[0] == [200, 0, 0, 255])
    });
    let last = mock.writes("lamp-a").last().cloned().unwrap();
    assert_eq!(last, vec![[200, 0, 0, 255]; 4]);
    let views = h.endpoints();
    assert_eq!(
        (views.len(), views[0].leds, views[0].name.as_str()),
        (1, 4, "Mock LampArray lamp-a")
    );

    h.control(Control::Brightness(Brightness {
        global: 50,
        layout: 100,
    }));
    wait_until(&frames, RED, || {
        mock.writes("lamp-a")
            .last()
            .is_some_and(|w| w[0][0] < 200 && w[0][0] > 0)
    });
    assert!(h.live().first_write_ms.is_some());

    h.stop();
    let report = h.join().unwrap();
    assert!(report.initialized && report.shutdown_called && report.failures.is_empty());
    assert_eq!(report.name, BUILTIN_NAME);
    assert_eq!(mock.releases("lamp-a"), 1);
}

#[test]
fn unchanged_colours_are_not_resent_every_frame() {
    let mock = MockLampArray::new();
    mock.add(MockLampArray::row("lamp-a", 1, 2, 3));
    let (h, frames) = start(&mock, options());
    wait_until(&frames, RED, || !mock.writes("lamp-a").is_empty());
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(300) {
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, RED));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        mock.writes("lamp-a").len() <= 2,
        "{} writes",
        mock.writes("lamp-a").len()
    );
    assert!(h.live().frames > 10);
    h.stop();
    h.join().unwrap();
}

#[test]
fn pauses_while_another_app_has_the_device_and_resends_when_back() {
    let mock = MockLampArray::new();
    mock.add(MockLampArray::row("lamp-a", 1, 2, 2));
    mock.set_available("lamp-a", false);
    let (h, frames) = start(&mock, options());
    wait_until(&frames, RED, || h.live().paused);
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(200) {
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, RED));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(mock.writes("lamp-a").is_empty(), "no writes while unavailable");
    assert_eq!(h.live().state, SessionState::Running);

    mock.set_available("lamp-a", true);
    wait_until(&frames, RED, || !mock.writes("lamp-a").is_empty());
    assert!(!h.live().paused);
    // Unavailable then available again: the unchanged frame is sent once more.
    let before = mock.writes("lamp-a").len();
    mock.set_available("lamp-a", false);
    wait_until(&frames, RED, || h.live().paused);
    mock.set_available("lamp-a", true);
    wait_until(&frames, RED, || mock.writes("lamp-a").len() > before);
    h.stop();
    let report = h.join().unwrap();
    assert!(report.log_tail.iter().any(|l| l.contains("paused")));
}

#[test]
fn dry_run_never_writes_and_removal_shows_as_disconnected() {
    let mock = MockLampArray::new();
    mock.add(MockLampArray::row("lamp-a", 1, 2, 2));
    let (h, frames) = start(
        &mock,
        SessionOptions {
            write_mode: WriteMode::DryRun,
            ..options()
        },
    );
    wait_until(&frames, RED, || h.live().frames > 5);
    h.stop();
    let report = h.join().unwrap();
    assert!(mock.writes("lamp-a").is_empty());
    assert_eq!(mock.releases("lamp-a"), 0);
    assert!(report.first_write_ms.is_some());

    let (h, frames) = start(&mock, options());
    wait_until(&frames, RED, || !mock.writes("lamp-a").is_empty());
    mock.remove("lamp-a");
    let mut c = 0u8;
    wait_until(&frames, Rgb::new(0, 0, 9), || {
        c = c.wrapping_add(1);
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(c, 1, 1)));
        h.live().disconnected
    });
    h.stop();
    let report = h.join().unwrap();
    assert!(report.io_errors > 0);
}

#[test]
fn the_device_manager_runs_lamparray_sessions() {
    let mock = MockLampArray::new();
    mock.add(MockLampArray::row("lamp-a", 1, 2, 2));
    let backend: Arc<dyn LampArrayBackend> = Arc::new(mock.clone());
    let info = backend.enumerate().unwrap().remove(0);
    let present = Present {
        plugin: builtin_entry(&info),
        device: SessionDevice::lamparray(Arc::clone(&backend), info),
    };
    let key = present.device.key.clone();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let hid: Arc<dyn HidBackend> = Arc::new(MockHid::new());
    let mut m = DeviceManager::new(hid, Arc::clone(&frames), Box::new(|_, _| options()));
    let changes = m.reconcile(vec![present]);
    assert_eq!(
        changes,
        vec![Change::Started {
            key: key.clone(),
            plugin: BUILTIN_NAME.into()
        }]
    );
    wait_until(&frames, RED, || !mock.writes("lamp-a").is_empty());
    assert_eq!(m.reconcile(Vec::new()), vec![Change::Removed { key }]);
    let (reports, failed) = m.finish();
    assert!(failed.is_empty());
    assert_eq!(reports.len(), 1);
    assert_eq!(mock.releases("lamp-a"), 1);
}
