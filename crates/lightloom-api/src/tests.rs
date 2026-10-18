// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Router tests against a scripted backend: the guard (loopback Host, same
//! Origin, token for mutations), every documented route, the compat envelope,
//! the dashboard token and the preview payload.

use super::*;
use crate::model::{EffectView, Lighting};
use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode};
use http_body_util::BodyExt;
use lightloom_engine::{Frame, FrameStore, Rgb};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
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

const HOST: &str = "127.0.0.1:16138";

#[tokio::test]
async fn the_guard_wants_a_loopback_host_the_same_origin_and_a_token_for_changes() {
    let b = backend();
    let r = native_router(ctx(&b, false), NATIVE_PORT);
    let (s, _) = send(&r, "GET", "/v1/health", HOST, false, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(&r, "GET", "/v1/health", "localhost:16138", false, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = send(&r, "GET", "/v1/health", "rebind.example:16138", false, None).await;
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
        let path = format!("{COMPAT_PREFIX}{}", route.path).replace("{id}", "lightloom:plasma");
        let body = route
            .mutating
            .then(|| json!({"preset": "x", "layout": "Default", "enabled": true, "global_brightness": 5}));
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
