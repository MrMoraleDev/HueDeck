// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Golden test of a full device session (M1): the first-party MSI Mystic
//! Light plugin runs through the same catalog → session → device-host path
//! the CLI uses on hardware, against a scripted mock HID endpoint. Expected
//! packets come from `plugins/msi-mystic-light-7c91/fixtures/expected-packet.json`,
//! which was produced independently of the plugin.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::{MockHid, TransactionKind};
use lightloom_hal::{HidBackend, HidEndpointInfo, ReportLengths};
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry};
use lightloom_plugin_host::device::{IoKind, WriteMode};
use lightloom_plugin_host::session::{self, RestoreProbe, SessionDevice, SessionOptions, SessionReport};
use std::path::PathBuf;
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

fn case(fx: &serde_json::Value, name: &str) -> Vec<u8> {
    let c = fx["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("fixture case {name}"));
    hex(c["expectedPacket"].as_str().unwrap())
}

fn msi_plugin() -> CatalogEntry {
    let catalog = Catalog::scan(&[repo().join("plugins")], Duration::from_secs(10));
    catalog
        .lookup(0x1462, 0x7C91)
        .expect("MSI plugin in the catalog")
        .clone()
}

fn endpoint(interface: i32) -> HidEndpointInfo {
    MockHid::endpoint_info(
        0x1462,
        0x7C91,
        "A00000000001",
        interface,
        0x0001,
        0x0000,
        0,
        ReportLengths {
            input: 64,
            output: 64,
            feature: 725,
        },
    )
}

struct Rig {
    mock: Arc<MockHid>,
    device: SessionDevice,
    frames: Arc<FrameStore>,
}

fn rig(interface: i32) -> Rig {
    let info = endpoint(interface);
    let mock = MockHid::new().add_endpoint(info.clone());
    mock.set_feature_report(&info.path, hex(fixture()["restoreImage"].as_str().unwrap()))
        .unwrap();
    let mock = Arc::new(mock);
    let device = SessionDevice {
        key: "1462:7c91:A00000000001".into(),
        vid: 0x1462,
        pid: 0x7C91,
        endpoints: mock.enumerate().unwrap(),
    };
    Rig {
        mock,
        device,
        frames: Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT)),
    }
}

/// Publish `color`, wait until the session rendered `frames` frames, stop, join.
fn run(rig: &Rig, opts: SessionOptions, color: Rgb, frames: u64) -> SessionReport {
    rig.frames
        .publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, color));
    let backend: Arc<dyn HidBackend> = rig.mock.clone();
    let handle = session::spawn(
        rig.device.clone(),
        msi_plugin(),
        backend,
        Arc::clone(&rig.frames),
        opts,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.live().frames < frames && !handle.is_finished() && Instant::now() < deadline {
        rig.frames
            .publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, color));
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.stop();
    handle.join().expect("session thread")
}

#[test]
fn solid_red_frame_drives_the_golden_packet_and_shutdown_restores() {
    let fx = fixture();
    let rig = rig(0);
    let report = run(
        &rig,
        SessionOptions {
            restore_probe: Some(RestoreProbe {
                report_id: 0x52,
                len: 185,
            }),
            ..SessionOptions::default()
        },
        Rgb::new(255, 0, 0),
        3,
    );
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    assert!(report.faulted.is_none(), "{:?}", report.faulted);
    assert!(report.initialized && report.shutdown_called);
    assert!(report.frames >= 3, "{}", report.frames);
    assert_eq!((report.endpoints, report.validated), (1, 1));
    assert_eq!(report.io.errors(), 0);

    let sends = rig.mock.recorder().feature_sends();
    // One packet for the colour (unchanged frames are not resent within the
    // plugin's 1 s keepalive), then the restore image on Shutdown(false).
    assert_eq!(sends.len(), 2, "{} sends", sends.len());
    assert_eq!(sends[0], case(&fx, "solid red via device.color"));
    assert_eq!(sends[1], hex(fx["expectedRestorePacket"].as_str().unwrap()));

    // The device was read before Initialize (plugin restore image) and the
    // restore check read it again after Shutdown: only the save flag (0xB8),
    // which Lightloom always clears, differs from the synthetic fixture image.
    let check = report.restore_check.expect("restore check");
    assert_eq!(check.differing_offsets, vec![0xB8]);
    let kinds: Vec<TransactionKind> = rig.mock.recorder().snapshot().iter().map(|t| t.kind).collect();
    assert_eq!(kinds.first(), Some(&TransactionKind::Open));
    assert_eq!(kinds.last(), Some(&TransactionKind::Close));
    assert!(report.first_write_ms.is_some());
    assert!(
        report.render.count >= 3 && report.render.max_ms < 250.0,
        "{:?}",
        report.render
    );
}

#[test]
fn forced_mode_parameters_override_the_canvas() {
    let fx = fixture();
    let rig = rig(0);
    let mut parameters = serde_json::Map::new();
    parameters.insert("LightingMode".into(), "Forced".into());
    parameters.insert("forcedColor".into(), "#00FF00".into());
    let report = run(
        &rig,
        SessionOptions {
            parameters,
            ..SessionOptions::default()
        },
        Rgb::new(0x12, 0x34, 0x56),
        1,
    );
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    let sends = rig.mock.recorder().feature_sends();
    assert_eq!(sends[0], case(&fx, "forced colour #00FF00"));
}

#[test]
fn dry_run_reads_the_device_but_never_writes() {
    let fx = fixture();
    let rig = rig(0);
    let report = run(
        &rig,
        SessionOptions {
            write_mode: WriteMode::DryRun,
            ..SessionOptions::default()
        },
        Rgb::new(255, 0, 0),
        2,
    );
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    assert!(
        rig.mock.recorder().feature_sends().is_empty(),
        "dry-run must not send"
    );
    let dry: Vec<_> = report
        .io_log
        .iter()
        .filter(|r| r.kind == IoKind::SendFeature)
        .collect();
    assert!(dry.iter().all(|r| r.dry_run));
    assert_eq!(dry[0].bytes, case(&fx, "solid red via device.color"));
    assert_eq!(
        report.io_log[0].kind,
        IoKind::GetFeature,
        "the restore image was read for real"
    );
}

#[test]
fn endpoint_rejected_by_validate_is_never_opened() {
    let rig = rig(2);
    let report = run(&rig, SessionOptions::default(), Rgb::new(255, 0, 0), 1);
    assert_eq!(report.validated, 0);
    assert!(report.faulted.as_deref().unwrap_or("").contains("Validate"));
    assert!(rig.mock.recorder().is_empty(), "nothing opened, nothing sent");
}
