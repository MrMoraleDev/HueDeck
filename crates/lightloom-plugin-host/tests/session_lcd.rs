// SPDX-License-Identifier: GPL-3.0-or-later
//! `@SignalRGB/lcd` in a device session, with the mock face provider (no renderer):
//! `LCD.initialize` attaches at the plugin's size, `getFrame` hands out the face encoded
//! as asked, the other members and the typed errors reach the plugin, and the session's
//! end detaches the face.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_lcd::provider::{mock_face_frame, MockEvent};
use lightloom_lcd::{encode, LcdFormat, LcdProvider, MockLcdProvider};
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::session::{self, SessionDevice, SessionOptions};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn lcd_frames_reach_the_device_encoded() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x1cd0)
        .expect("lcd probe")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x1cd0, "L", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let faces = Arc::new(MockLcdProvider::new());
    let handle = session::spawn(
        SessionDevice {
            usb: None,
            smbus: None,
            lamparray: None,
            serial: None,
            key: "1234:1cd0:L".into(),
            vid: 0x1234,
            pid: 0x1cd0,
            endpoints: vec![info],
            network: None,
        },
        plugin,
        backend,
        Arc::clone(&frames),
        SessionOptions {
            lcd: Some(faces.clone() as Arc<dyn LcdProvider>),
            ..SessionOptions::default()
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while mock.recorder().writes().len() < 2 && !handle.is_finished() {
        assert!(Instant::now() < deadline, "timed out");
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(1, 2, 3)));
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.is_clean(), "{report:#?}");

    let face = mock_face_frame(4, 2);
    let rgb565 = encode(&face, LcdFormat::Rgb565, None).unwrap();
    let writes = mock.recorder().writes();
    assert_eq!(writes[0][0], 0x00);
    assert_eq!(
        &writes[0][1..],
        &rgb565[..],
        "the face, RGB565, after the report id"
    );

    let probe = report
        .log_tail
        .iter()
        .find_map(|l| l.strip_prefix("probe "))
        .expect("probe line");
    let probe: serde_json::Value = serde_json::from_str(probe).unwrap();
    let rgb = encode(&face, LcdFormat::Rgb, None).unwrap();
    assert_eq!(
        probe,
        serde_json::json!({
            "before": { "initialized": false, "early": "LcdError NOT_INITIALIZED" },
            "badSettings": "LcdError INVALID_SETTINGS",
            "badSize": "LcdError INVALID_SETTINGS",
            "initialized": true,
            "isArray": true,
            "rgb": rgb.len(),
            "rgbFirst": [rgb[0], rgb[1], rgb[2]],
            "jpegMagic": [0xFF, 0xD8],
            "cells": [2 * 2 * 4, 2 * 2 * 4],
            "crop": 4 * 4 * 4,
            "q565": "LcdError UNSUPPORTED_FORMAT",
            "nzxt565": "LcdError UNSUPPORTED_FORMAT",
            "gif": "LcdError INVALID_FORMAT",
            "noFormat": "LcdError INVALID_FORMAT",
            "grid": "LcdError INVALID_GRID",
            "bounds": "LcdError OUT_OF_BOUNDS",
        })
    );

    let lcd = report.lcd.expect("lcd stats");
    assert_eq!(
        lcd.spec.map(|s| (s.width, s.height, s.circular)),
        Some((4, 2, true))
    );
    assert_eq!(lcd.attaches, 1);
    assert!(lcd.frames >= 6, "{lcd:?}");
    assert_eq!(lcd.format.as_deref(), Some("RGB565"));
    // getFrame early, badSize, Q565, NZXT565, GIF, grid, bounds (the JS-side checks never
    // reach the native).
    assert_eq!(lcd.errors, 7, "{lcd:?}");

    let events = faces.events();
    let MockEvent::Attach { key, spec, id } = &events[0] else {
        panic!("{events:?}")
    };
    assert_eq!((key.as_str(), spec.width, spec.height), ("1234:1cd0:L", 4, 2));
    assert_eq!(
        events[1..],
        [MockEvent::Detach {
            key: key.clone(),
            id: *id
        }],
        "the session's end detaches the face"
    );
}
