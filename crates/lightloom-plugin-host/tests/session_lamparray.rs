// SPDX-License-Identifier: GPL-3.0-or-later
//! The generic HID LampArray plugin end to end: the catalog finds it by HID
//! usage for a device no plugin claims by VID:PID, the session opens the
//! LampArray collection, the host hands the plugin the report layout from the
//! collection's report descriptor (`plugins/hid-lamparray/fixtures/descriptor.hex`),
//! and the plugin queries the lamps, takes control and sends colour updates.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HidBackend, HidEndpointInfo, ReportLengths};
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::session::{self, SessionDevice, SessionOptions};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn descriptor() -> Vec<u8> {
    let text = std::fs::read_to_string(repo().join("plugins/hid-lamparray/fixtures/descriptor.hex")).unwrap();
    let hex: String = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn le(value: u32, bytes: usize) -> Vec<u8> {
    value.to_le_bytes()[..bytes].to_vec()
}

#[test]
fn a_lamp_array_device_is_driven_by_the_generic_plugin() {
    // A mouse-like device nobody has a plugin for: a mouse collection and a LampArray one.
    let mouse = MockHid::endpoint_info(
        0x1234,
        0x5678,
        "L",
        0,
        0x0001,
        0x0002,
        0,
        ReportLengths::default(),
    );
    let lamps = MockHid::endpoint_info(
        0x1234,
        0x5678,
        "L",
        3,
        0x0059,
        0x0001,
        0,
        ReportLengths {
            input: 0,
            output: 0,
            feature: 51,
        },
    );
    let catalog = Catalog::scan(&[repo().join("plugins")], Duration::from_secs(10));
    let endpoints = vec![mouse.clone(), lamps.clone()];
    let plugin = catalog
        .lookup_device(0x1234, 0x5678, &endpoints)
        .expect("the LampArray plugin claims the device by usage")
        .clone();
    assert_eq!(plugin.meta.name, "HID LampArray");

    let mock = MockHid::new().add_endpoint(mouse).add_endpoint(lamps.clone());
    mock.endpoint(&lamps.path)
        .unwrap()
        .lock()
        .unwrap()
        .report_descriptor = Some(descriptor());
    // Attributes: 2 lamps in a 60 x 120 mm box, a mouse (kind 2), 8 ms between updates.
    let mut attributes = vec![1];
    for (value, bytes) in [(2, 2), (60_000, 4), (120_000, 4), (40_000, 4), (2, 4), (8_000, 4)] {
        attributes.extend(le(value, bytes));
    }
    mock.set_feature_report(&lamps.path, attributes).unwrap();
    // The lamp the device describes (the mock answers every request with lamp 0).
    let mut response = vec![3];
    for (value, bytes) in [(0, 2), (30_000, 4), (100_000, 4), (0, 4), (1_000, 4), (1, 4)] {
        response.extend(le(value, bytes));
    }
    response.extend([0xFF, 0xFF, 0xFF, 0x01, 0x01, 0x00]);
    mock.set_feature_report(&lamps.path, response).unwrap();
    let mock = Arc::new(mock);

    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let red = Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(255, 0, 0));
    frames.publish(red.clone());
    let backend: Arc<dyn HidBackend> = mock.clone();
    let device = SessionDevice {
        smbus: None,
        key: "1234:5678:L".into(),
        vid: 0x1234,
        pid: 0x5678,
        endpoints: endpoints as Vec<HidEndpointInfo>,
    };
    let handle = session::spawn(
        device,
        plugin,
        backend,
        Arc::clone(&frames),
        SessionOptions::default(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.live().frames < 2 && !handle.is_finished() && Instant::now() < deadline {
        frames.publish(red.clone());
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.stop();
    let report = handle.join().expect("session thread");
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    assert!(report.faulted.is_none(), "{:?}", report.faulted);
    assert_eq!(
        (report.endpoints, report.validated),
        (2, 1),
        "only the LampArray collection validates"
    );

    let sends = mock.recorder().feature_sends();
    let ids: Vec<u8> = sends.iter().map(|s| s[0]).collect();
    // Lamp 0 and lamp 1 asked for, control taken, one update for both lamps, control given back.
    assert_eq!(ids, vec![2, 2, 6, 4, 6], "{sends:02x?}");
    assert_eq!(sends[0], vec![2, 0, 0]);
    assert_eq!(sends[1], vec![2, 1, 0]);
    assert_eq!(sends[2], vec![6, 0]);
    let update = &sends[3];
    assert_eq!(update.len(), 51);
    assert_eq!(&update[1..3], &[2, 1], "2 lamps, update complete");
    assert_eq!(&update[3..7], &[0, 0, 1, 0], "lamp ids 0 and 1");
    // 255 red levels (0..=254) as the lamp reports; intensity at its field's maximum.
    assert_eq!(
        &update[19..27],
        &[254, 0, 0, 255, 254, 0, 0, 255],
        "red at full intensity"
    );
    assert_eq!(sends[4], vec![6, 1]);
}
