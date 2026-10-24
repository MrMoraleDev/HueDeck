// SPDX-License-Identifier: GPL-3.0-or-later
//! Network plugins end to end on the loopback interface: a service finds a
//! fake lamp over UDP, keeps settings, takes dashboard commands and announces
//! the lamp; a network device session streams to it and reads its HTTP API.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::hid::NoHid;
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry};
use lightloom_plugin_host::device::WriteMode;
use lightloom_plugin_host::service::{self, MemorySettings, PluginSettings, ServiceCommand, ServiceOptions};
use lightloom_plugin_host::session::{self, NetworkTarget, SessionDevice, SessionOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn plugin() -> CatalogEntry {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/network");
    let catalog = Catalog::scan(&[dir], Duration::from_secs(10));
    let found = catalog.network_plugins();
    assert_eq!(found.len(), 1, "skipped: {:?}", catalog.skipped());
    found[0].clone()
}

/// A lamp answering `scan` with `lamp:<id>` and recording what else arrives.
fn lamp() -> (UdpSocket, u16) {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let port = s.local_addr().unwrap().port();
    (s, port)
}

/// An HTTP server answering every request with `body` (one connection at a time).
fn http_server(body: &'static str) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while !f() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn service_finds_pairs_and_announces_then_a_session_streams() {
    let (lamp, lamp_port) = lamp();
    let http_port = http_server("{\"name\":\"lamp\"}");
    let settings = Arc::new(MemorySettings::default());
    settings.set(
        "Loopback Lights",
        "test",
        "lampPort",
        Some(&lamp_port.to_string()),
    );
    settings.set(
        "Loopback Lights",
        "test",
        "httpPort",
        Some(&http_port.to_string()),
    );
    let entry = plugin();
    let svc = service::spawn(
        entry.clone(),
        ServiceOptions {
            settings: settings.clone(),
            ..ServiceOptions::default()
        },
    )
    .unwrap();

    // The service scans every second; the lamp answers.
    let mut buf = [0u8; 64];
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < end, "no scan arrived");
        if let Ok((n, from)) = lamp.recv_from(&mut buf) {
            assert_eq!(&buf[..n], b"scan");
            lamp.send_to(b"lamp:one", from).unwrap();
            break;
        }
    }
    wait_for("the controller", || {
        svc.state().controllers.iter().any(|c| c.id == "one")
    });
    let st = svc.state();
    let one = st.controllers.iter().find(|c| c.id == "one").unwrap();
    assert!(one.can_pair);
    assert_eq!(one.data["ip"], "127.0.0.1");
    assert_eq!(one.data["status"], "found");
    assert!(one.data.get("startLink").is_none(), "functions are not data");
    assert!(st.announced.is_empty());
    assert!(st.can_forget);

    // Dashboard: pair (announces), add by IP, forget.
    assert!(svc.command(ServiceCommand::Pair("one".into())));
    wait_for("the announcement", || svc.state().announced.contains_key("one"));
    assert_eq!(
        settings.get("Loopback Lights", "one", "paired").as_deref(),
        Some("yes")
    );
    assert!(svc.command(ServiceCommand::Add("192.168.7.9".into())));
    wait_for("the manual controller", || {
        svc.state()
            .controllers
            .iter()
            .any(|c| c.id == "manual-192.168.7.9")
    });
    assert!(svc.command(ServiceCommand::Forget("manual-192.168.7.9".into())));
    wait_for("forget", || {
        !svc.state()
            .controllers
            .iter()
            .any(|c| c.id == "manual-192.168.7.9")
    });
    wait_for("Update() ticks", || {
        settings.get("Loopback Lights", "test", "ticks").is_some()
    });

    // The announced data becomes the device context's `controller`.
    let data = svc.state().announced["one"].clone();
    assert_eq!(data["paired"], true);
    let device = SessionDevice::network(NetworkTarget {
        service: "Loopback Lights".into(),
        id: "one".into(),
        controller: data,
    });
    assert_eq!(device.key, "net:loopback-lights:one");
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(10, 20, 30)));
    let handle = session::spawn(
        device,
        entry,
        Arc::new(NoHid),
        Arc::clone(&frames),
        SessionOptions::default(),
    )
    .unwrap();
    let mut got = None;
    let end = Instant::now() + Duration::from_secs(15);
    while got.is_none() && Instant::now() < end {
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(10, 20, 30)));
        if let Ok((n, _)) = lamp.recv_from(&mut buf) {
            if buf[0] == 0xaa {
                got = Some(buf[..n].to_vec());
            }
        }
    }
    // Colours of both LEDs, then the length of the HTTP body read in Initialize.
    assert_eq!(got.expect("a frame"), vec![0xaa, 10, 20, 30, 10, 20, 30, 15]);
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.is_clean(), "{report:#?}");
    assert!(report.shutdown_called);
    let net = report.network.expect("network counters");
    assert!(net.sent >= 2 && net.http_requests == 1, "{net:?}");
    // Shutdown's packet.
    let end = Instant::now() + Duration::from_secs(2);
    let mut last = 0;
    while Instant::now() < end {
        if let Ok((_, _)) = lamp.recv_from(&mut buf) {
            last = buf[0];
            if last == 0xbb {
                break;
            }
        }
    }
    assert_eq!(last, 0xbb);

    svc.shutdown(Duration::from_secs(5));
}

#[test]
fn dry_run_sessions_send_nothing() {
    let (lamp, lamp_port) = lamp();
    let device = SessionDevice::network(NetworkTarget {
        service: "Loopback Lights".into(),
        id: "dry".into(),
        controller: serde_json::json!({ "id": "dry", "name": "Dry", "ip": "127.0.0.1", "port": lamp_port, "httpPort": 1 }),
    });
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let handle = session::spawn(
        device,
        plugin(),
        Arc::new(NoHid),
        Arc::clone(&frames),
        SessionOptions {
            write_mode: WriteMode::DryRun,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    wait_for("frames", || {
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(1, 2, 3)));
        handle.live().frames >= 3 || handle.is_finished()
    });
    handle.stop();
    let report = handle.join().unwrap();
    assert!(report.network.as_ref().unwrap().dry_run >= 3, "{report:#?}");
    assert!(
        lamp.recv_from(&mut [0u8; 16]).is_err(),
        "nothing reached the lamp"
    );
}

#[test]
fn an_internet_address_is_refused() {
    let device = SessionDevice::network(NetworkTarget {
        service: "Loopback Lights".into(),
        id: "far".into(),
        controller: serde_json::json!({ "id": "far", "name": "Far", "ip": "8.8.8.8", "port": 53, "httpPort": 80 }),
    });
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let handle = session::spawn(
        device,
        plugin(),
        Arc::new(NoHid),
        Arc::clone(&frames),
        SessionOptions::default(),
    )
    .unwrap();
    wait_for("frames", || {
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(1, 2, 3)));
        handle.live().frames >= 2 || handle.is_finished()
    });
    handle.stop();
    let report = handle.join().unwrap();
    let net = report.network.clone().unwrap();
    assert_eq!(net.sent, 0, "{report:#?}");
    assert!(net.refused >= 2 && net.http_errors == 1, "{net:?}");
    assert!(
        report
            .log_tail
            .iter()
            .any(|l| l.contains("not on the local network")),
        "{:?}",
        report.log_tail
    );
}
