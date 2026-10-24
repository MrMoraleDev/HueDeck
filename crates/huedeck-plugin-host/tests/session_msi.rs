// SPDX-License-Identifier: GPL-3.0-or-later
//! Golden test of a full device session: the first-party MSI Mystic Light
//! plugin (per-LED mode) runs through the same catalog → session → device-host
//! path the CLI uses on hardware, against a scripted mock HID endpoint. The
//! reset and configuration packets come from
//! `plugins/msi-mystic-light-7c91/fixtures/per-led.json` (recorded on the
//! wire); the colour streams are built here from the documented slot order.

use huedeck_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use huedeck_hal::mock::{MockHid, TransactionKind};
use huedeck_hal::{HidBackend, HidEndpointInfo, ReportLengths, ReportSpec};
use huedeck_plugin_host::catalog::{Catalog, CatalogEntry};
use huedeck_plugin_host::device::{IoKind, WriteMode};
use huedeck_plugin_host::session::{
    self, RestoreCheckMode, RestoreProbe, SessionDevice, SessionOptions, SessionReport,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> serde_json::Value {
    let text =
        std::fs::read_to_string(repo().join("plugins/msi-mystic-light-7c91/fixtures/per-led.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn packet(fx: &serde_json::Value, name: &str) -> Vec<u8> {
    hex(fx[name]
        .as_str()
        .unwrap_or_else(|| panic!("fixture packet {name}")))
}

/// Slots a session lights: 6 onboard LEDs, JRGB1, JRGB2 and the two channels'
/// default strips of 60 LEDs.
const LIT: usize = 6 + 2 + 60 + 60;

/// The 0x53 colour stream with its first `lit` slots in `rgb`, the rest black.
fn stream(rgb: [u8; 3], lit: usize) -> Vec<u8> {
    let mut s = vec![0u8; 725];
    s[..5].copy_from_slice(&[0x53, 0x25, 0x06, 0x00, 0x00]);
    for slot in 0..lit {
        s[5 + 3 * slot..8 + 3 * slot].copy_from_slice(&rgb);
    }
    s
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
    rig_with(endpoint(interface))
}

fn rig_with(info: HidEndpointInfo) -> Rig {
    let mock = MockHid::new().add_endpoint(info.clone());
    mock.set_feature_report(&info.path, hex(fixture()["restoreImage"].as_str().unwrap()))
        .unwrap();
    let mock = Arc::new(mock);
    let device = SessionDevice {
        usb: None,
        smbus: None,
        key: "1462:7c91:A00000000001".into(),
        vid: 0x1462,
        pid: 0x7C91,
        endpoints: mock.enumerate().unwrap(),
        network: None,
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
fn solid_red_frame_streams_every_led_and_shutdown_restores() {
    let fx = fixture();
    let rig = rig(0);
    let report = run(
        &rig,
        SessionOptions {
            restore_check: RestoreCheckMode::Reports(vec![RestoreProbe {
                report_id: 0x52,
                len: 185,
            }]),
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
    // The recorded reset and configuration, one stream for the colour
    // (unchanged frames are not resent within the plugin's 1 s keepalive),
    // then a black stream and the restore image on Shutdown(false).
    assert_eq!(sends.len(), 5, "{} sends", sends.len());
    assert_eq!(sends[0], packet(&fx, "reset"));
    assert_eq!(sends[1], packet(&fx, "config_60_60"));
    assert_eq!(sends[2], stream([255, 0, 0], LIT));
    assert_eq!(sends[3], stream([0, 0, 0], 0));
    assert_eq!(sends[4], packet(&fx, "expectedRestorePacket"));
    assert_eq!(rig.mock.recorder().writes(), vec![packet(&fx, "ezLedQuery")]);

    // The device was read before Initialize (plugin restore image) and the
    // restore check read it again after Shutdown. The mock stores what was
    // written verbatim, so two bytes differ from the synthetic fixture image:
    // 0x3E, which the plugin writes as read - 4 (the real controller reads it
    // back as written + 4), and the save flag 0xB8, always cleared.
    assert_eq!(report.restore_checks.len(), 1, "{:?}", report.restore_notes);
    assert_eq!(report.restore_checks[0].differing_offsets, vec![0x3E, 0xB8]);
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
    assert_eq!(sends[2], stream([0, 255, 0], LIT));
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
    assert_eq!(dry[1].bytes, packet(&fx, "config_60_60"));
    assert_eq!(dry[2].bytes, stream([255, 0, 0], LIT));
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

#[test]
fn declared_feature_reports_are_compared_without_naming_them() {
    let mut info = endpoint(0);
    // What the Windows backend reads from the report descriptor at open time.
    info.feature_reports = vec![
        ReportSpec { id: 0x52, len: 185 },
        ReportSpec { id: 0x53, len: 725 },
    ];
    let rig = rig_with(info);
    let report = run(
        &rig,
        SessionOptions {
            restore_check: RestoreCheckMode::DeclaredFeatureReports,
            ..SessionOptions::default()
        },
        Rgb::new(255, 0, 0),
        1,
    );
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    // 0x52 is compared; 0x53 is declared but the mock holds no such report,
    // so it is reported as unreadable instead of being compared.
    let ids: Vec<u8> = report.restore_checks.iter().map(|c| c.report_id).collect();
    assert_eq!(ids, vec![0x52]);
    assert_eq!(report.restore_checks[0].differing_offsets, vec![0x3E, 0xB8]);
    assert_eq!(report.restore_notes.len(), 1, "{:?}", report.restore_notes);
    assert!(report.restore_notes[0].contains("0x53"));
}

#[test]
fn declared_mode_without_a_descriptor_says_so() {
    let rig = rig(0);
    let report = run(
        &rig,
        SessionOptions {
            restore_check: RestoreCheckMode::DeclaredFeatureReports,
            ..SessionOptions::default()
        },
        Rgb::new(255, 0, 0),
        1,
    );
    assert!(report.restore_checks.is_empty());
    assert!(report.restore_notes[0].contains("declares no feature reports"));
}
