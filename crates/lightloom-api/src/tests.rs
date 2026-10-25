// SPDX-License-Identifier: GPL-3.0-or-later
//! Router tests against a scripted backend: the guard (loopback Host, same
//! Origin, token for mutations), every documented route, the compat envelope,
//! the dashboard token and the preview payload.

use super::*;
use crate::model::{EffectView, Lighting, Playlist, PlaylistEntry};
use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode};
use http_body_util::BodyExt;
use lightloom_engine::{Frame, FrameStore, Rgb};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot};
use tower::ServiceExt;

const TOKEN: &str = "test-token-0123456789";

struct Scripted {
    snapshot: Arc<Snapshot>,
    calls: Mutex<Vec<Request>>,
    events: broadcast::Sender<Event>,
    frames: Arc<FrameStore>,
}

impl Backend for Scripted {
    fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshot)
    }

    fn call(&self, request: Request) -> oneshot::Receiver<Result<Value, ApiError>> {
        let (tx, rx) = oneshot::channel();
        let answer = match &request {
            Request::ApplyEffect { id, .. } if id == "missing" => {
                Err(ApiError::not_found("no effect missing"))
            }
            Request::EffectImage { id } if id == "lightloom:plasma" => {
                Ok(json!({ "path": image_file(), "contentType": "image/png" }))
            }
            Request::EffectImage { .. } => Err(ApiError::not_found("The requested effect has no image")),
            _ => Ok(json!({ "done": request.name() })),
        };
        self.calls.lock().unwrap().push(request);
        let _ = tx.send(answer);
        rx
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    fn frames(&self) -> Arc<FrameStore> {
        Arc::clone(&self.frames)
    }
}

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, b'f', b'a', b'k', b'e'];

/// A picture for `lightloom:plasma`.
fn image_file() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("lightloom-api-image-{}.png", std::process::id()));
    if !path.exists() {
        std::fs::write(&path, PNG).unwrap();
    }
    path
}

fn backend() -> Arc<Scripted> {
    let snapshot = Snapshot {
        version: "0.0.1".into(),
        state: "Running".into(),
        lighting: Lighting {
            effect: Some("lightloom:plasma".into()),
            effect_name: Some("Plasma".into()),
            brightness: 80,
            enabled: true,
            layout: "Default".into(),
            layout_brightness: 100,
            ..Lighting::default()
        },
        effects: vec![EffectView {
            id: "lightloom:plasma".into(),
            title: "Plasma".into(),
            controls: vec![json!({"property": "speed", "label": "Speed", "type": "number", "default": "30"})],
            ..EffectView::default()
        }],
        layouts: vec!["Default".into()],
        playlists: vec![Playlist {
            id: "evening".into(),
            name: "Evening".into(),
            entries: vec![PlaylistEntry {
                effect: "lightloom:plasma".into(),
                duration: Some(60),
                ..PlaylistEntry::default()
            }],
            updated_at: "2026-10-23T10:00:00Z".into(),
            ..Playlist::default()
        }],
        ..Snapshot::default()
    };
    let frames = Arc::new(FrameStore::new(320, 200));
    frames.publish(Frame::solid(320, 200, Rgb::new(1, 2, 3)));
    Arc::new(Scripted {
        snapshot: Arc::new(snapshot),
        calls: Mutex::new(Vec::new()),
        events: broadcast::channel(16).0,
        frames,
    })
}

fn ctx(b: &Arc<Scripted>, open_compat: bool) -> ApiContext {
    ApiContext {
        backend: b.clone(),
        token: Arc::from(TOKEN),
        ui_dir: None,
        open_compat_mutations: open_compat,
        native: None,
    }
}

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    host: &str,
    token: bool,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header("host", host);
    if token {
        req = req.header("authorization", format!("Bearer {TOKEN}"));
    }
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, value)
}

const HOST: &str = "127.0.0.1:53000";

#[tokio::test]
async fn the_guard_wants_a_loopback_host_the_same_origin_and_a_token_for_changes() {
    let b = backend();
    let r = native_router(ctx(&b, false), NATIVE_PORT);
    let (s, _) = send(&r, "GET", "/v1/health", HOST, false, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(&r, "GET", "/v1/health", "localhost:53000", false, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = send(&r, "GET", "/v1/health", "rebind.example:53000", false, None).await;
    assert_eq!(
        (s, v["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden_host"))
    );
    let req = HttpRequest::builder()
        .uri("/v1/health")
        .header("host", HOST)
        .header("origin", "https://evil.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        r.clone().oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let (s, _) = send(
        &r,
        "PATCH",
        "/v1/lighting",
        HOST,
        false,
        Some(json!({"brightness": 10})),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(b.calls.lock().unwrap().is_empty());
    let (s, v) = send(
        &r,
        "PATCH",
        "/v1/lighting",
        HOST,
        true,
        Some(json!({"brightness": 10})),
    )
    .await;
    assert_eq!((s, v["done"].as_str()), (StatusCode::OK, Some("setLighting")));
    assert_eq!(
        b.calls.lock().unwrap()[0],
        Request::SetLighting {
            brightness: Some(10),
            enabled: None
        }
    );
}

#[tokio::test]
async fn every_documented_native_route_exists() {
    let b = backend();
    let r = native_router(ctx(&b, false), NATIVE_PORT);
    for route in NATIVE_ROUTES {
        let path = format!("{NATIVE_PREFIX}{}", route.path)
            .replace("{key}", "1462:7c91:A0")
            .replace("{channel}", "JRainbow%201")
            .replace("{id}", "lightloom:plasma")
            .replace("{name}", "Calm")
            .replace("{uid}", "comp-1");
        let path = if path.ends_with("/canvas/event") {
            format!("{path}?sender=game&event=kill")
        } else {
            path
        };
        let body = route.mutating.then(|| json!({}));
        let (s, v) = send(&r, route.method, &path, HOST, true, body).await;
        assert!(
            s != StatusCode::NOT_FOUND
                || v["error"]["code"] == "not_found" && !v.to_string().contains("no route"),
            "{} {path}: {s} {v}",
            route.method
        );
        assert_ne!(s, StatusCode::METHOD_NOT_ALLOWED, "{} {path}", route.method);
    }
    // An unknown API path is a 404, not the dashboard.
    let (s, v) = send(&r, "GET", "/v1/nope", HOST, false, None).await;
    assert_eq!(
        (s, v["error"]["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
}

#[tokio::test]
async fn requests_reach_the_backend_with_their_parameters() {
    let b = backend();
    let r = native_router(ctx(&b, false), NATIVE_PORT);
    let (s, _) = send(
        &r,
        "POST",
        "/v1/effects/signalrgb:Neon%20Shift/apply",
        HOST,
        true,
        Some(json!({"values": {"speed": 3}})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = send(&r, "POST", "/v1/effects/missing/apply", HOST, true, None).await;
    assert_eq!(
        (s, v["error"]["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found"))
    );
    let (s, _) = send(
        &r,
        "PUT",
        "/v1/placements/1462:7c91:A0:12v%20RGB%20Header%201",
        HOST,
        true,
        Some(json!({"position": {"x": 5, "y": 6}, "rotation": 90})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let calls = b.calls.lock().unwrap();
    assert_eq!(
        calls[0],
        Request::ApplyEffect {
            id: "signalrgb:Neon Shift".into(),
            values: json!({"speed": 3}).as_object().unwrap().clone()
        }
    );
    match &calls[2] {
        Request::SetPlacement { uid, placement } => {
            assert_eq!(uid, "1462:7c91:A0:12v RGB Header 1");
            assert_eq!((placement.position.x, placement.rotation), (5.0, 90.0));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn compat_answers_in_signalrgbs_envelope() {
    let b = backend();
    let r = compat_router(ctx(&b, false), COMPAT_REST_PORT);
    let host = "127.0.0.1:16038";
    let (s, v) = send(&r, "GET", "/api/v1/lighting", host, false, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (v["api_version"].as_str(), v["status"].as_str()),
        (Some("1.0"), Some("ok"))
    );
    assert_eq!(v["data"]["global_brightness"], 80);
    assert_eq!(v["data"]["name"], "Plasma");
    let (s, v) = send(
        &r,
        "GET",
        "/api/v1/lighting/effects/lightloom:plasma",
        host,
        false,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["parameters"]["speed"]["value"], "30");
    let (s, v) = send(&r, "GET", "/api/v1/lighting/effects/nope", host, false, None).await;
    assert_eq!((s, v["status"].as_str()), (StatusCode::NOT_FOUND, Some("error")));
    let (s, _) = send(
        &r,
        "PATCH",
        "/api/v1/lighting/global_brightness",
        host,
        false,
        Some(json!({"global_brightness": 20})),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, v) = send(&r, "GET", "/api/v1/scenes/layouts", host, false, None).await;
    assert_eq!(
        (s, v["data"]["items"][0]["id"].as_str()),
        (StatusCode::OK, Some("Default"))
    );

    let open = compat_router(ctx(&b, true), COMPAT_REST_PORT);
    let (s, _) = send(
        &open,
        "PATCH",
        "/api/v1/lighting/global_brightness",
        host,
        false,
        Some(json!({"global_brightness": 20})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    for route in COMPAT_ROUTES {
        let path = compat_path(route);
        let body = route.mutating.then(|| {
            json!({"preset": "x", "layout": "Default", "enabled": true, "global_brightness": 5,
                   "uid": "dev", "html": "<html></html>"})
        });
        let (s, _) = send(&open, route.method, &path, host, true, body).await;
        assert!(s.is_success(), "{} {path}: {s}", route.method);
    }

    let events = canvas_event_router(ctx(&b, false), COMPAT_CANVAS_EVENT_PORT, false);
    let (s, _) = send(
        &events,
        "POST",
        "/canvas/event?sender=game&event=kill",
        "127.0.0.1:16034",
        false,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &events,
        "POST",
        "/canvas/event?sender=&event=kill",
        "127.0.0.1:16034",
        false,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

/// A compat route's path with ids that exist in the scripted snapshot.
fn compat_path(route: &Route) -> String {
    let id = if route.path.contains("/playlists/") {
        "evening"
    } else {
        "lightloom:plasma"
    };
    format!("{COMPAT_PREFIX}{}", route.path).replace("{id}", id)
}

/// Like [`send`], with the raw body and its content type.
async fn send_raw(
    router: &axum::Router,
    method: &str,
    uri: &str,
    host: &str,
) -> (StatusCode, String, Vec<u8>) {
    let req = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header("host", host)
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let kind = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, kind, bytes)
}

/// SignalRGB's envelope: exactly these keys, `data` on success, `errors` on failure.
fn assert_envelope(v: &Value, method: &str, path: &str, ok: bool) {
    let obj = v
        .as_object()
        .unwrap_or_else(|| panic!("{method} {path}: not an object: {v}"));
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let payload = if ok { "data" } else { "errors" };
    let mut want = vec!["api_version", "id", "method", "params", "status", payload];
    want.sort_unstable();
    assert_eq!(keys, want, "{method} {path}: {v}");
    assert_eq!(v["api_version"], "1.0", "{method} {path}");
    assert!(v["id"].is_u64(), "{method} {path}: {v}");
    assert!(v["params"].is_object(), "{method} {path}: {v}");
    assert_eq!(
        v["status"],
        if ok { "ok" } else { "error" },
        "{method} {path}: {v}"
    );
    assert_eq!(v["method"], format!("{method} {path}"), "{method} {path}");
    if !ok {
        let e = &v["errors"][0];
        for k in ["code", "title", "detail"] {
            assert!(e[k].is_string(), "{method} {path}: errors[0].{k} in {v}");
        }
    }
}

#[tokio::test]
async fn every_compat_route_answers_in_the_envelope() {
    let b = backend();
    let r = compat_router(ctx(&b, true), COMPAT_REST_PORT);
    let host = "127.0.0.1:16038";
    let body = json!({"preset": "Calm", "layout": "Default", "enabled": true, "global_brightness": 5,
                      "uid": "dev", "html": "<html></html>", "starting_effect_id": "lightloom:plasma"});
    let mut ids = Vec::new();
    for route in COMPAT_ROUTES {
        let path = compat_path(route);
        if route.path.ends_with("/image") {
            // The picture itself on success.
            let (s, kind, bytes) = send_raw(&r, route.method, &path, host).await;
            assert_eq!(
                (s, kind.as_str(), bytes.as_slice()),
                (StatusCode::OK, "image/png", PNG)
            );
            continue;
        }
        let body = route.mutating.then(|| body.clone());
        let (s, v) = send(&r, route.method, &path, host, true, body).await;
        assert_eq!(s, StatusCode::OK, "{} {path}: {v}", route.method);
        assert_envelope(&v, route.method, &path, true);
        ids.push(v["id"].as_u64().unwrap());
    }
    // Request ids are distinct.
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n);

    // Failures carry `errors`, with the HTTP status.
    for (method, path) in [
        ("GET", "/api/v1/lighting/effects/nope"),
        ("GET", "/api/v1/lighting/effects/nope/presets"),
        ("GET", "/api/v1/lighting/effects/nope/image"),
        ("GET", "/api/v1/lighting/playlists/nope"),
        ("GET", "/api/v1/nothing/here"),
        ("POST", "/api/v1/lighting/effects/missing/apply"),
    ] {
        let (s, v) = send(&r, method, path, host, true, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{method} {path}: {v}");
        assert_envelope(&v, method, path, false);
    }

    // Answers with path parameters echo them.
    let (_, v) = send(&r, "GET", "/api/v1/lighting/playlists/evening", host, false, None).await;
    assert_eq!(v["params"]["id"], "evening");
    assert_eq!(
        (
            v["data"]["effects_count"].as_u64(),
            v["data"]["effects"][0]["name"].as_str()
        ),
        (Some(1), Some("Plasma"))
    );
    assert_eq!(
        v["data"]["links"]["play"],
        "/api/v1/lighting/playlists/evening/play"
    );
    let (_, v) = send(&r, "GET", "/api/v1/lighting/playlists", host, false, None).await;
    assert_eq!(
        (v["data"]["type"].as_str(), v["data"]["items"][0]["id"].as_str()),
        (Some("playlists"), Some("evening"))
    );
    assert_eq!(v["data"]["items"][0]["last_update_time"], "2026-10-23T10:00:00Z");
    let (_, v) = send(&r, "GET", "/api/v1/app", host, false, None).await;
    assert_eq!(
        (v["data"]["name"].as_str(), v["data"]["api_version"].as_str()),
        (Some("Lightloom"), Some("1.0"))
    );
    let (_, v) = send(&r, "GET", "/api/v1/new", host, false, None).await;
    assert_eq!(v["data"], json!({"items": [], "type": "new"}));
    let (_, v) = send(&r, "GET", "/api/v1/partners/MSI", host, false, None).await;
    assert_eq!(
        v["data"],
        json!({"id": "MSI", "permissions": {}, "type": "partner"})
    );
    let (_, v) = send(
        &r,
        "GET",
        "/api/v1/lighting/effects/lightloom:plasma",
        host,
        false,
        None,
    )
    .await;
    assert_eq!(
        (
            v["data"]["developer_effect"].as_bool(),
            v["data"]["uses_meters"].as_bool()
        ),
        (Some(false), Some(false))
    );
    assert_eq!(
        v["data"]["image"],
        "/api/v1/lighting/effects/lightloom:plasma/image"
    );
}

#[tokio::test]
async fn compat_requests_reach_the_backend() {
    let b = backend();
    let r = compat_router(ctx(&b, true), COMPAT_REST_PORT);
    let host = "127.0.0.1:16038";
    send(&r, "POST", "/api/v1/lighting/shuffle", host, false, None).await;
    let start = json!({"starting_effect_id": "lightloom:plasma"});
    let play = "/api/v1/lighting/playlists/evening/play";
    send(&r, "POST", play, host, false, Some(start)).await;
    send(&r, "POST", play, host, false, None).await;
    // loadEffect is a developer endpoint: the token even when changes are open.
    let load = json!({"uid": "dev", "html": "<title>Dev</title>"});
    let path = "/api/v1/lighting/loadEffect";
    let (s, v) = send(&r, "POST", path, host, false, Some(load.clone())).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_envelope(&v, "POST", path, false);
    let (s, v) = send(&r, "POST", path, host, true, Some(load)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["params"], json!({"uid": "dev"}));
    let calls = b.calls.lock().unwrap();
    assert_eq!(
        *calls,
        vec![
            Request::ShuffleEffect,
            Request::PlayPlaylist {
                id: "evening".into(),
                start: Some("lightloom:plasma".into())
            },
            Request::PlayPlaylist {
                id: "evening".into(),
                start: None
            },
            Request::LoadEffect {
                uid: "dev".into(),
                html: "<title>Dev</title>".into()
            },
        ]
    );
}

#[tokio::test]
async fn native_playlist_routes_reach_the_backend() {
    let b = backend();
    let r = native_router(ctx(&b, false), NATIVE_PORT);
    let (s, v) = send(&r, "GET", "/v1/playlists", HOST, false, None).await;
    assert_eq!((s, v[0]["id"].as_str()), (StatusCode::OK, Some("evening")));
    let (s, v) = send(&r, "GET", "/v1/playlists/evening", HOST, false, None).await;
    assert_eq!(
        (s, v["entries"][0]["duration"].as_u64()),
        (StatusCode::OK, Some(60))
    );
    let (s, _) = send(&r, "GET", "/v1/playlists/nope", HOST, false, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let list = json!({"name": "Mix", "shuffle": true,
                      "entries": [{"effect": "lightloom:plasma", "preset": "Calm"}]});
    let start = json!({"start": "lightloom:plasma"});
    let timer = json!({"enabled": true, "interval": 60});
    send(&r, "POST", "/v1/playlists", HOST, true, Some(list.clone())).await;
    send(&r, "PUT", "/v1/playlists/mix", HOST, true, Some(list)).await;
    send(&r, "DELETE", "/v1/playlists/mix", HOST, true, None).await;
    send(&r, "POST", "/v1/playlists/mix/play", HOST, true, Some(start)).await;
    send(&r, "POST", "/v1/playlists/stop", HOST, true, None).await;
    send(&r, "PATCH", "/v1/lighting/timer", HOST, true, Some(timer)).await;
    send(&r, "POST", "/v1/effects/shuffle", HOST, true, None).await;
    let calls = b.calls.lock().unwrap();
    let names: Vec<&str> = calls.iter().map(Request::name).collect();
    assert_eq!(
        names,
        [
            "savePlaylist",
            "savePlaylist",
            "deletePlaylist",
            "playPlaylist",
            "stopPlaylist",
            "setEffectTimer",
            "shuffleEffect"
        ]
    );
    match (&calls[0], &calls[1]) {
        (Request::SavePlaylist { id: None, playlist }, Request::SavePlaylist { id: Some(id), .. }) => {
            assert_eq!(
                (playlist.name.as_str(), playlist.shuffle, id.as_str()),
                ("Mix", true, "mix")
            );
            assert_eq!(playlist.entries[0].preset.as_deref(), Some("Calm"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        calls[3],
        Request::PlayPlaylist {
            id: "mix".into(),
            start: Some("lightloom:plasma".into())
        }
    );
    assert_eq!(
        calls[5],
        Request::SetEffectTimer {
            enabled: Some(true),
            interval: Some(60)
        }
    );
}

#[tokio::test]
async fn the_dashboard_page_carries_the_token() {
    let b = backend();
    let r = native_router(ctx(&b, false), NATIVE_PORT);
    let (s, v) = send(&r, "GET", "/", HOST, false, None).await;
    assert_eq!(s, StatusCode::OK);
    let page = v.as_str().unwrap();
    assert!(page.contains(TOKEN) && !page.contains("%LIGHTLOOM_TOKEN%"));
    // Client-side routes get the page too; missing assets do not.
    let (s, _) = send(&r, "GET", "/effects", HOST, false, None).await;
    assert_eq!(s, StatusCode::OK);
}

#[test]
fn the_native_port_is_53000_and_moves_only_within_range() {
    assert_eq!(NATIVE_PORT, 53000);
    assert_eq!(check_native_port(53000), Ok(53000));
    assert_eq!(check_native_port(1024), Ok(1024));
    assert_eq!(check_native_port(65535), Ok(65535));
    for bad in [0, 80, 1023, 65536, 70000, 16038, 16034, 6742] {
        assert!(check_native_port(bad).is_err(), "{bad}");
    }
    assert_eq!(native_url(17000), "http://127.0.0.1:17000/");
}

#[tokio::test]
async fn a_bad_port_is_refused_before_anything_changes() {
    let b = backend();
    let mut c = ctx(&b, false);
    c.native = Some(Arc::new(NativeListener::new(Duration::from_millis(10))));
    let r = native_router(c, NATIVE_PORT);
    for port in [json!(80), json!(70000), json!(16038)] {
        let (s, v) = send(
            &r,
            "PATCH",
            "/v1/settings",
            HOST,
            true,
            Some(json!({"port": port})),
        )
        .await;
        assert_eq!(
            (s, v["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_port")),
            "{port}"
        );
    }
    // Without a native listener (e.g. the compat side) the port cannot move.
    let r = native_router(ctx(&b, false), NATIVE_PORT);
    let (s, _) = send(
        &r,
        "PATCH",
        "/v1/settings",
        HOST,
        true,
        Some(json!({"port": 17000})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(b.calls.lock().unwrap().is_empty());
    // Other settings still pass through, with no port.
    let (s, _) = send(
        &r,
        "PATCH",
        "/v1/settings",
        HOST,
        true,
        Some(json!({"smbus": false})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        b.calls.lock().unwrap()[0],
        Request::SetSettings {
            signalrgb: None,
            gpu_compositing: None,
            smbus: Some(false),
            shutdown_mode: None,
            battery_low: None,
            battery_critical: None,
            battery_notifications: None,
            port: None,
            openrgb_sdk: None,
            lights_off_when_idle: None,
            network: None
        }
    );
}

/// One HTTP/1.1 request over a real socket: (status, JSON body).
async fn http(port: u16, method: &str, path: &str, body: Option<Value>) -> std::io::Result<(u16, Value)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    let text = String::from_utf8_lossy(&raw);
    let status = text.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let json = text
        .split_once("\r\n\r\n")
        .and_then(|(_, b)| serde_json::from_str(b).ok())
        .unwrap_or(Value::Null);
    Ok((status, json))
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_port_moves_and_the_old_listener_closes_after_the_grace_period() {
    let b = backend();
    let served = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&served);
    let grace = Duration::from_millis(700);
    let native = Arc::new(NativeListener::new(grace).on_serve(move |p| seen.lock().unwrap().push(p)));
    let mut c = ctx(&b, false);
    c.native = Some(Arc::clone(&native));
    let first = crate::listener::bind_loopback(0).await.unwrap();
    let old = first.local_addr().unwrap().port();
    native.serve(first, c.clone());
    assert_eq!(native.port(), old);

    let new = free_port();
    let (s, v) = http(old, "PATCH", "/v1/settings", Some(json!({"port": new})))
        .await
        .unwrap();
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["port"], new);
    assert_eq!(v["url"], format!("http://127.0.0.1:{new}/"));
    assert_eq!(native.port(), new);
    assert_eq!(*served.lock().unwrap(), vec![old, new]);
    assert_eq!(
        b.calls.lock().unwrap()[0],
        Request::SetSettings {
            signalrgb: None,
            gpu_compositing: None,
            smbus: None,
            shutdown_mode: None,
            battery_low: None,
            battery_critical: None,
            battery_notifications: None,
            port: Some(new),
            openrgb_sdk: None,
            lights_off_when_idle: None,
            network: None
        }
    );
    // Both answer during the grace period; the dashboard page comes from the new one.
    assert_eq!(http(new, "GET", "/v1/health", None).await.unwrap().0, 200);
    assert_eq!(http(old, "GET", "/v1/health", None).await.unwrap().0, 200);
    tokio::time::sleep(grace + Duration::from_millis(800)).await;
    assert!(
        http(old, "GET", "/v1/health", None).await.is_err(),
        "the old listener is closed"
    );
    assert_eq!(http(new, "GET", "/v1/health", None).await.unwrap().0, 200);

    // A busy port: 409, nothing changes, nothing is saved.
    let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = holder.local_addr().unwrap().port();
    let (s, v) = http(new, "PATCH", "/v1/settings", Some(json!({"port": busy})))
        .await
        .unwrap();
    assert_eq!(
        (s, v["error"]["code"].as_str()),
        (409, Some("port_in_use")),
        "{v}"
    );
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains(&busy.to_string()));
    assert_eq!(native.port(), new);
    assert_eq!(b.calls.lock().unwrap().len(), 1);

    // The same port again is no move (the choice is still saved).
    let (s, v) = http(new, "PATCH", "/v1/settings", Some(json!({"port": new})))
        .await
        .unwrap();
    assert_eq!((s, v["port"].as_u64()), (200, Some(u64::from(new))));
    assert_eq!(*served.lock().unwrap(), vec![old, new]);

    native.shutdown().await;
    assert!(http(new, "GET", "/v1/health", None).await.is_err());
    drop(holder);
}

#[test]
fn preview_frames_are_downscaled_bgra_with_a_size_header() {
    let store = FrameStore::new(320, 200);
    store.publish(Frame::solid(320, 200, Rgb::new(10, 20, 30)));
    let p = crate::server::preview_payload(&store, 2);
    assert_eq!(
        (u16::from_le_bytes([p[0], p[1]]), u16::from_le_bytes([p[2], p[3]])),
        (160, 100)
    );
    assert_eq!(p.len(), 4 + 160 * 100 * 4);
    assert_eq!(&p[4..8], &[30, 20, 10, 255]);
}
