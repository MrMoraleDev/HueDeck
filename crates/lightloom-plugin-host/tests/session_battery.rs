// SPDX-License-Identifier: GPL-3.0-or-later
//! The `battery` feature global in a device session (docs/host-api/hid-usb.md
//! §addFeature): levels and states reach the live snapshot and the report,
//! the global reads them back, `lowBatteryWarningLevel` follows the caller's
//! setting; and the alert override pulses every LED in its colour.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::battery::BatteryState;
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::session::{self, Control, SessionDevice, SessionHandle, SessionOptions};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_until(deadline: Instant, frames: &FrameStore, mut done: impl FnMut() -> bool) {
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(0, 80, 160)));
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn start(opts: SessionOptions) -> (SessionHandle, Arc<MockHid>, Arc<FrameStore>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x2222)
        .expect("battery probe")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x2222, "B", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(0, 80, 160)));
    let handle = session::spawn(
        SessionDevice {
            smbus: None,
            key: "1234:2222:B".into(),
            vid: 0x1234,
            pid: 0x2222,
            endpoints: vec![info],
            network: None,
        },
        plugin,
        backend,
        Arc::clone(&frames),
        opts,
    )
    .unwrap();
    (handle, mock, frames)
}

#[test]
fn battery_reports_reach_the_live_snapshot_and_the_report() {
    let (handle, mock, frames) = start(SessionOptions {
        battery_warning_level: 20,
        ..SessionOptions::default()
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, &frames, || {
        handle.live().frames >= 1 || handle.is_finished()
    });
    let first = handle.live().battery.expect("battery reported");
    assert_eq!(first.level, Some(42));
    assert_eq!(first.state, BatteryState::Draining);
    assert!(first.updated_ms > 0);

    // Frame 3 reports charging and a level above 100 (clamped).
    wait_until(deadline, &frames, || {
        handle.live().frames >= 4 || handle.is_finished()
    });
    let later = handle.live().battery.unwrap();
    assert_eq!((later.level, later.state), (Some(100), BatteryState::Charging));

    handle.control(Control::BatteryWarningLevel(7));
    let seen = mock.recorder().writes().len();
    wait_until(deadline, &frames, || {
        mock.recorder().writes().len() >= seen + 2 || handle.is_finished()
    });
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.is_clean(), "{report:#?}");

    let probe = report
        .log_tail
        .iter()
        .find_map(|l| l.strip_prefix("probe "))
        .expect("probe line");
    let probe: serde_json::Value = serde_json::from_str(probe).unwrap();
    assert_eq!(
        probe,
        serde_json::json!({
            "before": { "level": 0, "state": 0 },
            "level": 42,
            "state": 1,
            "text": "Draining",
            "warn": 20,
            "constants": [0, 1, 2, 3, 4, 5],
        })
    );
    let writes = mock.recorder().writes();
    assert_eq!(writes[0].last(), Some(&20), "{writes:?}");
    assert_eq!(writes.last().unwrap().last(), Some(&7), "{writes:?}");
    let battery = report.battery.unwrap();
    assert_eq!(
        (battery.level, battery.state),
        (Some(100), BatteryState::Charging)
    );
}

#[test]
fn the_alert_override_pulses_every_led_and_ends() {
    let (handle, mock, frames) = start(SessionOptions::default());
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, &frames, || {
        handle.live().frames >= 1 || handle.is_finished()
    });
    let canvas = |w: &Vec<u8>| w[1..4] == [0, 80, 160];
    assert!(canvas(mock.recorder().writes().last().unwrap()));

    handle.control(Control::Alert(Some(Rgb::new(255, 0, 0))));
    // Over a whole pulse the red channel moves; green and blue stay off.
    let seen = mock.recorder().writes().len();
    let pulse = Instant::now();
    wait_until(deadline, &frames, || {
        pulse.elapsed() >= Duration::from_millis(1800) || handle.is_finished()
    });
    let alerted: Vec<Vec<u8>> = mock.recorder().writes()[seen + 1..].to_vec();
    assert!(alerted.len() > 5, "{alerted:?}");
    assert!(
        alerted.iter().all(|w| w[1] > 0 && w[2] == 0 && w[3] == 0),
        "{alerted:?}"
    );
    let reds: Vec<u8> = alerted.iter().map(|w| w[1]).collect();
    let (lo, hi) = (*reds.iter().min().unwrap(), *reds.iter().max().unwrap());
    assert!(lo < 120 && hi > 200, "{reds:?}");

    handle.control(Control::Alert(None));
    let seen = mock.recorder().writes().len();
    wait_until(deadline, &frames, || {
        mock.recorder().writes().len() >= seen + 3 || handle.is_finished()
    });
    assert!(canvas(mock.recorder().writes().last().unwrap()));
    handle.stop();
    assert!(handle.join().unwrap().is_clean());
}
