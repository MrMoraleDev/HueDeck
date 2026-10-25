// SPDX-License-Identifier: GPL-3.0-or-later
//! `Type() == "serial"` device sessions over the mock serial backend
//! (docs/host-api/hid-usb.md §3): the plugin is matched by VID:PID,
//! `Validate` picks the port `connect()` binds, only the device's own ports
//! open, writes and reads reach the port (or are held back in a dry run), a
//! port that goes away closes and the plugin reconnects after a replug.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::mock::{MockSerial, TransactionKind};
use lightloom_hal::SerialBackend;
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::device::WriteMode;
use lightloom_plugin_host::serial::SerialTarget;
use lightloom_plugin_host::session::{self, SessionDevice, SessionHandle, SessionOptions};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const VID: u16 = 0x1234;
const PID: u16 = 0x5e01;

fn wait_until(deadline: Instant, frames: &FrameStore, mut done: impl FnMut() -> bool) {
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(0, 80, 160)));
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// COM3 (interface 2) and COM4 (interface 0) of the strip, COM9 of another device.
fn mock() -> Arc<MockSerial> {
    let m = MockSerial::new()
        .add_port(MockSerial::usb_port("COM3", VID, PID, "S1", Some(2)))
        .add_port(MockSerial::usb_port("COM4", VID, PID, "S1", Some(0)))
        .add_port(MockSerial::usb_port("COM9", 0x0403, 0x6001, "MODEM", Some(0)));
    m.reply("COM4", &[0xAA, 0x01], &[0xAA, 0x01, 0x00, 0x05]).unwrap();
    Arc::new(m)
}

fn start(mock: &Arc<MockSerial>, opts: SessionOptions) -> (SessionHandle, Arc<FrameStore>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let catalog = Catalog::scan(&[dir], Duration::from_secs(10));
    let plugin = catalog.lookup_serial(VID, PID).expect("serial probe").clone();
    assert!(catalog.lookup_device(VID, PID, &[]).is_none(), "not a HID driver");
    let ports: Vec<_> = mock
        .available_ports()
        .unwrap()
        .into_iter()
        .filter(|p| p.usb_ids() == Some((VID, PID)))
        .collect();
    let backend: Arc<dyn SerialBackend> = mock.clone();
    let device = SessionDevice::serial(VID, PID, SerialTarget { backend, ports });
    assert_eq!(device.key, "1234:5e01:S1");
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(0, 80, 160)));
    let handle = session::spawn(
        device,
        plugin,
        Arc::new(MockHid::new()),
        Arc::clone(&frames),
        opts,
    )
    .unwrap();
    (handle, frames)
}

fn probe(report: &session::SessionReport) -> serde_json::Value {
    let line = report
        .log_tail
        .iter()
        .find_map(|l| l.strip_prefix("probe "))
        .unwrap_or_else(|| panic!("probe line: {report:#?}"));
    serde_json::from_str(line).unwrap()
}

fn writes_to(mock: &MockSerial, port: &str) -> Vec<Vec<u8>> {
    mock.recorder()
        .snapshot()
        .into_iter()
        .filter(|t| t.kind == TransactionKind::Write && t.target == port)
        .map(|t| t.bytes)
        .collect()
}

#[test]
fn a_serial_plugin_drives_its_own_port() {
    let mock = mock();
    let (handle, frames) = start(&mock, SessionOptions::default());
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, &frames, || {
        handle.live().frames >= 3 || handle.is_finished()
    });
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.is_clean(), "{report:#?}");
    assert_eq!(report.validated, 1);
    assert_eq!(report.endpoint_path.as_deref(), Some("COM4 (serial)"));
    assert_eq!(
        probe(&report),
        serde_json::json!({
            "foreign": false,
            "unsupported": false,
            "connected": true,
            "written": 2,
            "reply": [0xAA, 0x01, 0x00, 0x05],
            "silent": [],
            "port": "COM4",
            "baud": 115200,
            "ports": [["COM3", VID, PID, 2], ["COM4", VID, PID, 0]],
            "deviceId": "S1",
            "isConnected": true,
        })
    );
    let state = mock.port("COM9").unwrap();
    assert_eq!(
        state.lock().unwrap().open_count,
        0,
        "the other device's port is never opened"
    );
    let cfg = mock
        .port("COM4")
        .unwrap()
        .lock()
        .unwrap()
        .last_config
        .clone()
        .unwrap();
    assert_eq!((cfg.baud_rate, cfg.data_bits, cfg.stop_bits), (115_200, 8, 1));
    let writes = writes_to(&mock, "COM4");
    assert_eq!(writes[0], [0xAA, 0x01]);
    assert!(
        writes[1..].iter().any(|w| w[..] == [0x55, 0, 80, 160]),
        "{writes:?}"
    );
    assert_eq!(writes.last().unwrap(), &[0x55, 0, 0, 0], "Shutdown");
    assert!(
        !mock.port("COM4").unwrap().lock().unwrap().is_open,
        "closed at the end"
    );
    let serial = report.serial.unwrap();
    assert_eq!(serial.stats.port.as_deref(), Some("COM4"));
    assert_eq!(serial.stats.refused, 1);
    assert_eq!(serial.stats.read_timeouts, 1);
    assert_eq!(serial.stats.reads, 1);
    assert_eq!(serial.stats.dry_run, 0);
    // The refused port counts as an I/O error of the session.
    assert_eq!(report.io_errors, 1);
}

#[test]
fn a_dry_run_holds_every_write_back() {
    let mock = mock();
    let (handle, frames) = start(
        &mock,
        SessionOptions {
            write_mode: WriteMode::DryRun,
            ..SessionOptions::default()
        },
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, &frames, || {
        handle.live().frames >= 2 || handle.is_finished()
    });
    handle.stop();
    let report = handle.join().unwrap();
    assert!(writes_to(&mock, "COM4").is_empty(), "nothing reached the port");
    let serial = report.serial.clone().unwrap();
    assert!(serial.stats.dry_run >= 3, "{serial:#?}");
    assert_eq!(serial.io_log[0].bytes, [0xAA, 0x01]);
    assert!(serial.io_log.iter().all(|r| r.dry_run && r.port == "COM4"));
    // The handshake reply never came: nothing was sent.
    assert_eq!(probe(&report)["reply"], serde_json::json!([]));
}

#[test]
fn a_port_that_goes_away_reconnects_after_a_replug() {
    let mock = mock();
    let (handle, frames) = start(&mock, SessionOptions::default());
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, &frames, || {
        handle.live().frames >= 2 || handle.is_finished()
    });
    mock.set_unplugged("COM4", true).unwrap();
    let frames_then = handle.live().frames;
    wait_until(deadline, &frames, || {
        handle.live().frames >= frames_then + 3 || handle.is_finished()
    });
    mock.set_unplugged("COM4", false).unwrap();
    let written = writes_to(&mock, "COM4").len();
    wait_until(deadline, &frames, || {
        writes_to(&mock, "COM4").len() >= written + 2 || handle.is_finished()
    });
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.faulted.is_none(), "{report:#?}");
    let stats = report.serial.unwrap().stats;
    assert_eq!(stats.disconnects, 1, "{stats:#?}");
    assert!(stats.connects >= 2, "{stats:#?}");
    assert!(
        stats.connect_errors >= 1,
        "reconnects failed while unplugged: {stats:#?}"
    );
}
