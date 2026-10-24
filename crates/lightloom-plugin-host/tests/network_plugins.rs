// SPDX-License-Identifier: GPL-3.0-or-later
//! The first-party network plugins in Lightloom's real host (QuickJS, the real
//! service and session natives), on the loopback interface only: each
//! service starts without discovery sockets and takes *Add by IP*
//! 127.0.0.1, each device runs a few frames in dry run against a controller
//! on 127.0.0.1. Nothing may throw or hit an API gap. Protocol details are
//! checked by the plugins' own node tests.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::hid::NoHid;
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry};
use lightloom_plugin_host::device::WriteMode;
use lightloom_plugin_host::service::{self, ServiceCommand, ServiceOptions};
use lightloom_plugin_host::session::{self, NetworkTarget, SessionDevice, SessionOptions};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn plugins() -> Vec<CatalogEntry> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let catalog = Catalog::scan(&[dir], Duration::from_secs(10));
    catalog.network_plugins().into_iter().cloned().collect()
}

/// Controller data like each plugin's service announces it, on 127.0.0.1.
fn controller(plugin: &str) -> Value {
    let mut c = json!({ "id": "test", "name": "Test light", "ip": "127.0.0.1", "status": "ready" });
    let extra = match plugin {
        "WLED" => json!({ "ledCount": 3, "udpPort": 21324, "mac": "aabbccddeeff" }),
        "DDP" => json!({ "port": 4048 }),
        "E1.31 (sACN)" => json!({ "port": 5568, "cid": "0f1e2d3c-4b5a-4697-8877-665544332211" }),
        "Philips Hue" => json!({
            "username": "user", "paired": true,
            "lights": [{ "id": "1", "name": "Lamp", "gamut": null }],
        }),
        "Nanoleaf" => json!({
            "token": "token", "paired": true, "port": 16021, "extControl": "v2", "model": "NL29",
            "panels": [{ "panelId": 11, "x": 0, "y": 0 }, { "panelId": 12, "x": 100, "y": 0 }],
        }),
        "Govee" => json!({ "sku": "H6159", "port": 4003 }),
        "LIFX" => json!({ "mac": "d073d5000001", "port": 56700 }),
        "Yeelight" => {
            json!({ "port": 55443, "model": "color", "support": ["set_rgb", "set_bright", "set_power", "set_music", "get_prop"] })
        }
        _ => json!({}),
    };
    if let (Some(c), Some(e)) = (c.as_object_mut(), extra.as_object()) {
        c.extend(e.clone());
    }
    c
}

fn bad_line(l: &str) -> bool {
    ["API_GAP", "TypeError", "ReferenceError", "SyntaxError", "RangeError", "host panic", "not implemented!"]
        .iter()
        .any(|p| l.contains(p))
        // A plugin without Removal/forceDiscover is fine for the hooks it does not need.
        && !l.starts_with("Removal() not implemented")
}

#[test]
fn every_first_party_network_plugin_is_in_the_catalog() {
    let names: Vec<String> = plugins().iter().map(|p| p.meta.name.clone()).collect();
    for n in [
        "WLED",
        "DDP",
        "E1.31 (sACN)",
        "Philips Hue",
        "Nanoleaf",
        "Govee",
        "LIFX",
        "Yeelight",
    ] {
        assert!(names.iter().any(|x| x == n), "{n} missing from {names:?}");
    }
}

#[test]
fn services_start_and_take_add_by_ip_in_the_real_host() {
    let services: Vec<_> = plugins()
        .into_iter()
        .map(|p| {
            service::spawn(
                p,
                ServiceOptions {
                    discovery: false,
                    ..ServiceOptions::default()
                },
            )
            .unwrap()
        })
        .collect();
    for s in &services {
        assert!(s.command(ServiceCommand::Add("127.0.0.1".into())));
    }
    // Long enough for a few Update() ticks and the refused loopback requests.
    std::thread::sleep(Duration::from_secs(5));
    for s in services {
        let st = s.state();
        let bad: Vec<&String> = st.log_tail.iter().filter(|l| bad_line(l)).collect();
        assert!(st.running && st.error.is_none(), "{}: {st:#?}", s.name());
        assert!(bad.is_empty(), "{}: {bad:?}", s.name());
        s.shutdown(Duration::from_secs(5));
    }
}

#[test]
fn devices_run_in_the_real_host() {
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let handles: Vec<_> = plugins()
        .into_iter()
        .map(|p| {
            let device = SessionDevice::network(NetworkTarget {
                service: p.meta.name.clone(),
                id: "test".into(),
                controller: controller(&p.meta.name),
            });
            session::spawn(
                device,
                p,
                Arc::new(NoHid),
                Arc::clone(&frames),
                SessionOptions {
                    write_mode: WriteMode::DryRun,
                    ..SessionOptions::default()
                },
            )
            .unwrap()
        })
        .collect();
    let end = Instant::now() + Duration::from_secs(40);
    let mut shade = 0u8;
    while Instant::now() < end && handles.iter().any(|h| h.live().frames < 10 && !h.is_finished()) {
        shade = shade.wrapping_add(17);
        frames.publish(Frame::solid(
            CANVAS_WIDTH,
            CANVAS_HEIGHT,
            Rgb::new(shade, 255 - shade, 40),
        ));
        std::thread::sleep(Duration::from_millis(20));
    }
    for h in handles {
        h.stop();
        let r = h.join().unwrap();
        let bad: Vec<&String> = r.log_tail.iter().filter(|l| bad_line(l)).collect();
        assert!(r.failures.is_empty() && r.faulted.is_none(), "{}: {r:#?}", r.name);
        assert!(
            r.initialized && r.frames >= 10 && r.shutdown_called,
            "{}: {r:#?}",
            r.name
        );
        assert!(bad.is_empty(), "{}: {bad:?}", r.name);
        assert_eq!(
            r.network.as_ref().unwrap().sent,
            0,
            "{}: dry run sends nothing",
            r.name
        );
    }
}
