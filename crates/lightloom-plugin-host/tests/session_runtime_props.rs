// SPDX-License-Identifier: GPL-3.0-or-later
//! Run-time parameters and messages in a device session
//! (docs/host-api/components-channels.md §2): `device.addProperty` /
//! `removeProperty` / `getProperty`, `addMessage` / `removeMessage`,
//! `notify` / `denotify` reach the handle's plugin UI and are announced on
//! the session's events channel; numeric strings are coerced; refused objects
//! are logged with the exe's messages.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::meta::{ParamType, ParamValue};
use lightloom_plugin_host::session::{self, Control, SessionDevice, SessionEvent, SessionHandle, SessionOptions};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_until(deadline: Instant, frames: &FrameStore, mut done: impl FnMut() -> bool) {
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(0, 0, 0)));
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn start() -> (SessionHandle, Arc<MockHid>, Arc<FrameStore>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x8888)
        .expect("runtime properties probe")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x8888, "R", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(0, 0, 0)));
    let handle = session::spawn(
        SessionDevice::hid("1234:8888:R", 0x1234, 0x8888, vec![info]),
        plugin,
        backend,
        Arc::clone(&frames),
        SessionOptions::default(),
    )
    .unwrap();
    (handle, mock, frames)
}

fn names(handle: &SessionHandle) -> Vec<String> {
    handle
        .plugin_ui()
        .map(|ui| ui.properties().iter().map(|p| p.property.clone()).collect())
        .unwrap_or_default()
}

fn message_ids(handle: &SessionHandle) -> Vec<String> {
    handle
        .plugin_ui()
        .map(|ui| ui.messages().iter().map(|m| m.id.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn properties_and_messages_change_at_run_time() {
    let (handle, mock, frames) = start();
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, &frames, || handle.live().frames >= 1 || handle.is_finished());

    // After Initialize: the export plus `dpi`, the exported message plus `hello`.
    let ui = handle.plugin_ui().expect("published after Initialize");
    assert_eq!(names(&handle), ["mode", "dpi"]);
    let dpi = &ui.properties()[1];
    assert_eq!(dpi.param_type, ParamType::Number);
    assert_eq!(dpi.default, Some(ParamValue::Number(800.0)));
    assert_eq!((dpi.min, dpi.max, dpi.step, dpi.order), (Some(100.0), Some(1600.0), Some(100.0), Some(1)));
    assert!(!dpi.live);
    assert_eq!(dpi.group.as_deref(), Some("dpi"));
    assert_eq!(message_ids(&handle), ["fw", "hello"]);
    assert_eq!(ui.messages()[0].tooltip.as_deref(), Some("Update in the vendor app"));
    assert_eq!(ui.messages()[1].tooltip, None);
    let events = handle.events();
    assert!(events.contains(&SessionEvent::Properties), "{events:?}");
    assert!(events.contains(&SessionEvent::Messages), "{events:?}");
    // The `dpi` global holds the coerced default.
    assert_eq!(mock.recorder().writes()[0], vec![0x00, 8], "{:?}", mock.recorder().writes());

    // A run-time property takes values like an exported one.
    handle.control(Control::Parameters(json!({ "dpi": 1200 }).as_object().unwrap().clone()));
    wait_until(deadline, &frames, || {
        mock.recorder().writes().iter().any(|w| w[..] == [0x01, 12]) || handle.is_finished()
    });

    // Mode "b": dpi removed, mode relabelled in place, keycode added, the
    // exported message dropped, a notification raised.
    handle.control(Control::Parameters(json!({ "mode": "b" }).as_object().unwrap().clone()));
    wait_until(deadline, &frames, || names(&handle) == ["mode", "key"] || handle.is_finished());
    let ui = handle.plugin_ui().unwrap();
    assert_eq!(ui.properties()[0].label, "Mode B");
    assert_eq!(ui.properties()[0].values, ["a", "b"]);
    assert_eq!(ui.properties()[1].param_type, ParamType::Keycode);
    assert_eq!(ui.properties()[1].default, Some(ParamValue::Text("F1".into())));
    assert_eq!(message_ids(&handle), ["hello", "lightloom-alert-1"]);
    let alert = &ui.messages()[1];
    assert_eq!(
        (alert.title.as_deref(), alert.message.as_str(), alert.priority),
        (Some("Heads up"), "Mode b is on", Some(2))
    );
    let events = handle.events();
    assert!(events.contains(&SessionEvent::Properties), "{events:?}");
    assert!(events.contains(&SessionEvent::Messages), "{events:?}");

    // Mode "a": denotify withdraws the notification; the properties stay.
    handle.control(Control::Parameters(json!({ "mode": "a" }).as_object().unwrap().clone()));
    wait_until(deadline, &frames, || message_ids(&handle) == ["hello"] || handle.is_finished());
    assert_eq!(handle.events(), [SessionEvent::Messages]);

    // Nothing changes from here on: no more events.
    let seen = handle.live().frames;
    wait_until(deadline, &frames, || handle.live().frames >= seen + 3 || handle.is_finished());
    assert_eq!(handle.events(), []);

    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.is_clean(), "{report:#?}");
    let log = report.log_tail.join("\n");
    assert!(log.contains("names mode,dpi dpi number 800"), "{log}");
    assert!(log.contains("addProperty(): parameter must be an object!"), "{log}");
    assert!(log.contains("addProperty(): object requires a 'property' parameter!"), "{log}");
    assert!(log.contains("notify: Heads up: Mode b is on"), "{log}");
}
