// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw USB device sessions (`Type() == "rawusb"` / `"hybrid"`) against the
//! mock USB backend: bulk IN/OUT, control transfers, timeouts and the default
//! timeout, interfaces bound to other drivers never claimed, a device without
//! WinUSB reported as needing it, dry-run recording and `setProtocol`.

use huedeck_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use huedeck_hal::mock::{MockHid, MockUsb, TransactionKind, TransactionRecorder};
use huedeck_hal::usb::{UsbBackend, UsbDeviceInfo, UsbHandle};
use huedeck_hal::{HalResult, HidBackend, ReportLengths};
use huedeck_plugin_host::catalog::{Catalog, CatalogEntry};
use huedeck_plugin_host::device::{IoKind, WriteMode};
use huedeck_plugin_host::session::{self, SessionDevice, SessionOptions, SessionReport, UsbTarget};
use huedeck_plugin_host::PluginType;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn plugin(pid: u16) -> CatalogEntry {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, pid)
        .expect("test plugin")
        .clone()
}

/// A hub like the Lian Li ones: HID on interface 0, vendor interface 1 under WinUSB.
fn hub(pid: u16, winusb: bool) -> UsbDeviceInfo {
    let vendor = if winusb {
        MockUsb::winusb_interface(1, &[0x02, 0x82])
    } else {
        MockUsb::foreign_interface(1, 0xFF, "", &[0x02, 0x82])
    };
    MockUsb::device_info(
        0x1234,
        pid,
        "U1",
        vec![MockUsb::foreign_interface(0, 3, "HidUsb", &[0x81]), vendor],
    )
}

/// Records the timeout of every transfer, then hands it to the mock.
struct TimeoutSpy {
    inner: Arc<MockUsb>,
    seen: Arc<Mutex<Vec<u32>>>,
}

struct SpyHandle {
    inner: Box<dyn UsbHandle>,
    seen: Arc<Mutex<Vec<u32>>>,
}

impl UsbBackend for TimeoutSpy {
    fn enumerate(&self) -> HalResult<Vec<UsbDeviceInfo>> {
        self.inner.enumerate()
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn UsbHandle>> {
        Ok(Box::new(SpyHandle {
            inner: self.inner.open(path)?,
            seen: Arc::clone(&self.seen),
        }))
    }
}

impl UsbHandle for SpyHandle {
    fn info(&self) -> &UsbDeviceInfo {
        self.inner.info()
    }

    fn claim_interface(&mut self, interface: u8) -> HalResult<()> {
        self.inner.claim_interface(interface)
    }

    fn bulk_transfer(&mut self, endpoint: u8, data: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
        self.seen.lock().unwrap().push(timeout_ms);
        self.inner.bulk_transfer(endpoint, data, timeout_ms)
    }

    fn control_transfer(
        &mut self,
        bm: u8,
        rq: u8,
        v: u16,
        i: u16,
        data: &mut [u8],
        t: u32,
    ) -> HalResult<usize> {
        self.seen.lock().unwrap().push(t);
        self.inner.control_transfer(bm, rq, v, i, data, t)
    }
}

struct Run {
    report: SessionReport,
    recorder: TransactionRecorder,
    timeouts: Vec<u32>,
}

/// Run `plugin` on `device` (HID collections in `hid`) until a few frames rendered.
fn run(
    plugin: CatalogEntry,
    device: SessionDevice,
    hid: Arc<MockHid>,
    opts: SessionOptions,
    frames_wanted: u64,
) -> Run {
    let recorder = hid.recorder();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let backend: Arc<dyn HidBackend> = hid;
    let handle = session::spawn(device, plugin, backend, Arc::clone(&frames), opts).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.live().frames < frames_wanted && !handle.is_finished() {
        assert!(Instant::now() < deadline, "timed out");
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(10, 20, 30)));
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.stop();
    Run {
        report: handle.join().unwrap(),
        recorder,
        timeouts: Vec::new(),
    }
}

/// A rawusb session on `info`, logging HID and USB into one recorder.
fn rawusb(info: UsbDeviceInfo, probe: &str, write_mode: WriteMode) -> Run {
    let recorder = TransactionRecorder::new();
    let usb = Arc::new(MockUsb::with_recorder(recorder.clone()).add_device(info.clone()));
    usb.script_control_in(&info.path, vec![0x03, 0xE8, 0x00, 0x00])
        .unwrap();
    usb.script_bulk_in(&info.path, 0x82, vec![0xAA, 0xBB]).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let spy = Arc::new(TimeoutSpy {
        inner: usb,
        seen: Arc::clone(&seen),
    });
    let device = SessionDevice::usb("1234:7701:U1", UsbTarget { backend: spy, info });
    let mut parameters = serde_json::Map::new();
    parameters.insert("probe".into(), probe.into());
    let opts = SessionOptions {
        parameters,
        write_mode,
        ..SessionOptions::default()
    };
    let hid = Arc::new(MockHid::with_recorder(recorder));
    let mut r = run(plugin(0x7701), device, hid, opts, 3);
    r.timeouts = seen.lock().unwrap().clone();
    r
}

fn log_line<'a>(report: &'a SessionReport, prefix: &str) -> &'a str {
    report
        .log_tail
        .iter()
        .find_map(|l| l.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("no `{prefix}` line in {:#?}", report.log_tail))
}

#[test]
fn rawusb_and_hybrid_plugins_are_catalogued() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let catalog = Catalog::scan(&[dir], Duration::from_secs(10));
    assert_eq!(
        catalog.lookup(0x1234, 0x7701).unwrap().meta.plugin_type,
        PluginType::RawUsb
    );
    assert_eq!(
        catalog.lookup(0x1234, 0x9999).unwrap().meta.plugin_type,
        PluginType::Hybrid
    );
}

#[test]
fn a_rawusb_session_drives_bulk_and_control_transfers() {
    let r = rawusb(hub(0x7701, true), "none", WriteMode::Live);
    assert!(r.report.is_clean(), "{:#?}", r.report);
    assert_eq!(r.report.endpoint_path.as_deref(), Some("mockusb://1234:7701/U1"));
    // Control OUT returns the bytes sent; control IN the data stage.
    assert_eq!(log_line(&r.report, "control "), "2 03e80000 4");
    assert_eq!(log_line(&r.report, "bulk-in "), "aabb 2");
    // An IN transfer without data times out: empty, getLastReadSize() 0, not an error.
    assert_eq!(log_line(&r.report, "timeout "), "0 0");
    assert_eq!(r.report.io.usb_timeouts, 1);

    let kinds: Vec<TransactionKind> = r.recorder.snapshot().iter().map(|t| t.kind).collect();
    assert_eq!(
        &kinds[..3],
        &[
            TransactionKind::Open,
            TransactionKind::UsbClaim,
            TransactionKind::ControlOut
        ]
    );
    assert_eq!(kinds.last(), Some(&TransactionKind::Close));
    // Only the WinUSB interface was claimed.
    assert_eq!(r.recorder.payloads(TransactionKind::UsbClaim), vec![vec![1]]);
    assert_eq!(
        r.recorder.payloads(TransactionKind::ControlOut),
        vec![vec![0x40, 0x80, 0, 0, 0x10, 0, 2, 0, 0x01, 0x02]]
    );
    // Render() streams the canvas colour; Shutdown(false) sends black last.
    let outs = r.recorder.payloads(TransactionKind::BulkOut);
    assert!(outs.len() >= 4, "{outs:?}");
    assert_eq!(outs[0], vec![0x02, 0xA0, 10, 20, 30]);
    assert_eq!(outs.last().unwrap(), &vec![0x02, 0xA0, 0, 0, 0]);
    // Timeouts as given; 1000 ms when the plugin passes none.
    assert_eq!(&r.timeouts[..4], &[1000, 1000, 500, 50]);
    assert_eq!(r.timeouts[4], 1000, "bulk_transfer without a timeout argument");
}

#[test]
fn interfaces_of_other_drivers_and_standard_requests_are_refused() {
    let r = rawusb(hub(0x7701, true), "refusals", WriteMode::Live);
    assert_eq!(log_line(&r.report, "refused "), "-1 -1 0");
    assert_eq!(r.recorder.payloads(TransactionKind::UsbClaim), vec![vec![1]]);
    assert!(r
        .recorder
        .payloads(TransactionKind::ControlOut)
        .iter()
        .all(|p| p[0] == 0x40));
    assert_eq!(r.report.io.usb_write_errors, 2);
    assert_eq!(r.report.io.usb_read_errors, 1);
    let log = r.report.log_tail.join("\n");
    assert!(
        log.contains("needs WinUSB driver") && log.contains("HidUsb"),
        "{log}"
    );
    assert!(log.contains("standard OUT requests"), "{log}");
}

#[test]
fn a_device_without_winusb_faults_with_a_clear_message() {
    let info = hub(0x7701, false);
    let device = SessionDevice::usb(
        "1234:7701:U1",
        UsbTarget {
            backend: Arc::new(MockUsb::new().add_device(info.clone())),
            info: info.clone(),
        },
    );
    let problem = device.driver_problem().expect("no WinUSB");
    assert!(problem.starts_with("needs WinUSB driver"), "{problem}");
    assert!(problem.contains(huedeck_hal::usb::WINUSB_HELP_URL));
    let r = run(
        plugin(0x7701),
        device,
        Arc::new(MockHid::new()),
        SessionOptions::default(),
        0,
    );
    let fault = r.report.faulted.expect("faulted");
    assert!(fault.contains("needs WinUSB driver"), "{fault}");
    assert!(!r.report.initialized);
}

#[test]
fn dry_runs_record_out_transfers_and_still_read() {
    let r = rawusb(hub(0x7701, true), "none", WriteMode::DryRun);
    assert!(r.recorder.payloads(TransactionKind::BulkOut).is_empty());
    assert!(r.recorder.payloads(TransactionKind::ControlOut).is_empty());
    assert_eq!(r.recorder.payloads(TransactionKind::ControlIn).len(), 1);
    assert_eq!(log_line(&r.report, "bulk-in "), "aabb 2");
    let dry: Vec<_> = r.report.io_log.iter().filter(|x| x.dry_run).collect();
    assert_eq!(dry[0].kind, IoKind::ControlOut);
    assert_eq!(dry[0].bytes, vec![0x40, 0x80, 0, 0, 0x10, 0, 2, 0, 0x01, 0x02]);
    assert!(dry
        .iter()
        .any(|x| x.kind == IoKind::BulkOut && x.bytes == vec![0x02, 0xA0, 10, 20, 30]));
    assert!(r
        .report
        .io_log
        .iter()
        .any(|x| !x.dry_run && x.kind == IoKind::BulkIn && x.bytes == vec![0x82, 0xAA, 0xBB]));
}

#[test]
fn a_hybrid_session_mixes_hid_and_bulk_and_switches_protocol() {
    let recorder = TransactionRecorder::new();
    let ep = MockHid::endpoint_info(0x1234, 0x9999, "U1", 0, 0xFF00, 1, 0, ReportLengths::default());
    let hid = Arc::new(MockHid::with_recorder(recorder.clone()).add_endpoint(ep.clone()));
    let info = hub(0x9999, true);
    let usb = Arc::new(MockUsb::with_recorder(recorder.clone()).add_device(info.clone()));
    let device =
        SessionDevice::hid("1234:9999:U1", 0x1234, 0x9999, vec![ep.clone()]).with_usb(Some(UsbTarget {
            backend: usb,
            info: info.clone(),
        }));
    let r = run(plugin(0x9999), device, hid, SessionOptions::default(), 2);
    assert!(r.report.failures.is_empty(), "{:#?}", r.report.failures);
    // setProtocol("hid") closed the USB handle (bulk fails), "hybrid" reopened it.
    assert_eq!(log_line(&r.report, "protocol "), "2 -1 1");
    let usb_ops: Vec<(TransactionKind, Vec<u8>)> = recorder
        .snapshot()
        .into_iter()
        .filter(|t| t.target == info.path)
        .map(|t| (t.kind, t.bytes))
        .collect();
    assert_eq!(
        usb_ops,
        vec![
            (TransactionKind::Open, vec![]),
            (TransactionKind::UsbClaim, vec![1]),
            (TransactionKind::BulkOut, vec![0x02, 0x05, 0x06]),
            (TransactionKind::Close, vec![]),
            (TransactionKind::Open, vec![]),
            (TransactionKind::UsbClaim, vec![1]),
            (TransactionKind::BulkOut, vec![0x02, 0x08]),
            (TransactionKind::Close, vec![]),
        ]
    );
    let writes = recorder.writes();
    assert_eq!(writes[0], vec![0x00, 0x01]);
    assert!(writes[1..].iter().all(|w| w == &vec![0x00, 0x02]), "{writes:?}");
}

#[test]
fn a_hybrid_session_without_winusb_keeps_its_hid_side() {
    let ep = MockHid::endpoint_info(0x1234, 0x9999, "U1", 0, 0xFF00, 1, 0, ReportLengths::default());
    let hid = Arc::new(MockHid::new().add_endpoint(ep.clone()));
    let info = hub(0x9999, false);
    let device = SessionDevice::hid("1234:9999:U1", 0x1234, 0x9999, vec![ep]).with_usb(Some(UsbTarget {
        backend: Arc::new(MockUsb::new().add_device(info.clone())),
        info,
    }));
    let r = run(plugin(0x9999), device, hid, SessionOptions::default(), 2);
    assert!(r.report.faulted.is_none(), "{:#?}", r.report);
    assert!(r.report.initialized);
    assert!(
        r.report.notes.iter().any(|n| n.contains("needs WinUSB driver")),
        "{:?}",
        r.report.notes
    );
    assert_eq!(log_line(&r.report, "protocol "), "-1 -1 -1");
    assert!(r.recorder.writes().len() >= 2);
}
