// SPDX-License-Identifier: GPL-3.0-or-later
//! Channels, components and subdevices in a device session
//! (docs/host-api/components-channels.md): a synthetic ARGB-controller plugin
//! on the mock HAL, with one channel left to the placeholder strip and one
//! with a component assigned by the caller; and live controls (parameter
//! changes with their `on<name>Changed` hooks, `onBrightnessChanged`).

use lightloom_engine::{Component, ComponentInstance, Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::smbios::{RamModule, SystemInfo};
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::session::{self, SessionDevice, SessionOptions};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_until(deadline: Instant, mut done: impl FnMut() -> bool) {
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn channels_components_and_subdevices_reach_the_plugin() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x7777)
        .expect("channel probe")
        .clone();
    assert!(plugin.meta.subdevice_controller);

    let info = MockHid::endpoint_info(0x1234, 0x7777, "C", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(200, 100, 50)));

    // The caller's assignment for "ARGB 2": a 3-LED fan with an extra JSON key.
    let mut fan = ComponentInstance::with_id(Component::strip("Probe Fan".into(), 3), "comp-a".into());
    fan.data
        .insert("Extra".into(), serde_json::json!({ "kept": true }));
    let mut opts = SessionOptions::default();
    opts.channel_components.insert("ARGB 2".into(), vec![fan]);
    let mut info = SystemInfo::default();
    info.motherboard.model = "MAG B550 TOMAHAWK (MS-7C91)".into();
    info.bios.release_date = "2021-06-29".into();
    info.ram.push(RamModule {
        part_number: "BL8G32C16U4WL.8FE".into(),
        ..RamModule::default()
    });
    opts.system_info = Some(Arc::new(info));

    let handle = session::spawn(
        SessionDevice {
            smbus: None,
            key: "1234:7777:C".into(),
            vid: 0x1234,
            pid: 0x7777,
            endpoints: vec![MockHid::endpoint_info(
                0x1234,
                0x7777,
                "C",
                0,
                0xFF00,
                1,
                0,
                ReportLengths::default(),
            )],
            network: None,
        },
        plugin,
        backend,
        frames,
        opts,
    )
    .unwrap();
    wait_until(Instant::now() + Duration::from_secs(20), || {
        handle.live().frames >= 1 || handle.is_finished()
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
    assert_eq!(probe["names"], serde_json::json!(["ARGB 1", "ARGB 2"]));
    assert_eq!(probe["missing"], true);
    assert_eq!(
        (probe["count1"].as_u64(), probe["count2"].as_u64()),
        (Some(60), Some(3))
    );
    assert_eq!(
        (probe["limit2"].as_u64(), probe["total"].as_u64()),
        (Some(10), Some(63))
    );
    assert_eq!(probe["ids"], serde_json::json!(["comp-a"]));
    assert_eq!(probe["product"], "Probe Fan");
    assert_eq!(probe["extra"], serde_json::json!({ "kept": true }));
    assert_eq!(probe["component"], "comp-a");
    assert_eq!(
        (probe["onlyDefaults1"].as_bool(), probe["onlyDefaults2"].as_bool()),
        (Some(true), Some(false))
    );
    assert_eq!(probe["subdevices"], serde_json::json!(["Logo"]));
    assert_eq!(
        (probe["pulse"].as_str(), probe["pulsing"].as_bool()),
        (Some("#009EDB"), Some(false))
    );
    assert_eq!(probe["lighting"], true);
    assert_eq!(probe["board"], "MAG B550 TOMAHAWK (MS-7C91)");
    assert_eq!(probe["bios"], "2021-06-29");
    assert_eq!(probe["ram"], serde_json::json!(["BL8G32C16U4WL.8FE"]));

    // One Render: placeholder strip colours, the fan's first LED in GRB, the subdevice.
    assert_eq!(
        mock.recorder().writes(),
        vec![
            vec![0x01, 200, 100, 50, 200, 100, 50],
            vec![0x02, 100, 200, 50],
            vec![0x03, 200, 100, 50],
        ]
    );

    assert_eq!(report.subdevices.len(), 1);
    assert_eq!(
        (report.subdevices[0].name.as_str(), report.subdevices[0].size),
        ("Probe - Logo", [2, 1])
    );
    let channels: Vec<_> = report
        .channels
        .iter()
        .map(|c| (c.name.as_str(), c.led_limit, c.led_count, c.components.len()))
        .collect();
    assert_eq!(channels, vec![("ARGB 1", 120, 60, 1), ("ARGB 2", 10, 3, 1)]);
    let first = &report.channels[0].components[0];
    assert_eq!(
        (first.name.as_str(), first.led_count, first.id.len()),
        ("Default Strip - 60", 60, 36)
    );
    let fan = &report.channels[1].components[0];
    assert_eq!(
        (fan.name.as_str(), fan.id.as_str(), fan.led_count),
        ("Probe Fan", "comp-a", 3)
    );
}

#[test]
fn controls_apply_before_the_next_render_and_call_the_hooks() {
    use lightloom_engine::Placement;
    use lightloom_plugin_host::session::Control;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x4444)
        .expect("parameter probe")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x4444, "P", 0, 0xFF00, 1, 0, ReportLengths::default());
    let mock = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let backend: Arc<dyn HidBackend> = mock.clone();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let handle = session::spawn(
        SessionDevice {
            smbus: None,
            key: "1234:4444:P".into(),
            vid: 0x1234,
            pid: 0x4444,
            endpoints: vec![info],
            network: None,
        },
        plugin,
        backend,
        Arc::clone(&frames),
        SessionOptions::default(),
    )
    .unwrap();
    wait_until(Instant::now() + Duration::from_secs(20), || {
        handle.live().frames >= 1
    });
    assert!(mock.recorder().writes().is_empty(), "no hook before a change");

    let mut values = serde_json::Map::new();
    values.insert("mode".into(), "b".into());
    assert!(handle.control(Control::Parameters(values)));
    assert!(handle.control(Control::Placement(Placement {
        brightness: 50,
        ..Placement::default()
    })));
    // Same brightness again: no hook.
    assert!(handle.control(Control::Placement(Placement {
        brightness: 50,
        ..Placement::default()
    })));
    frames.publish(Frame::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    wait_until(Instant::now() + Duration::from_secs(20), || {
        handle.live().frames >= 2
    });
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.is_clean(), "{report:#?}");
    assert_eq!(mock.recorder().writes(), vec![vec![0x00, 0x02], vec![0x00, 0x15]]);
}

#[test]
fn endpoints_are_published_and_child_placements_apply_live() {
    use lightloom_engine::{Placement, Point};
    use lightloom_plugin_host::session::{Control, EndpointKind};
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device");
    let plugin = Catalog::scan(&[dir], Duration::from_secs(10))
        .lookup(0x1234, 0x7777)
        .expect("channel probe")
        .clone();
    let info = MockHid::endpoint_info(0x1234, 0x7777, "C", 0, 0xFF00, 1, 0, ReportLengths::default());
    let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new().add_endpoint(info.clone()));
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(10, 20, 30)));
    let mut opts = SessionOptions::default();
    opts.channel_components.insert(
        "ARGB 2".into(),
        vec![ComponentInstance::with_id(
            Component::strip("Probe Fan".into(), 3),
            "comp-a".into(),
        )],
    );
    // The probe's Initialize reads @SignalRGB/systeminfo.
    opts.system_info = Some(Arc::new(SystemInfo::default()));
    let handle = session::spawn(
        SessionDevice {
            smbus: None,
            key: "1234:7777:C".into(),
            vid: 0x1234,
            pid: 0x7777,
            endpoints: vec![info],
            network: None,
        },
        plugin,
        backend,
        Arc::clone(&frames),
        opts,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    wait_until(deadline, || {
        !handle.endpoints().is_empty() || handle.is_finished()
    });
    let views = handle.endpoints();
    let kinds: Vec<_> = views.iter().map(|v| (v.uid.as_str(), v.kind)).collect();
    assert_eq!(kinds[0], ("1234:7777:C", EndpointKind::Device));
    assert!(
        kinds.contains(&("1234:7777:C:Logo", EndpointKind::Subdevice)),
        "{kinds:?}"
    );
    let fan = views
        .iter()
        .find(|v| v.uid == "comp-a")
        .expect("component endpoint");
    assert_eq!(
        (fan.kind, fan.channel.as_deref(), fan.leds),
        (EndpointKind::Component, Some("ARGB 2"), 3)
    );

    let channels = handle.channels();
    let summary: Vec<_> = channels
        .iter()
        .map(|c| (c.name.as_str(), c.led_limit, c.led_count, c.components.len()))
        .collect();
    assert_eq!(summary, vec![("ARGB 1", 120, 60, 1), ("ARGB 2", 10, 3, 1)]);
    assert_eq!(channels[1].components, vec!["comp-a".to_string()]);

    let moved = Placement {
        position: Point { x: 100.0, y: 50.0 },
        ..Placement::default()
    };
    assert!(handle.control(Control::ChildPlacement {
        uid: "comp-a".into(),
        placement: moved.clone(),
    }));
    // Controls apply before the next Render: give it a frame.
    frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(10, 20, 31)));
    wait_until(deadline, || {
        handle
            .endpoints()
            .iter()
            .any(|v| v.uid == "comp-a" && v.placement == moved)
    });
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.is_clean(), "{report:#?}");
}
