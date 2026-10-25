// SPDX-License-Identifier: GPL-3.0-or-later
//! The `keyboard` / `mouse` feature globals in a device session: their events
//! reach the session's input sink with the device key (the macro runtime), a
//! `sendHid` without `released` throws, and `keyboard.sendKey` stays undefined.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::input::PluginInput;
use lightloom_plugin_host::session::{self, SessionDevice, SessionOptions};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
fn feature_events_reach_the_input_sink() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x2223)
        .expect("input probe")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x2223, "I", 0, 0xFF00, 1, 0, ReportLengths::default());
    let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let seen: Arc<Mutex<Vec<(String, PluginInput)>>> = Arc::default();
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let (s, l) = (Arc::clone(&seen), Arc::clone(&lines));
    let handle = session::spawn(
        SessionDevice {
            usb: None,
            smbus: None,
            lamparray: None,
            serial: None,
            key: "1234:2223:I".into(),
            vid: 0x1234,
            pid: 0x2223,
            endpoints: vec![info],
            network: None,
        },
        plugin,
        backend,
        Arc::clone(&frames),
        SessionOptions {
            input_sink: Some(Arc::new(move |key: &str, e| s.lock().unwrap().push((key.to_string(), e)))),
            log_sink: Some(Arc::new(move |line: &str| l.lock().unwrap().push(line.to_string()))),
            ..SessionOptions::default()
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.live().frames < 1 && !handle.is_finished() {
        assert!(Instant::now() < deadline, "timed out");
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(1, 2, 3)));
        std::thread::sleep(Duration::from_millis(5));
    }
    let seen = seen.lock().unwrap().clone();
    let key = "1234:2223:I".to_string();
    assert_eq!(
        seen,
        vec![
            (
                key.clone(),
                PluginInput::Key {
                    key: "G1".into(),
                    key_code: 0,
                    released: false
                }
            ),
            (
                key.clone(),
                PluginInput::Button {
                    name: "DPI Up".into(),
                    button_code: 0,
                    released: true
                }
            ),
            (
                key.clone(),
                PluginInput::KeyHid {
                    vk: 0xaf,
                    released: false
                }
            ),
            (
                key,
                PluginInput::ButtonHid {
                    vk: 5,
                    released: true,
                    xbutton: Some(1)
                }
            ),
        ]
    );
    let probe = lines
        .lock()
        .unwrap()
        .iter()
        .find_map(|l| l.strip_prefix("probe ").map(str::to_string))
        .expect("probe line");
    let v: serde_json::Value = serde_json::from_str(&probe).unwrap();
    assert_eq!(v["sendKey"], "undefined");
    assert_eq!(v["threw"], "sendHid(): released option must be specified!");
    handle.stop();
}
