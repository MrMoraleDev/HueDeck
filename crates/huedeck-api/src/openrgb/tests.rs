// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenRGB SDK server tests: byte goldens assembled from the documented
//! layouts, version negotiation, and a TCP session against a fake daemon
//! (count → data → LED updates reach the direct colours → disconnect returns
//! the endpoints to the canvas), plus device-list updates and the setting.

use super::wire::{self, pid, Header};
use super::*;
use crate::model::{ApiError, DeviceView, Endpoint, Event, Request, Settings, Snapshot};
use huedeck_engine::{FrameStore, Rgb};
use serde_json::Value;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, oneshot};

struct Fake {
    snapshot: Mutex<Arc<Snapshot>>,
    direct: Arc<DirectLeds>,
    frames: Arc<FrameStore>,
    events: broadcast::Sender<Event>,
}

impl Backend for Fake {
    fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshot.lock().unwrap())
    }
    fn call(&self, _: Request) -> oneshot::Receiver<Result<Value, ApiError>> {
        oneshot::channel().1
    }
    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
    fn frames(&self) -> Arc<FrameStore> {
        Arc::clone(&self.frames)
    }
    fn direct(&self) -> Option<Arc<DirectLeds>> {
        Some(Arc::clone(&self.direct))
    }
}

impl Fake {
    fn set(&self, s: Snapshot) {
        *self.snapshot.lock().unwrap() = Arc::new(s);
    }
}

fn endpoint(uid: &str, kind: &str, name: &str, channel: Option<&str>, leds: usize) -> Endpoint {
    Endpoint {
        uid: uid.into(),
        kind: kind.into(),
        name: name.into(),
        channel: channel.map(String::from),
        size: [leds as u32, 1],
        leds,
        ..Endpoint::default()
    }
}

/// A board (2 body LEDs, a 3-LED strip on "ARGB 1") and a stopped mouse.
fn snapshot() -> Snapshot {
    Snapshot {
        version: "0.9.0".into(),
        devices: vec![
            DeviceView {
                key: "1462:7c91:".into(),
                name: "Mystic Light".into(),
                plugin: "MSI Mystic Light".into(),
                device_type: "motherboard".into(),
                state: "running".into(),
                endpoints: vec![
                    endpoint("1462:7c91:", "device", "Mystic Light", None, 2),
                    endpoint("comp-1", "component", "Strip", Some("ARGB 1"), 3),
                    endpoint("1462:7c91:S:empty", "subdevice", "Empty", None, 0),
                ],
                ..DeviceView::default()
            },
            DeviceView {
                key: "1:2:".into(),
                name: "Mouse".into(),
                state: "stopped".into(),
                endpoints: vec![endpoint("1:2:", "device", "Mouse", None, 4)],
                ..DeviceView::default()
            },
        ],
        settings: Settings {
            openrgb_sdk: true,
            ..Settings::default()
        },
        ..Snapshot::default()
    }
}

fn fake() -> Arc<Fake> {
    Arc::new(Fake {
        snapshot: Mutex::new(Arc::new(snapshot())),
        direct: Arc::new(DirectLeds::new()),
        frames: Arc::new(FrameStore::new(320, 200)),
        events: broadcast::channel(4).0,
    })
}

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.split_whitespace().collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

// ------------------------------------------------------------------ goldens

#[test]
fn header_vectors() {
    // REQUEST_PROTOCOL_VERSION from a v5 client.
    let b = hex("4F524742 00000000 28000000 04000000 05000000");
    let h = Header::decode(b[..16].try_into().unwrap()).unwrap();
    assert_eq!(
        h,
        Header {
            dev: 0,
            id: pid::REQUEST_PROTOCOL_VERSION,
            len: 4
        }
    );
    // Controller count reply, 3 controllers.
    assert_eq!(
        wire::packet(0, pid::REQUEST_CONTROLLER_COUNT, &wire::u32_payload(3)),
        hex("4F524742 00000000 00000000 04000000 03000000")
    );
    // UPDATELEDS to device 0: red, green.
    let b = hex("4F524742 00000000 1A040000 0E000000 0E000000 0200 FF000000 00FF0000");
    let h = Header::decode(b[..16].try_into().unwrap()).unwrap();
    assert_eq!((h.id, h.len), (pid::UPDATELEDS, 14));
    assert_eq!(
        wire::parse_update_leds(&b[16..]).unwrap(),
        vec![[255, 0, 0], [0, 255, 0]]
    );
    // Bad magic and oversized payloads are refused.
    let mut bad = b.clone();
    bad[0] = b'X';
    assert_eq!(
        Header::decode(bad[..16].try_into().unwrap()),
        Err(wire::HeaderError::Magic)
    );
    let big = wire::packet(0, 1050, &[]);
    let mut big: [u8; 16] = big[..16].try_into().unwrap();
    big[12..16].copy_from_slice(&(wire::MAX_PAYLOAD + 1).to_le_bytes());
    assert!(matches!(
        Header::decode(&big),
        Err(wire::HeaderError::TooLarge(_))
    ));
}

#[test]
fn request_payload_layouts() {
    // UPDATEZONELEDS: data_size, zone 1, one blue LED.
    let p = hex("0E000000 01000000 0100 0000FF00");
    assert_eq!(wire::parse_update_zone_leds(&p).unwrap(), (1, vec![[0, 0, 255]]));
    // UPDATESINGLELED: LED 4, white.
    let p = hex("04000000 FFFFFF00");
    assert_eq!(wire::parse_update_single_led(&p).unwrap(), (4, [255, 255, 255]));
    // RESIZEZONE: zone 0 to 30.
    assert_eq!(
        wire::parse_resize_zone(&hex("00000000 1E000000")).unwrap(),
        (0, 30)
    );
    // UPDATEMODE: data_size, mode index 0, Mode (ignored).
    assert_eq!(wire::parse_mode_index(&hex("08000000 00000000")).unwrap(), 0);
    // Truncated payloads are malformed, not panics.
    assert!(wire::parse_update_leds(&hex("0E000000 0300 FF000000")).is_err());
    assert_eq!(wire::parse_c_string(b"Artemis\0junk"), "Artemis");
    // Empty profile/plugin list: data_size 6 (counts itself), n = 0.
    assert_eq!(wire::empty_list(), hex("06000000 0000"));
}

fn tiny() -> Controller {
    Controller {
        ty: 17,
        name: "A".into(),
        vendor: "E".into(),
        description: "D".into(),
        version: "V".into(),
        serial: "S".into(),
        location: "L".into(),
        modes: vec![Mode::direct()],
        active_mode: 0,
        zones: vec![Zone {
            name: "Z".into(),
            ty: 0,
            leds_min: 1,
            leds_max: 1,
            leds_count: 1,
        }],
        leds: vec!["L".into()],
        colors: vec![[0, 0, 0]],
    }
}

/// Device Data at protocol 0: no vendor, no mode brightness, no segments.
/// One field per line; `#` starts a comment.
const TINY_V0: &str = "
    79000000            # data_size 121
    11000000            # type 17
    0200 4100           # name
    0200 4400           # description
    0200 5600           # version
    0200 5300           # serial
    0200 4C00           # location
    0100 00000000       # 1 mode, active 0
    0700 44697265637400 # Direct
    00000000 20000000   # value, flags per-LED
    00000000 00000000   # speed min/max
    00000000 00000000   # colors min/max
    00000000 00000000   # speed, direction
    01000000 0000       # colour mode per-LED, no colours
    0100                # 1 zone
    0200 5A00 00000000  # name, type single
    01000000 01000000 01000000  # min, max, count
    0000                # no matrix
    0100 0200 4C00 00000000   # 1 LED: name, led_value
    0100 00000000       # 1 colour";

/// Device Data at protocol 4: vendor, mode brightness, zone segments.
const TINY_V4: &str = "
    8B000000            # data_size 139
    11000000
    0200 4100           # name
    0200 4500           # vendor
    0200 4400 0200 5600 0200 5300 0200 4C00
    0100 00000000
    0700 44697265637400
    00000000 20000000
    00000000 00000000   # speed min/max
    00000000 00000000   # brightness min/max
    00000000 00000000   # colors min/max
    00000000 00000000   # speed, brightness
    00000000 01000000 0000  # direction, colour mode, colours
    0100
    0200 5A00 00000000
    01000000 01000000 01000000
    0000 0000           # no matrix, no segments
    0100 0200 4C00 00000000
    0100 00000000";

fn golden(s: &str) -> Vec<u8> {
    let hexed: String = s.lines().map(|l| l.split('#').next().unwrap_or("")).collect();
    hex(&hexed)
}

#[test]
fn controller_data_goldens() {
    let c = tiny();
    assert_eq!(c.encode(0), golden(TINY_V0));
    assert_eq!(c.encode(1).len(), 121 + 4, "v1 adds the vendor");
    assert_eq!(c.encode(3).len(), 121 + 4 + 12, "v3 adds mode brightness");
    assert_eq!(c.encode(4), golden(TINY_V4));
    for v in 0..=4 {
        let b = c.encode(v);
        assert_eq!(
            u32::from_le_bytes(b[..4].try_into().unwrap()) as usize,
            b.len(),
            "data_size counts itself (v{v})"
        );
    }
}

#[test]
fn snapshot_maps_to_controllers() {
    let list = controllers(&snapshot());
    assert_eq!(
        list.len(),
        1,
        "stopped devices and LED-less endpoints are left out"
    );
    let e = &list[0];
    assert_eq!(
        e.zones,
        vec![("1462:7c91:".to_string(), 2), ("comp-1".to_string(), 3)]
    );
    let c = &e.controller;
    assert_eq!(c.ty, 0, "motherboard");
    assert_eq!(c.name, "Mystic Light");
    assert_eq!(c.location, "HueDeck: 1462:7c91:");
    assert_eq!(c.modes[0].name, "Direct");
    let zones: Vec<(&str, u32)> = c.zones.iter().map(|z| (z.name.as_str(), z.leds_count)).collect();
    assert_eq!(zones, vec![("Main", 2), ("ARGB 1: Strip", 3)]);
    assert_eq!(c.leds.len(), 5);
    assert_eq!(c.leds[2], "ARGB 1: Strip LED 1");
    assert_eq!(c.colors.len(), 5);
    assert_eq!(wire::device_type("dongle"), 17);
}

// ------------------------------------------------------------------ negotiation

fn request(conn: &mut Connection, dev: u32, id: u32, payload: &[u8]) -> Option<(Header, Vec<u8>)> {
    let h = Header {
        dev,
        id,
        len: payload.len() as u32,
    };
    conn.handle(h, payload).map(|b| {
        let h = Header::decode(b[..16].try_into().unwrap()).unwrap();
        assert_eq!(h.len as usize, b.len() - 16);
        (h, b[16..].to_vec())
    })
}

#[test]
fn version_negotiation() {
    let b = fake();
    let expected = controllers(&snapshot())[0].controller.clone();
    for (client, used) in [
        (None, 0),
        (Some(1), 1),
        (Some(3), 3),
        (Some(4), 4),
        (Some(5), 4),
        (Some(6), 4),
    ] {
        let mut conn = Connection::new(b.clone(), Arc::clone(&b.direct));
        if let Some(v) = client {
            let (h, p) = request(&mut conn, 0, pid::REQUEST_PROTOCOL_VERSION, &wire::u32_payload(v)).unwrap();
            assert_eq!(h.id, pid::REQUEST_PROTOCOL_VERSION);
            assert_eq!(p, wire::u32_payload(4), "the server answers its maximum");
            assert_eq!(conn.proto, v.min(4));
        }
        let (_, p) = request(&mut conn, 0, pid::REQUEST_CONTROLLER_COUNT, &[]).unwrap();
        assert_eq!(p, wire::u32_payload(1));
        // v0 clients send no version with REQUEST_CONTROLLER_DATA.
        let asked = client.map(wire::u32_payload).unwrap_or_default();
        let (h, p) = request(&mut conn, 0, pid::REQUEST_CONTROLLER_DATA, &asked).unwrap();
        assert_eq!(h.dev, 0);
        assert_eq!(p, expected.encode(used), "client {client:?}");
        assert!(request(&mut conn, 7, pid::REQUEST_CONTROLLER_DATA, &asked).is_none());
    }
}

#[test]
fn unsupported_requests() {
    let b = fake();
    let mut conn = Connection::new(b.clone(), Arc::clone(&b.direct));
    let (h, p) = request(&mut conn, 0, pid::REQUEST_PROFILE_LIST, &[]).unwrap();
    assert_eq!((h.id, p), (pid::REQUEST_PROFILE_LIST, wire::empty_list()));
    let (_, p) = request(&mut conn, 0, pid::REQUEST_PLUGIN_LIST, &[]).unwrap();
    assert_eq!(p, wire::empty_list());
    for id in [
        pid::REQUEST_SAVE_PROFILE,
        pid::REQUEST_LOAD_PROFILE,
        pid::REQUEST_DELETE_PROFILE,
        pid::REQUEST_RESCAN_DEVICES,
        pid::PLUGIN_SPECIFIC,
        pid::SETCUSTOMMODE,
        9999,
    ] {
        assert!(request(&mut conn, 0, id, b"x\0").is_none(), "packet {id}");
    }
    assert!(request(&mut conn, 0, pid::UPDATEMODE, &hex("08000000 00000000")).is_none());
    assert!(request(&mut conn, 0, pid::RESIZEZONE, &hex("01000000 1E000000")).is_none());
    assert!(b.direct.is_empty(), "nothing was written");
}

#[test]
fn device_list_updates_make_indexes_stale() {
    let b = fake();
    let mut conn = Connection::new(b.clone(), Arc::clone(&b.direct));
    request(&mut conn, 0, pid::REQUEST_PROTOCOL_VERSION, &wire::u32_payload(4));
    request(&mut conn, 0, pid::REQUEST_CONTROLLER_COUNT, &[]);
    assert!(conn.on_tick().is_none(), "nothing changed");
    // The mouse starts.
    let mut s = snapshot();
    s.devices[1].state = "running".into();
    b.set(s);
    let pushed = conn.on_tick().unwrap();
    assert_eq!(pushed, wire::packet(0, pid::DEVICE_LIST_UPDATED, &[]));
    assert!(conn.on_tick().is_none(), "sent once");
    // Writes with stale indexes are ignored until the client counts again.
    let leds = hex("0A000000 0100 FF000000");
    request(&mut conn, 0, pid::UPDATELEDS, &leds);
    assert!(b.direct.is_empty());
    let (_, p) = request(&mut conn, 0, pid::REQUEST_CONTROLLER_COUNT, &[]).unwrap();
    assert_eq!(p, wire::u32_payload(2));
    request(&mut conn, 1, pid::UPDATELEDS, &leds);
    assert_eq!(
        &*b.direct.get("1:2:").unwrap(),
        &[Rgb::new(255, 0, 0), Rgb::BLACK, Rgb::BLACK, Rgb::BLACK]
    );

    // A version-0 client is never told.
    let mut old = Connection::new(b.clone(), Arc::clone(&b.direct));
    b.set(snapshot());
    assert!(old.on_tick().is_none());
}

// ------------------------------------------------------------------ TCP

async fn send(s: &mut TcpStream, dev: u32, id: u32, payload: &[u8]) {
    s.write_all(&wire::packet(dev, id, payload)).await.unwrap();
}

async fn recv(s: &mut TcpStream) -> (Header, Vec<u8>) {
    let mut h = [0u8; 16];
    tokio::time::timeout(Duration::from_secs(5), s.read_exact(&mut h))
        .await
        .expect("a reply")
        .unwrap();
    let h = Header::decode(&h).unwrap();
    let mut p = vec![0u8; h.len as usize];
    s.read_exact(&mut p).await.unwrap();
    (h, p)
}

async fn eventually(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..200 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn colors(payload: &[[u8; 3]]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    for c in payload {
        p.extend_from_slice(&[c[0], c[1], c[2], 0]);
    }
    p
}

fn sized(body: Vec<u8>) -> Vec<u8> {
    let mut p = ((body.len() + 4) as u32).to_le_bytes().to_vec();
    p.extend(body);
    p
}

#[tokio::test]
async fn tcp_session_drives_and_releases_the_endpoints() {
    let b = fake();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop_tx, stop) = watch::channel(false);
    let server = tokio::spawn(serve(listener, b.clone(), Arc::clone(&b.direct), stop));

    let mut c = TcpStream::connect(addr).await.unwrap();
    send(&mut c, 0, pid::REQUEST_PROTOCOL_VERSION, &wire::u32_payload(4)).await;
    assert_eq!(recv(&mut c).await.1, wire::u32_payload(4));
    send(&mut c, 0, pid::SET_CLIENT_NAME, b"test client\0").await;
    send(&mut c, 0, pid::REQUEST_CONTROLLER_COUNT, &[]).await;
    assert_eq!(recv(&mut c).await.1, wire::u32_payload(1));
    send(&mut c, 0, pid::REQUEST_CONTROLLER_DATA, &wire::u32_payload(4)).await;
    let (h, data) = recv(&mut c).await;
    assert_eq!((h.dev, h.id), (0, pid::REQUEST_CONTROLLER_DATA));
    assert_eq!(data, controllers(&snapshot())[0].controller.encode(4));
    send(&mut c, 0, pid::SETCUSTOMMODE, &[]).await;

    // All five LEDs: two on the body, three on the strip.
    let (r, g, bl) = ([255, 0, 0], [0, 255, 0], [0, 0, 255]);
    send(&mut c, 0, pid::UPDATELEDS, &sized(colors(&[r, r, g, g, g]))).await;
    eventually("the body and the strip", || {
        b.direct.get("1462:7c91:").as_deref() == Some(&[Rgb::new(255, 0, 0); 2][..])
            && b.direct.get("comp-1").as_deref() == Some(&[Rgb::new(0, 255, 0); 3][..])
    })
    .await;
    // Zone 1 (the strip), then LED 4 (its last LED).
    let mut zone = 1u32.to_le_bytes().to_vec();
    zone.extend(colors(&[bl]));
    send(&mut c, 0, pid::UPDATEZONELEDS, &sized(zone)).await;
    let mut single = 4i32.to_le_bytes().to_vec();
    single.extend_from_slice(&[9, 9, 9, 0]);
    send(&mut c, 0, pid::UPDATESINGLELED, &single).await;
    eventually("zone and single LED updates", || {
        b.direct.get("comp-1").as_deref()
            == Some(&[Rgb::new(0, 0, 255), Rgb::new(0, 255, 0), Rgb::new(9, 9, 9)][..])
    })
    .await;
    assert_eq!(
        b.direct.uids(),
        vec!["1462:7c91:".to_string(), "comp-1".to_string()]
    );

    // The client leaves: back to the canvas.
    drop(c);
    eventually("the endpoints released", || b.direct.is_empty()).await;

    // A bad header drops the connection (and nothing is written).
    let mut c = TcpStream::connect(addr).await.unwrap();
    c.write_all(b"XXXX000000000000").await.unwrap();
    let mut buf = [0u8; 1];
    let n = tokio::time::timeout(Duration::from_secs(5), c.read(&mut buf))
        .await
        .unwrap()
        .unwrap_or(0);
    assert_eq!(n, 0, "closed");

    // Stopping the server ends every connection and releases its colours.
    let mut c = TcpStream::connect(addr).await.unwrap();
    send(&mut c, 0, pid::REQUEST_CONTROLLER_COUNT, &[]).await;
    recv(&mut c).await;
    send(&mut c, 0, pid::UPDATELEDS, &sized(colors(&[r]))).await;
    eventually("written", || !b.direct.is_empty()).await;
    stop_tx.send(true).unwrap();
    server.await.unwrap();
    assert!(b.direct.is_empty());
}

async fn connect_soon(port: u16) -> Option<TcpStream> {
    for _ in 0..100 {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", port)).await {
            return Some(s);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

#[tokio::test]
async fn the_setting_starts_and_stops_the_server() {
    let b = fake();
    let mut off = snapshot();
    off.settings.openrgb_sdk = false;
    b.set(off.clone());
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let (stop_tx, stop) = watch::channel(false);
    let sup = tokio::spawn(serve_when_enabled(b.clone(), port, stop, Arc::new(|_| false)));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        TcpStream::connect(("127.0.0.1", port)).await.is_err(),
        "off by default"
    );
    b.set(snapshot());
    let mut c = connect_soon(port).await.expect("the server started");
    send(&mut c, 0, pid::REQUEST_CONTROLLER_COUNT, &[]).await;
    assert_eq!(recv(&mut c).await.1, wire::u32_payload(1));
    b.set(off);
    let mut buf = [0u8; 1];
    let n = tokio::time::timeout(Duration::from_secs(5), c.read(&mut buf))
        .await
        .expect("closed when turned off")
        .unwrap_or(0);
    assert_eq!(n, 0);
    stop_tx.send(true).unwrap();
    sup.await.unwrap();
}

#[tokio::test]
async fn a_busy_port_is_left_alone() {
    let b = fake();
    // Another program reported on the port: never bound.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let (stop_tx, stop) = watch::channel(false);
    let sup = tokio::spawn(serve_when_enabled(b.clone(), port, stop, Arc::new(|_| true)));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    stop_tx.send(true).unwrap();
    sup.await.unwrap();
}
