// SPDX-License-Identifier: GPL-3.0-or-later
//! Device-session robustness (M1 review): a panic in a native or a driver
//! must not skip `Shutdown(false)`, and members a device session does not
//! implement must throw `HostApiMissing` instead of answering with stub data.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HalResult, HidBackend, HidEndpointInfo, HidHandle, ReportLengths};
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry};
use lightloom_plugin_host::session::{self, SessionDevice, SessionOptions, SessionReport, SessionState};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> serde_json::Value {
    let text =
        std::fs::read_to_string(repo().join("plugins/msi-mystic-light-7c91/fixtures/expected-packet.json"))
            .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn red_packet(fx: &serde_json::Value) -> Vec<u8> {
    let c = fx["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "solid red via device.color")
        .unwrap();
    hex(c["expectedPacket"].as_str().unwrap())
}

fn msi_plugin() -> CatalogEntry {
    let catalog = Catalog::scan(&[repo().join("plugins")], Duration::from_secs(10));
    catalog.lookup(0x1462, 0x7C91).expect("MSI plugin").clone()
}

/// A backend whose handles panic on feature sends while `panics` is non-zero
/// (each panic consumes one): what a buggy driver or host native looks like.
struct PanickyBackend {
    inner: Arc<MockHid>,
    panics: Arc<AtomicU32>,
}

struct PanickyHandle {
    inner: Box<dyn HidHandle>,
    panics: Arc<AtomicU32>,
}

impl HidBackend for PanickyBackend {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        self.inner.enumerate()
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        Ok(Box::new(PanickyHandle {
            inner: self.inner.open(path)?,
            panics: Arc::clone(&self.panics),
        }))
    }
}

impl HidHandle for PanickyHandle {
    fn info(&self) -> &HidEndpointInfo {
        self.inner.info()
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        self.inner.write(data)
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        self.inner.read_timeout(buf, timeout_ms)
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        // One plugin thread uses the handle: load + store is enough here.
        let left = self.panics.load(Ordering::SeqCst);
        if left > 0 {
            self.panics.store(left - 1, Ordering::SeqCst);
            panic!("simulated driver panic");
        }
        self.inner.send_feature_report(data)
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        self.inner.get_feature_report(buf)
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        self.inner.get_input_report(buf)
    }
}

struct MsiRig {
    mock: Arc<MockHid>,
    device: SessionDevice,
    frames: Arc<FrameStore>,
}

fn msi_rig() -> MsiRig {
    let info = MockHid::endpoint_info(
        0x1462,
        0x7C91,
        "A00000000001",
        0,
        0x0001,
        0x0000,
        0,
        ReportLengths {
            input: 64,
            output: 64,
            feature: 725,
        },
    );
    let mock = MockHid::new().add_endpoint(info.clone());
    mock.set_feature_report(&info.path, hex(fixture()["restoreImage"].as_str().unwrap()))
        .unwrap();
    MsiRig {
        mock: Arc::new(mock),
        device: SessionDevice {
            key: "1462:7c91:A00000000001".into(),
            vid: 0x1462,
            pid: 0x7C91,
            endpoints: vec![info],
        },
        frames: Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT)),
    }
}

fn wait_until(deadline: Instant, mut done: impl FnMut() -> bool) {
    while !done() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_panic_before_the_first_write_faults_without_touching_the_device() {
    let rig = msi_rig();
    rig.frames
        .publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(255, 0, 0)));
    let backend: Arc<dyn HidBackend> = Arc::new(PanickyBackend {
        inner: rig.mock.clone(),
        panics: Arc::new(AtomicU32::new(1)),
    });
    let handle = session::spawn(
        rig.device.clone(),
        msi_plugin(),
        backend,
        Arc::clone(&rig.frames),
        SessionOptions::default(),
    )
    .unwrap();
    wait_until(Instant::now() + Duration::from_secs(20), || handle.is_finished());
    handle.stop();
    let report = handle.join().expect("the panic is caught inside the session");
    assert!(
        report.faulted.as_deref().unwrap_or("").contains("panic"),
        "{:?}",
        report.faulted
    );
    assert!(report.shutdown_called, "{:#?}", report.failures);
    assert!(!report.is_clean());
    // The Render packet panicked before reaching the device, so the plugin never wrote
    // and its Shutdown(false) has nothing to restore.
    assert!(rig.mock.recorder().feature_sends().is_empty());
}

#[test]
fn a_panic_after_a_write_still_restores_the_device() {
    let fx = fixture();
    let rig = msi_rig();
    let panics = Arc::new(AtomicU32::new(0));
    let backend: Arc<dyn HidBackend> = Arc::new(PanickyBackend {
        inner: rig.mock.clone(),
        panics: Arc::clone(&panics),
    });
    rig.frames
        .publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(255, 0, 0)));
    let handle = session::spawn(
        rig.device.clone(),
        msi_plugin(),
        backend,
        Arc::clone(&rig.frames),
        SessionOptions::default(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, || handle.live().frames >= 1);
    // The next (changed) packet panics in the "driver".
    panics.store(1, Ordering::SeqCst);
    rig.frames
        .publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(0, 0, 255)));
    wait_until(deadline, || handle.is_finished());
    handle.stop();
    let report = handle.join().expect("the panic is caught inside the session");
    assert!(report.faulted.as_deref().unwrap_or("").contains("panic"));
    assert!(report.shutdown_called);
    let sends = rig.mock.recorder().feature_sends();
    assert_eq!(sends.len(), 2, "red packet, then the restore image");
    assert_eq!(sends[0], red_packet(&fx));
    assert_eq!(sends[1], hex(fx["expectedRestorePacket"].as_str().unwrap()));
}

fn gap_probe_plugin() -> CatalogEntry {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let catalog = Catalog::scan(&[dir], Duration::from_secs(10));
    assert!(
        catalog.lookup(0x1234, 0x9999).is_none(),
        "hybrid plugins are refused"
    );
    assert!(catalog
        .skipped()
        .iter()
        .any(|(p, why)| p.ends_with("hybrid.js") && why.contains("Hybrid")));
    catalog.lookup(0x1234, 0x5678).expect("gap probe").clone()
}

fn run_probe(probe: &str) -> (SessionReport, Arc<MockHid>) {
    let info = MockHid::endpoint_info(0x1234, 0x5678, "P", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let mut parameters = serde_json::Map::new();
    parameters.insert("probe".into(), probe.into());
    let backend: Arc<dyn HidBackend> = mock.clone();
    let handle = session::spawn(
        SessionDevice {
            key: "1234:5678:P".into(),
            vid: 0x1234,
            pid: 0x5678,
            endpoints: vec![info],
        },
        gap_probe_plugin(),
        backend,
        frames,
        SessionOptions {
            parameters,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    wait_until(Instant::now() + Duration::from_secs(20), || {
        handle.live().state != SessionState::Starting
    });
    handle.stop();
    (handle.join().unwrap(), mock)
}

#[test]
fn unimplemented_host_members_throw_instead_of_faking_io() {
    for probe in [
        "bulk",
        "control",
        "setProtocol",
        "fan",
        "board",
        "bus",
        "udp",
        "unknown",
    ] {
        let (report, mock) = run_probe(probe);
        let init = report
            .failures
            .iter()
            .find(|f| f.phase == "Initialize")
            .unwrap_or_else(|| panic!("{probe}: Initialize should fail: {report:#?}"));
        assert_eq!(init.error.class, "HostApiMissing", "{probe}: {:?}", init.error);
        assert!(mock.recorder().writes().is_empty(), "{probe}: nothing written");
        assert!(!report.is_clean());
    }
    // Faithful answers stay available: fan control is reported as disabled,
    // subdevices are real, system info is real (none known here: undefined).
    for probe in ["fanflag", "subdevice", "systeminfo"] {
        let (report, mock) = run_probe(probe);
        assert!(report.failures.is_empty(), "{probe}: {:#?}", report.failures);
        assert_eq!(mock.recorder().writes(), vec![vec![0x00, 0x01]], "{probe}");
        if probe == "subdevice" {
            assert_eq!(report.subdevices.len(), 1);
            assert_eq!(report.subdevices[0].id, "Strip");
        }
    }
}

#[test]
fn long_blocking_renders_survive_the_watchdog_and_stop_promptly() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x6666)
        .expect("slow render plugin")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x6666, "S", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let handle = session::spawn(
        SessionDevice {
            key: "1234:6666:S".into(),
            vid: 0x1234,
            pid: 0x6666,
            endpoints: vec![info],
        },
        plugin,
        backend,
        frames,
        SessionOptions::default(),
    )
    .unwrap();
    wait_until(Instant::now() + Duration::from_secs(20), || {
        handle.live().state == SessionState::Running
    });
    // Well past the 250 ms JavaScript budget: the paused time does not count.
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(handle.live().render_errors, 0);
    let asked = Instant::now();
    handle.stop();
    let report = handle.join().unwrap();
    assert!(
        asked.elapsed() < Duration::from_secs(3),
        "stop took {:?}",
        asked.elapsed()
    );
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    assert_eq!(report.render_errors, 0);
    assert!(report.shutdown_called);
    assert_eq!(mock.recorder().writes(), vec![vec![0x00, 0xff]]);
}

/// The slow-render plugin in `mode` on a mock device whose reads block.
fn slow_session(mode: &str, opts: SessionOptions) -> (session::SessionHandle, Arc<MockHid>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x6666)
        .expect("slow render plugin")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x6666, "S", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    mock.endpoint(&info.path).unwrap().lock().unwrap().block_reads = true;
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let mut opts = opts;
    opts.parameters.insert("mode".into(), mode.into());
    let handle = session::spawn(
        SessionDevice {
            key: "1234:6666:S".into(),
            vid: 0x1234,
            pid: 0x6666,
            endpoints: vec![info],
        },
        plugin,
        backend,
        frames,
        opts,
    )
    .unwrap();
    (handle, mock)
}

#[test]
fn a_render_blocked_forever_hits_its_wall_clock_cap() {
    let mut opts = SessionOptions::default();
    opts.budgets.render_wall = Duration::from_secs(1);
    opts.max_render_timeouts = 1;
    let started = Instant::now();
    let (handle, mock) = slow_session("forever", opts);
    wait_until(started + Duration::from_secs(20), || handle.is_finished());
    let report = handle.join().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
    assert!(
        report.faulted.as_deref().unwrap_or("").contains("Render()"),
        "{report:#?}"
    );
    assert!(report.shutdown_called);
    assert_eq!(mock.recorder().writes(), vec![vec![0x00, 0xff]]);
}

#[test]
fn a_blocking_read_loop_stops_promptly() {
    let (handle, mock) = slow_session("read", SessionOptions::default());
    wait_until(Instant::now() + Duration::from_secs(20), || {
        handle.live().state == SessionState::Running
    });
    std::thread::sleep(Duration::from_millis(300));
    let asked = Instant::now();
    handle.stop();
    let report = handle.join().unwrap();
    assert!(
        asked.elapsed() < Duration::from_secs(3),
        "stop took {:?}",
        asked.elapsed()
    );
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    assert_eq!(
        (report.render_errors, report.frames),
        (0, 0),
        "the cut frame is not counted"
    );
    assert!(report.shutdown_called);
    assert_eq!(mock.recorder().writes(), vec![vec![0x00, 0xff]]);
}
