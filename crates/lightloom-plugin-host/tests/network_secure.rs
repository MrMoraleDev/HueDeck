// SPDX-License-Identifier: GPL-3.0-or-later
//! `device.addFeature("dtls")` and `https://` XMLHttpRequest end to end on the
//! loopback interface: a device session reads an HTTPS endpoint whose
//! self-signed certificate carries the expected common name (as a Hue bridge's
//! carries its bridge id), then streams colours to a DTLS-PSK server.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::hid::NoHid;
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry};
use lightloom_plugin_host::device::WriteMode;
use lightloom_plugin_host::net::dtls::test_server::{self, ServerEvent, ServerOptions};
use lightloom_plugin_host::session::{self, NetworkTarget, SessionDevice, SessionHandle, SessionOptions};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn plugin() -> CatalogEntry {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/network-secure");
    let catalog = Catalog::scan(&[dir], Duration::from_secs(10));
    let found = catalog.network_plugins();
    assert_eq!(found.len(), 1, "skipped: {:?}", catalog.skipped());
    found[0].clone()
}

/// An HTTPS server with the test bridge certificate, answering `{"ok":true}`.
fn https_server() -> u16 {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tls");
    let cert = std::fs::read(dir.join("bridge.crt.der")).unwrap();
    let key = std::fs::read(dir.join("bridge.key.der")).unwrap();
    let config =
        rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(cert)],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
            )
            .unwrap();
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let _ = tcp.set_read_timeout(Some(Duration::from_secs(5)));
            let conn = rustls::ServerConnection::new(Arc::clone(&config)).unwrap();
            let mut s = rustls::StreamOwned::new(conn, tcp);
            let mut got = Vec::new();
            let mut buf = [0u8; 1024];
            while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => got.extend_from_slice(&buf[..n]),
                }
            }
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}");
            let _ = s.flush();
        }
    });
    port
}

/// A session of the fixture with `controller`, and a thread publishing frames until `done`.
fn spawn(controller: serde_json::Value, mode: WriteMode, done: &Arc<AtomicBool>) -> SessionHandle {
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let device = SessionDevice::network(NetworkTarget {
        service: "Secure Loopback".into(),
        id: "bridge".into(),
        controller,
    });
    let handle = session::spawn(
        device,
        plugin(),
        Arc::new(NoHid),
        Arc::clone(&frames),
        SessionOptions {
            write_mode: mode,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    let done = Arc::clone(done);
    std::thread::spawn(move || {
        while !done.load(Ordering::SeqCst) {
            frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(10, 20, 30)));
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    handle
}

/// Let `handle` render for `at_least` (and 5 frames), then stop it.
fn run_for(handle: &SessionHandle, at_least: Duration) {
    let start = Instant::now();
    let end = start + Duration::from_secs(60);
    while Instant::now() < end
        && !handle.is_finished()
        && (start.elapsed() < at_least || handle.live().frames < 5)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    handle.stop();
}

#[test]
fn a_session_reads_https_and_streams_over_dtls() {
    let https_port = https_server();
    let server = test_server::start(ServerOptions {
        identity: b"hue-user".to_vec(),
        key: vec![0x5a; 16],
        ..ServerOptions::default()
    });
    let done = Arc::new(AtomicBool::new(false));
    let handle = spawn(
        serde_json::json!({
            "id": "bridge", "name": "Bridge", "ip": "127.0.0.1",
            "httpsPort": https_port, "certName": "001788FFFE1A2B3C",
            "dtlsPort": server.addr.port(), "identity": "hue-user", "key": "5a".repeat(16),
        }),
        WriteMode::Live,
        &done,
    );
    let end = Instant::now() + Duration::from_secs(60);
    let mut frame = None;
    while frame.is_none() && Instant::now() < end {
        match server.events.recv_timeout(Duration::from_millis(200)) {
            Ok(ServerEvent::Data(d)) => frame = Some(d),
            Ok(ServerEvent::Failed(e)) => panic!("server: {e}"),
            _ => {}
        }
    }
    // The colour, the length of the HTTPS body and of the certificate's SHA-256 (hex).
    assert_eq!(frame.expect("a DTLS frame"), vec![0xaa, 10, 20, 30, 11, 64]);
    handle.stop();
    let report = handle.join().unwrap();
    done.store(true, Ordering::SeqCst);
    assert!(report.is_clean(), "{report:#?}");
    let net = report.network.clone().unwrap();
    assert_eq!(
        (net.dtls_connections, net.http_requests, net.http_errors),
        (1, 1, 0),
        "{net:?}"
    );
    assert!(
        report.log_tail.iter().any(|l| l.contains("dtls: established")),
        "{:?}",
        report.log_tail
    );
    // Shutdown's CloseConnection(): close_notify.
    let end = Instant::now() + Duration::from_secs(5);
    let mut closed = false;
    while !closed && Instant::now() < end {
        closed = matches!(
            server.events.recv_timeout(Duration::from_millis(200)),
            Ok(ServerEvent::Closed)
        );
    }
    assert!(closed, "close_notify arrived");
}

#[test]
fn a_wrong_key_reports_an_error_and_dry_runs_never_connect() {
    let server = test_server::start(ServerOptions {
        identity: b"hue-user".to_vec(),
        key: vec![0x5a; 16],
        ..ServerOptions::default()
    });
    let controller = serde_json::json!({
        "id": "bridge", "name": "Bridge", "ip": "127.0.0.1",
        "httpsPort": 1, "certName": "x",
        "dtlsPort": server.addr.port(), "identity": "hue-user", "key": "00".repeat(16),
    });
    let done = Arc::new(AtomicBool::new(false));
    let handle = spawn(controller.clone(), WriteMode::Live, &done);
    run_for(&handle, Duration::from_secs(3));
    let report = handle.join().unwrap();
    assert!(
        report
            .log_tail
            .iter()
            .any(|l| l.contains("dtls: error") && l.contains("bad_record_mac")),
        "{:?}",
        report.log_tail
    );
    assert_eq!(report.network.unwrap().dtls_connections, 0);

    let quiet = test_server::start(ServerOptions::default());
    let mut dry = controller;
    dry["dtlsPort"] = quiet.addr.port().into();
    let handle = spawn(dry, WriteMode::DryRun, &done);
    run_for(&handle, Duration::from_secs(1));
    let report = handle.join().unwrap();
    done.store(true, Ordering::SeqCst);
    assert!(
        report.log_tail.iter().any(|l| l.contains("dtls: error dry run")),
        "{:?}",
        report.log_tail
    );
    assert!(
        quiet.events.recv_timeout(Duration::from_millis(300)).is_err(),
        "nothing reached the server"
    );
}
