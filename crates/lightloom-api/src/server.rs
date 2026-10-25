// SPDX-License-Identifier: GPL-3.0-or-later
//! The HTTP side: routers for the native API (with the dashboard and the
//! WebSocket streams), the SignalRGB-compatible API and the canvas-event
//! listener, all behind [`guard`].
//!
//! Security model (docs/architecture.md §7, ADR-0009):
//!
//! * listeners bind to the loopback address only;
//! * every request must name a loopback `Host` (`127.0.0.1:<port>`,
//!   `localhost:<port>`, `[::1]:<port>`), which defeats DNS rebinding, and an
//!   `Origin`, when present, must be the same loopback origin, so other web
//!   pages cannot drive the API from the user's browser;
//! * requests that change state (`POST`, `PUT`, `PATCH`, `DELETE`) need
//!   `Authorization: Bearer <token>`, the token from the user's data directory.
//!   The dashboard page gets it embedded when it is served (same-origin only).

use crate::model::{ApiError, Event, PawnIoAction, Playlist, Request, Snapshot};
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use lightloom_engine::{FrameStore, Placement};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, oneshot};

/// What the routers need from the daemon.
pub trait Backend: Send + Sync + 'static {
    /// The current read model.
    fn snapshot(&self) -> Arc<Snapshot>;
    /// Hand a request to the daemon; the answer arrives on the receiver.
    fn call(&self, request: Request) -> oneshot::Receiver<Result<Value, ApiError>>;
    /// Push events.
    fn subscribe(&self) -> broadcast::Receiver<Event>;
    /// The canvas (dashboard preview).
    fn frames(&self) -> Arc<FrameStore>;
    /// Direct LED colours that win over the canvas (the OpenRGB SDK server
    /// writes them); `None`: the daemon does not support them.
    fn direct(&self) -> Option<Arc<lightloom_engine::DirectLeds>> {
        None
    }
}

/// Shared by every router.
#[derive(Clone)]
pub struct ApiContext {
    /// The daemon.
    pub backend: Arc<dyn Backend>,
    /// Bearer token for state-changing requests.
    pub token: Arc<str>,
    /// Dashboard files (`index.html`, assets); `None` serves a placeholder page.
    pub ui_dir: Option<PathBuf>,
    /// Let the SignalRGB-compatible API change state without the token.
    pub open_compat_mutations: bool,
    /// The native listener, when the port can be changed (`PATCH /v1/settings`).
    pub native: Option<Arc<crate::NativeListener>>,
}

impl ApiContext {
    async fn call(&self, request: Request) -> Result<Value, ApiError> {
        self.backend
            .call(request)
            .await
            .unwrap_or_else(|_| Err(ApiError::unavailable("the daemon is shutting down")))
    }
}

// ------------------------------------------------------------------- guard

#[derive(Clone)]
struct Guard {
    hosts: Arc<Vec<String>>,
    origins: Arc<Vec<String>>,
    token: Arc<str>,
    require_token: bool,
}

impl Guard {
    fn new(port: u16, token: Arc<str>, require_token: bool) -> Self {
        let hosts: Vec<String> = ["127.0.0.1", "localhost", "[::1]"]
            .iter()
            .map(|h| format!("{h}:{port}"))
            .collect();
        let origins = hosts.iter().map(|h| format!("http://{h}")).collect();
        Self {
            hosts: Arc::new(hosts),
            origins: Arc::new(origins),
            token,
            require_token,
        }
    }
}

/// Constant-time comparison of two byte strings.
fn same_secret(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Whether `headers` carry `Authorization: Bearer <token>`.
pub fn has_token(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|given| same_secret(given.trim().as_bytes(), token.as_bytes()))
}

fn is_mutation(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

async fn guard(State(g): State<Guard>, req: axum::extract::Request, next: Next) -> Response {
    let headers = req.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !g.hosts.iter().any(|h| h.eq_ignore_ascii_case(host)) {
        return error_response(&ApiError {
            status: 403,
            code: "forbidden_host".into(),
            message: format!("requests must address the loopback interface (Host {host:?})"),
        });
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if !g.origins.iter().any(|o| o.eq_ignore_ascii_case(origin)) {
            return error_response(&ApiError {
                status: 403,
                code: "forbidden_origin".into(),
                message: format!("cross-origin requests are refused (Origin {origin:?})"),
            });
        }
    }
    if g.require_token && is_mutation(req.method()) && !has_token(headers, &g.token) {
        return error_response(&ApiError {
            status: 401,
            code: "unauthorized".into(),
            message: "this request changes state: send Authorization: Bearer <token> (the api-token file \
                      in Lightloom's data directory)"
                .into(),
        });
    }
    next.run(req).await
}

fn error_response(e: &ApiError) -> Response {
    let status = StatusCode::from_u16(e.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(json!({ "error": e }))).into_response()
}

fn reply(result: Result<Value, ApiError>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => error_response(&e),
    }
}

// ------------------------------------------------------------------ native

/// The native API on `port` ([`crate::NATIVE_PORT`] by default): `/v1/...`, the WebSocket streams and the dashboard.
pub fn native_router(ctx: ApiContext, port: u16) -> Router {
    let g = Guard::new(port, Arc::clone(&ctx.token), true);
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/state", get(state))
        .route("/v1/devices", get(devices))
        .route("/v1/devices/{key}", get(device).patch(patch_device))
        .route("/v1/devices/{key}/channels/{channel}", put(put_channel))
        .route("/v1/save/preview", post(save_preview))
        .route("/v1/save", post(save_to_devices))
        .route("/v1/effects", get(effects))
        .route("/v1/effects/next", post(next_effect))
        .route("/v1/effects/previous", post(previous_effect))
        .route("/v1/effects/shuffle", post(shuffle_effect))
        .route("/v1/effects/{id}", get(effect))
        .route("/v1/effects/{id}/apply", post(apply_effect))
        .route("/v1/effects/{id}/values", axum::routing::patch(set_effect_values))
        .route(
            "/v1/effects/{id}/presets/{name}",
            put(save_preset).delete(delete_preset),
        )
        .route("/v1/effects/{id}/presets/{name}/apply", post(apply_preset))
        .route("/v1/lighting", get(lighting).patch(patch_lighting))
        .route("/v1/lighting/timer", axum::routing::patch(patch_timer))
        .route("/v1/playlists", get(playlists).post(create_playlist))
        .route("/v1/playlists/stop", post(stop_playlist))
        .route(
            "/v1/playlists/{id}",
            get(get_playlist).put(put_playlist).delete(delete_playlist),
        )
        .route("/v1/playlists/{id}/play", post(play_playlist))
        .route("/v1/layouts", get(layouts))
        .route(
            "/v1/layouts/{name}",
            get(get_layout).put(save_layout).delete(delete_layout),
        )
        .route("/v1/layouts/{name}/apply", post(apply_layout))
        .route("/v1/layout", axum::routing::patch(patch_layout))
        .route("/v1/placements/{uid}", put(put_placement))
        .route("/v1/components", get(components))
        .route("/v1/settings", get(settings).patch(patch_settings))
        .route(
            "/v1/import/signalrgb",
            get(preview_signalrgb).post(import_signalrgb),
        )
        .route("/v1/export", get(export))
        .route("/v1/import", post(import))
        .route("/v1/pawnio", get(pawnio))
        .route("/v1/pawnio/install", post(pawnio_install))
        .route("/v1/pawnio/remove", post(pawnio_remove))
        .route("/v1/rescan", post(rescan))
        .route("/v1/network", get(network))
        .route("/v1/network/{plugin}/add", post(network_add))
        .route(
            "/v1/network/{plugin}/controllers/{id}/{action}",
            post(network_action),
        )
        .route("/v1/shutdown", post(shutdown))
        .route("/v1/canvas/event", post(canvas_event))
        .route("/v1/canvas/tap", post(canvas_tap))
        .route("/v1/actions", post(run_action))
        .route("/v1/events", get(events_ws))
        .route("/v1/preview", get(preview_ws))
        .route("/", get(index))
        .route("/index.html", get(index))
        .fallback(static_file)
        .with_state(ctx)
        .layer(middleware::from_fn_with_state(g, guard))
}

async fn health(State(ctx): State<ApiContext>) -> Response {
    let s = ctx.backend.snapshot();
    Json(json!({ "status": "ok", "version": s.version, "state": s.state })).into_response()
}

async fn state(State(ctx): State<ApiContext>) -> Response {
    Json(&*ctx.backend.snapshot()).into_response()
}

async fn devices(State(ctx): State<ApiContext>) -> Response {
    Json(&ctx.backend.snapshot().devices).into_response()
}

async fn device(State(ctx): State<ApiContext>, Path(key): Path<String>) -> Response {
    let s = ctx.backend.snapshot();
    match s.devices.iter().find(|d| d.key.eq_ignore_ascii_case(&key)) {
        Some(d) => Json(d).into_response(),
        None => error_response(&ApiError::not_found(format!("no device {key}"))),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicePatch {
    enabled: Option<bool>,
    parameters: Option<Map<String, Value>>,
    shutdown_mode: Option<String>,
    #[serde(default)]
    blink_low_battery: Option<bool>,
}

async fn patch_device(
    State(ctx): State<ApiContext>,
    Path(key): Path<String>,
    Json(p): Json<DevicePatch>,
) -> Response {
    reply(
        ctx.call(Request::SetDevice {
            key,
            enabled: p.enabled,
            parameters: p.parameters,
            shutdown_mode: p.shutdown_mode,
            blink_low_battery: p.blink_low_battery,
        })
        .await,
    )
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SaveBody {
    #[serde(default)]
    confirm: bool,
    #[serde(default)]
    devices: std::collections::BTreeMap<String, crate::model::DeviceSaveChoice>,
}

/// What "Save to devices" would store in each device (nothing is written).
async fn save_preview(State(ctx): State<ApiContext>, body: Option<Json<SaveBody>>) -> Response {
    let b = body.map(|Json(b)| b).unwrap_or_default();
    reply(
        ctx.call(Request::SaveToDevices {
            preview: true,
            confirm: false,
            devices: b.devices,
        })
        .await,
    )
}

/// Store lighting in the devices' own memory; needs `"confirm": true`.
async fn save_to_devices(State(ctx): State<ApiContext>, Json(b): Json<SaveBody>) -> Response {
    reply(
        ctx.call(Request::SaveToDevices {
            preview: false,
            confirm: b.confirm,
            devices: b.devices,
        })
        .await,
    )
}

async fn put_channel(
    State(ctx): State<ApiContext>,
    Path((device, channel)): Path<(String, String)>,
    Json(components): Json<Vec<Value>>,
) -> Response {
    reply(
        ctx.call(Request::SetChannel {
            device,
            channel,
            components,
        })
        .await,
    )
}

async fn effects(State(ctx): State<ApiContext>) -> Response {
    Json(&ctx.backend.snapshot().effects).into_response()
}

async fn effect(State(ctx): State<ApiContext>, Path(id): Path<String>) -> Response {
    let s = ctx.backend.snapshot();
    match s.effects.iter().find(|e| e.id == id) {
        Some(e) => Json(e).into_response(),
        None => error_response(&ApiError::not_found(format!("no effect {id}"))),
    }
}

#[derive(Deserialize, Default)]
struct ApplyBody {
    #[serde(default)]
    values: Map<String, Value>,
}

async fn apply_effect(
    State(ctx): State<ApiContext>,
    Path(id): Path<String>,
    body: Option<Json<ApplyBody>>,
) -> Response {
    let values = body.map(|Json(b)| b.values).unwrap_or_default();
    reply(ctx.call(Request::ApplyEffect { id, values }).await)
}

async fn set_effect_values(
    State(ctx): State<ApiContext>,
    Path(id): Path<String>,
    Json(values): Json<Map<String, Value>>,
) -> Response {
    reply(ctx.call(Request::SetEffectValues { id, values }).await)
}

async fn save_preset(
    State(ctx): State<ApiContext>,
    Path((effect, name)): Path<(String, String)>,
) -> Response {
    reply(ctx.call(Request::SavePreset { effect, name }).await)
}

async fn apply_preset(
    State(ctx): State<ApiContext>,
    Path((effect, name)): Path<(String, String)>,
) -> Response {
    reply(ctx.call(Request::ApplyPreset { effect, name }).await)
}

async fn delete_preset(
    State(ctx): State<ApiContext>,
    Path((effect, name)): Path<(String, String)>,
) -> Response {
    reply(ctx.call(Request::DeletePreset { effect, name }).await)
}

async fn next_effect(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::StepEffect { previous: false }).await)
}

async fn previous_effect(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::StepEffect { previous: true }).await)
}

async fn shuffle_effect(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::ShuffleEffect).await)
}

async fn lighting(State(ctx): State<ApiContext>) -> Response {
    Json(&ctx.backend.snapshot().lighting).into_response()
}

#[derive(Deserialize)]
struct LightingPatch {
    brightness: Option<u8>,
    enabled: Option<bool>,
}

async fn patch_lighting(State(ctx): State<ApiContext>, Json(p): Json<LightingPatch>) -> Response {
    reply(
        ctx.call(Request::SetLighting {
            brightness: p.brightness,
            enabled: p.enabled,
        })
        .await,
    )
}

#[derive(Deserialize)]
struct TimerPatch {
    enabled: Option<bool>,
    interval: Option<u32>,
}

async fn patch_timer(State(ctx): State<ApiContext>, Json(p): Json<TimerPatch>) -> Response {
    reply(
        ctx.call(Request::SetEffectTimer {
            enabled: p.enabled,
            interval: p.interval,
        })
        .await,
    )
}

async fn playlists(State(ctx): State<ApiContext>) -> Response {
    Json(&ctx.backend.snapshot().playlists).into_response()
}

async fn get_playlist(State(ctx): State<ApiContext>, Path(id): Path<String>) -> Response {
    let s = ctx.backend.snapshot();
    match s.playlists.iter().find(|p| p.id == id) {
        Some(p) => Json(p).into_response(),
        None => error_response(&ApiError::not_found(format!("no playlist {id}"))),
    }
}

async fn create_playlist(State(ctx): State<ApiContext>, Json(playlist): Json<Playlist>) -> Response {
    reply(ctx.call(Request::SavePlaylist { id: None, playlist }).await)
}

async fn put_playlist(
    State(ctx): State<ApiContext>,
    Path(id): Path<String>,
    Json(playlist): Json<Playlist>,
) -> Response {
    reply(
        ctx.call(Request::SavePlaylist {
            id: Some(id),
            playlist,
        })
        .await,
    )
}

async fn delete_playlist(State(ctx): State<ApiContext>, Path(id): Path<String>) -> Response {
    reply(ctx.call(Request::DeletePlaylist { id }).await)
}

#[derive(Deserialize, Default)]
struct PlayBody {
    start: Option<String>,
}

async fn play_playlist(
    State(ctx): State<ApiContext>,
    Path(id): Path<String>,
    body: Option<Json<PlayBody>>,
) -> Response {
    let start = body.and_then(|Json(b)| b.start);
    reply(ctx.call(Request::PlayPlaylist { id, start }).await)
}

async fn stop_playlist(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::StopPlaylist).await)
}

async fn layouts(State(ctx): State<ApiContext>) -> Response {
    Json(&ctx.backend.snapshot().layouts).into_response()
}

async fn get_layout(State(ctx): State<ApiContext>, Path(name): Path<String>) -> Response {
    reply(ctx.call(Request::GetLayout { name }).await)
}

async fn save_layout(State(ctx): State<ApiContext>, Path(name): Path<String>) -> Response {
    reply(ctx.call(Request::SaveLayoutAs { name }).await)
}

async fn delete_layout(State(ctx): State<ApiContext>, Path(name): Path<String>) -> Response {
    reply(ctx.call(Request::DeleteLayout { name }).await)
}

async fn apply_layout(State(ctx): State<ApiContext>, Path(name): Path<String>) -> Response {
    reply(ctx.call(Request::ApplyLayout { name }).await)
}

#[derive(Deserialize)]
struct LayoutPatch {
    brightness: u8,
}

async fn patch_layout(State(ctx): State<ApiContext>, Json(p): Json<LayoutPatch>) -> Response {
    reply(
        ctx.call(Request::SetLayoutBrightness {
            brightness: p.brightness,
        })
        .await,
    )
}

async fn put_placement(
    State(ctx): State<ApiContext>,
    Path(uid): Path<String>,
    Json(placement): Json<Placement>,
) -> Response {
    reply(ctx.call(Request::SetPlacement { uid, placement }).await)
}

async fn components(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::ListComponents).await)
}

async fn settings(State(ctx): State<ApiContext>) -> Response {
    Json(&ctx.backend.snapshot().settings).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPatch {
    signalrgb: Option<bool>,
    gpu_compositing: Option<bool>,
    smbus: Option<bool>,
    network: Option<bool>,
    shutdown_mode: Option<String>,
    #[serde(default)]
    battery_low: Option<u8>,
    #[serde(default)]
    battery_critical: Option<u8>,
    #[serde(default)]
    battery_notifications: Option<bool>,
    /// Wider than `u16` so an out-of-range number gets a clear 400.
    port: Option<u32>,
    openrgb_sdk: Option<bool>,
    /// The per-user `lightloom://` URL-scheme registration (opt-in).
    #[serde(default)]
    url_scheme: Option<bool>,
}

/// `PATCH /v1/settings`. A `port` moves the native API and the dashboard:
/// the new port is bound first (busy: 409, nothing changes), the daemon
/// persists it, the new listener starts and the old one stays for
/// a grace period ([`crate::listener::DEFAULT_GRACE`]) so this response and the
/// dashboard's navigation to the returned `url` go through.
async fn patch_settings(State(ctx): State<ApiContext>, Json(p): Json<SettingsPatch>) -> Response {
    let mut moved = None;
    let port = match p.port {
        None => None,
        Some(raw) => {
            let port = match crate::check_native_port(raw) {
                Ok(port) => port,
                Err(message) => {
                    return error_response(&ApiError {
                        status: 400,
                        code: "invalid_port".into(),
                        message,
                    })
                }
            };
            let Some(native) = ctx.native.clone() else {
                return error_response(&ApiError::bad_request(
                    "the port can only be changed on the native API",
                ));
            };
            if port != native.port() {
                match native.bind(port).await {
                    Ok(listener) => moved = Some((native, listener)),
                    Err(e) => {
                        return error_response(&ApiError {
                            status: 409,
                            code: "port_in_use".into(),
                            message: format!(
                                "port {port} cannot be used ({e}); another program probably listens on it. Nothing was changed: choose another port"
                            ),
                        })
                    }
                }
            }
            Some(port)
        }
    };
    // The URL scheme first: when it cannot be registered nothing else changes.
    let mut scheme = None;
    if let Some(enabled) = p.url_scheme {
        match ctx.call(Request::SetUrlScheme { enabled }).await {
            Ok(v) => scheme = Some(v),
            Err(e) => return error_response(&e),
        }
    }
    let result = ctx
        .call(Request::SetSettings {
            signalrgb: p.signalrgb,
            gpu_compositing: p.gpu_compositing,
            smbus: p.smbus,
            network: p.network,
            shutdown_mode: p.shutdown_mode,
            battery_low: p.battery_low,
            battery_critical: p.battery_critical,
            battery_notifications: p.battery_notifications,
            port,
            openrgb_sdk: p.openrgb_sdk,
        })
        .await;
    let mut value = match result {
        Ok(v) => v,
        // The new listener (if any) is dropped: nothing changed.
        Err(e) => return error_response(&e),
    };
    let serving = match moved {
        Some((native, listener)) => {
            native.serve(listener, ctx.clone());
            Some(native.port())
        }
        None => ctx.native.as_ref().map(|n| n.port()),
    };
    if let (Some(Value::Object(extra)), Some(obj)) = (scheme, value.as_object_mut()) {
        obj.extend(extra);
    }
    if let (Some(port), Some(obj)) = (serving, value.as_object_mut()) {
        obj.insert("port".into(), json!(port));
        obj.insert("url".into(), json!(crate::native_url(port)));
    }
    Json(value).into_response()
}

#[derive(Deserialize, Default)]
struct ImportLayoutBody {
    name: Option<String>,
}

async fn preview_signalrgb(State(ctx): State<ApiContext>, Query(q): Query<ImportLayoutBody>) -> Response {
    reply(ctx.call(Request::PreviewSignalRgb { name: q.name }).await)
}

async fn import_signalrgb(State(ctx): State<ApiContext>, body: Option<Json<ImportLayoutBody>>) -> Response {
    let name = body.and_then(|Json(b)| b.name);
    reply(ctx.call(Request::ImportSignalRgb { name }).await)
}

async fn export(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::Export).await)
}

async fn import(State(ctx): State<ApiContext>, Json(state): Json<Value>) -> Response {
    reply(ctx.call(Request::Import { state }).await)
}

async fn pawnio(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::PawnIoStatus).await)
}

async fn pawnio_install(State(ctx): State<ApiContext>) -> Response {
    reply(
        ctx.call(Request::PawnIoSetup {
            action: PawnIoAction::Install,
        })
        .await,
    )
}

async fn pawnio_remove(State(ctx): State<ApiContext>) -> Response {
    reply(
        ctx.call(Request::PawnIoSetup {
            action: PawnIoAction::Remove,
        })
        .await,
    )
}

async fn network(State(ctx): State<ApiContext>) -> Response {
    Json(&ctx.backend.snapshot().network).into_response()
}

#[derive(Deserialize)]
struct NetworkAddBody {
    ip: String,
}

async fn network_add(
    State(ctx): State<ApiContext>,
    Path(plugin): Path<String>,
    Json(b): Json<NetworkAddBody>,
) -> Response {
    reply(ctx.call(Request::NetworkAdd { plugin, ip: b.ip }).await)
}

async fn network_action(
    State(ctx): State<ApiContext>,
    Path((plugin, id, action)): Path<(String, String, String)>,
) -> Response {
    reply(ctx.call(Request::NetworkAction { plugin, id, action }).await)
}

async fn rescan(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::Rescan).await)
}

async fn shutdown(State(ctx): State<ApiContext>) -> Response {
    reply(ctx.call(Request::Shutdown).await)
}

#[derive(Deserialize)]
struct CanvasEventQuery {
    sender: String,
    event: String,
}

async fn canvas_event(State(ctx): State<ApiContext>, Query(q): Query<CanvasEventQuery>) -> Response {
    reply(
        ctx.call(Request::CanvasEvent {
            sender: q.sender,
            event: q.event,
        })
        .await,
    )
}

#[derive(Deserialize)]
struct Tap {
    x: f64,
    y: f64,
}

async fn canvas_tap(State(ctx): State<ApiContext>, Json(t): Json<Tap>) -> Response {
    reply(ctx.call(Request::CanvasTap { x: t.x, y: t.y }).await)
}

#[derive(Deserialize)]
struct ActionBody {
    url: String,
}

/// `POST /v1/actions {url}`: run a `lightloom://` action URL
/// (docs/integrations/actions.md).
async fn run_action(State(ctx): State<ApiContext>, Json(b): Json<ActionBody>) -> Response {
    reply(ctx.call(Request::Action { url: b.url }).await)
}

// --------------------------------------------------------------- websockets

async fn events_ws(State(ctx): State<ApiContext>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| event_stream(socket, ctx))
}

async fn event_stream(mut socket: WebSocket, ctx: ApiContext) {
    let mut rx = ctx.backend.subscribe();
    let hello = serde_json::to_string(&Event::state()).unwrap_or_default();
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            event = rx.recv() => {
                let event = match event {
                    Ok(e) => e,
                    // Too slow: tell the client to refetch everything.
                    Err(broadcast::error::RecvError::Lagged(_)) => Event::state(),
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let Ok(text) = serde_json::to_string(&event) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct PreviewQuery {
    /// Frames per second, 1-30 (default 15).
    fps: Option<u32>,
    /// Keep every n-th pixel (1 = 320x200, 2 = 160x100; default 2).
    step: Option<u32>,
}

async fn preview_ws(
    State(ctx): State<ApiContext>,
    Query(q): Query<PreviewQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let fps = q.fps.unwrap_or(15).clamp(1, 30);
    let step = q.step.unwrap_or(2).clamp(1, 8);
    ws.on_upgrade(move |socket| preview_stream(socket, ctx, fps, step))
}

/// Downscaled BGRA of the latest canvas frame, prefixed with width and height
/// (two little-endian `u16`).
pub fn preview_payload(store: &FrameStore, step: u32) -> Vec<u8> {
    let (_, frame) = store.latest();
    let (w, h) = (frame.width().div_ceil(step), frame.height().div_ceil(step));
    let src = frame.as_bgra();
    let mut out = Vec::with_capacity(4 + (w * h * 4) as usize);
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    for y in 0..h {
        for x in 0..w {
            let o = (((y * step) * frame.width() + x * step) * 4) as usize;
            out.extend_from_slice(src.get(o..o + 4).unwrap_or(&[0, 0, 0, 255]));
        }
    }
    out
}

async fn preview_stream(mut socket: WebSocket, ctx: ApiContext, fps: u32, step: u32) {
    let store = ctx.backend.frames();
    let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / f64::from(fps)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last = u64::MAX;
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            _ = tick.tick() => {
                let seq = store.seq();
                if seq == last {
                    continue;
                }
                last = seq;
                let payload = preview_payload(&store, step);
                if socket.send(Message::Binary(payload.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- dashboard

const PLACEHOLDER: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Lightloom</title>\
<meta name=\"lightloom-token\" content=\"%LIGHTLOOM_TOKEN%\"></head><body style=\"font-family:sans-serif\">\
<h1>Lightloom</h1><p>The dashboard is not installed next to lightloomd. Build it with \
<code>npm ci &amp;&amp; npm run build</code> in <code>ui/</code>, or use the API: \
<a href=\"/v1/state\">/v1/state</a>.</p></body></html>";

async fn index(State(ctx): State<ApiContext>) -> Response {
    let page = match &ctx.ui_dir {
        Some(dir) => tokio::fs::read_to_string(dir.join("index.html"))
            .await
            .unwrap_or_else(|_| PLACEHOLDER.to_string()),
        None => PLACEHOLDER.to_string(),
    };
    let page = page.replace("%LIGHTLOOM_TOKEN%", &ctx.token);
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(
                    "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'",
                ),
            ),
        ],
        page,
    )
        .into_response()
}

fn content_type(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Dashboard assets under `ui_dir`; anything unknown that is not an API path
/// gets `index.html` (client-side routes).
async fn static_file(State(ctx): State<ApiContext>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("v1/") || path == "v1" {
        return error_response(&ApiError::not_found(format!("no route {}", uri.path())));
    }
    let Some(dir) = &ctx.ui_dir else {
        return index(State(ctx)).await;
    };
    // Only plain relative paths inside the dashboard directory.
    let safe = !path.is_empty()
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != ".." && !seg.contains(['\\', ':']));
    if safe {
        let file = dir.join(path);
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return (
                [(header::CONTENT_TYPE, HeaderValue::from_static(content_type(path)))],
                Body::from(bytes),
            )
                .into_response();
        }
    }
    if path.contains('.') {
        return error_response(&ApiError::not_found(format!("no file {}", uri.path())));
    }
    index(State(ctx)).await
}

// ------------------------------------------------------------------- compat

static COMPAT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// SignalRGB's envelope: `{api_version, id, method, params, status, data | errors}`.
fn envelope(method: &Method, path: &str, result: Result<Value, ApiError>) -> Response {
    envelope_with(method, path, json!({}), result)
}

/// [`envelope`] with the request's parameters (path parameters, the body's fields).
fn envelope_with(method: &Method, path: &str, params: Value, result: Result<Value, ApiError>) -> Response {
    let id = COMPAT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let method = format!("{method} {path}");
    match result {
        Ok(data) => Json(json!({
            "api_version": crate::COMPAT_API_VERSION,
            "id": id,
            "method": method,
            "params": params,
            "status": "ok",
            "data": data,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::from_u16(e.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({
                "api_version": crate::COMPAT_API_VERSION,
                "id": id,
                "method": method,
                "params": params,
                "status": "error",
                "errors": [{ "code": e.code, "title": e.code, "detail": e.message }],
            })),
        )
            .into_response(),
    }
}

/// The SignalRGB-compatible API on [`crate::COMPAT_REST_PORT`] (`/api/v1/...`).
pub fn compat_router(ctx: ApiContext, port: u16) -> Router {
    let g = Guard::new(port, Arc::clone(&ctx.token), !ctx.open_compat_mutations);
    Router::new()
        .route("/api/v1/app", get(compat_app))
        .route("/api/v1/new", get(compat_new))
        .route("/api/v1/partners/{id}", get(compat_partner))
        .route("/api/v1/lighting", get(compat_lighting))
        .route(
            "/api/v1/lighting/global_brightness",
            axum::routing::patch(compat_brightness),
        )
        .route("/api/v1/lighting/enabled", axum::routing::patch(compat_enabled))
        .route("/api/v1/lighting/effects", get(compat_effects))
        .route("/api/v1/lighting/effects/{id}", get(compat_effect))
        .route("/api/v1/lighting/effects/{id}/image", get(compat_effect_image))
        .route("/api/v1/lighting/effects/{id}/apply", post(compat_apply))
        .route(
            "/api/v1/lighting/effects/{id}/presets",
            get(compat_presets).patch(compat_apply_preset),
        )
        .route("/api/v1/lighting/next", get(compat_next).post(compat_next))
        .route(
            "/api/v1/lighting/previous",
            get(compat_previous).post(compat_previous),
        )
        .route(
            "/api/v1/lighting/shuffle",
            get(compat_shuffle).post(compat_shuffle),
        )
        .route("/api/v1/lighting/loadEffect", post(compat_load_effect))
        .route("/api/v1/lighting/playlists", get(compat_playlists))
        .route("/api/v1/lighting/playlists/{id}", get(compat_playlist))
        .route("/api/v1/lighting/playlists/{id}/play", post(compat_play_playlist))
        .route(
            "/api/v1/scenes/current_layout",
            get(compat_current_layout).patch(compat_set_layout),
        )
        .route("/api/v1/scenes/layouts", get(compat_layouts))
        .fallback(compat_not_found)
        .with_state(ctx)
        .layer(middleware::from_fn_with_state(g, guard))
}

async fn compat_not_found(method: Method, uri: axum::http::Uri) -> Response {
    envelope(
        &method,
        uri.path(),
        Err(ApiError::not_found(format!(
            "{} is not part of Lightloom's compatible API",
            uri.path()
        ))),
    )
}

async fn compat_app(State(ctx): State<ApiContext>, method: Method) -> Response {
    let s = ctx.backend.snapshot();
    envelope(
        &method,
        "/api/v1/app",
        Ok(json!({
            "name": "Lightloom",
            "version": env!("CARGO_PKG_VERSION"),
            "api_version": crate::COMPAT_API_VERSION,
            "compatible_with": "SignalRGB local API",
            "platform": std::env::consts::OS,
            "state": s.state,
            "links": {
                "self": "/api/v1/app",
                "lighting": "/api/v1/lighting",
                "effects": "/api/v1/lighting/effects",
                "playlists": "/api/v1/lighting/playlists",
                "layouts": "/api/v1/scenes/layouts",
            },
        })),
    )
}

/// SignalRGB's "what's new" feed: Lightloom has none.
async fn compat_new(method: Method) -> Response {
    envelope(&method, "/api/v1/new", Ok(json!({ "items": [], "type": "new" })))
}

/// SignalRGB's partner permission grants: Lightloom grants none.
async fn compat_partner(method: Method, Path(id): Path<String>) -> Response {
    envelope_with(
        &method,
        &format!("/api/v1/partners/{id}"),
        json!({ "id": id }),
        Ok(json!({ "id": id, "permissions": {}, "type": "partner" })),
    )
}

fn compat_lighting_data(s: &Snapshot) -> Value {
    json!({
        "enabled": s.lighting.enabled,
        "global_brightness": s.lighting.brightness,
        "name": s.lighting.effect_name.clone().unwrap_or_default(),
        "id": s.lighting.effect.clone().unwrap_or_default(),
        "playlist": s.lighting.playlist.as_ref().map(|p| json!({ "id": p.id, "name": p.name })),
        "links": { "self": "/api/v1/lighting" },
    })
}

async fn compat_lighting(State(ctx): State<ApiContext>, method: Method) -> Response {
    envelope(
        &method,
        "/api/v1/lighting",
        Ok(compat_lighting_data(&ctx.backend.snapshot())),
    )
}

#[derive(Deserialize)]
struct CompatBrightness {
    global_brightness: u8,
}

async fn compat_brightness(
    State(ctx): State<ApiContext>,
    method: Method,
    Json(b): Json<CompatBrightness>,
) -> Response {
    let r = ctx
        .call(Request::SetLighting {
            brightness: Some(b.global_brightness.min(100)),
            enabled: None,
        })
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope_with(
        &method,
        "/api/v1/lighting/global_brightness",
        json!({ "global_brightness": b.global_brightness }),
        r,
    )
}

#[derive(Deserialize)]
struct CompatEnabled {
    enabled: bool,
}

async fn compat_enabled(
    State(ctx): State<ApiContext>,
    method: Method,
    Json(b): Json<CompatEnabled>,
) -> Response {
    let r = ctx
        .call(Request::SetLighting {
            brightness: None,
            enabled: Some(b.enabled),
        })
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope_with(
        &method,
        "/api/v1/lighting/enabled",
        json!({ "enabled": b.enabled }),
        r,
    )
}

async fn compat_effects(State(ctx): State<ApiContext>, method: Method) -> Response {
    let s = ctx.backend.snapshot();
    let items: Vec<Value> = s
        .effects
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.title,
                "links": {
                    "apply": format!("/api/v1/lighting/effects/{}/apply", e.id),
                    "self": format!("/api/v1/lighting/effects/{}", e.id),
                },
            })
        })
        .collect();
    envelope(
        &method,
        "/api/v1/lighting/effects",
        Ok(json!({ "items": items, "type": "effects" })),
    )
}

fn effect_not_found() -> ApiError {
    ApiError::not_found("The requested effect was not found")
}

async fn compat_effect(State(ctx): State<ApiContext>, method: Method, Path(id): Path<String>) -> Response {
    let s = ctx.backend.snapshot();
    let path = format!("/api/v1/lighting/effects/{id}");
    let params = json!({ "id": id });
    let Some(e) = s.effects.iter().find(|e| e.id == id) else {
        return envelope_with(&method, &path, params, Err(effect_not_found()));
    };
    let mut parameters = Map::new();
    for c in &e.controls {
        let Some(name) = c.get("property").and_then(|p| p.as_str()) else {
            continue;
        };
        let value = e
            .values
            .get(name)
            .cloned()
            .or_else(|| c.get("default").cloned())
            .unwrap_or(Value::Null);
        parameters.insert(
            name.to_string(),
            json!({ "label": c.get("label"), "type": c.get("type"), "value": value }),
        );
    }
    let uses = |what: &str| e.uses.iter().any(|u| u == what);
    envelope_with(
        &method,
        &path,
        params,
        Ok(json!({
            "id": e.id,
            "name": e.title,
            "description": e.description,
            "publisher": e.publisher,
            "image": format!("{path}/image"),
            "developer_effect": e.source == "developer",
            "favorite": false,
            "uses_audio": uses("audio"),
            "uses_input": uses("input"),
            "uses_video": uses("video"),
            "uses_meters": uses("meters"),
            "uses_webgl": uses("webgl"),
            "parameters": parameters,
            "presets": e.presets,
            "links": { "apply": format!("{path}/apply"), "image": format!("{path}/image"), "self": path },
        })),
    )
}

/// Largest effect picture served.
const MAX_IMAGE_BYTES: u64 = 8 << 20;

/// The effect's picture itself (not an envelope); errors come in the envelope.
async fn compat_effect_image(
    State(ctx): State<ApiContext>,
    method: Method,
    Path(id): Path<String>,
) -> Response {
    let path = format!("/api/v1/lighting/effects/{id}/image");
    let params = json!({ "id": id });
    let found = match ctx.call(Request::EffectImage { id: id.clone() }).await {
        Ok(v) => v,
        Err(e) => return envelope_with(&method, &path, params, Err(e)),
    };
    let (Some(file), Some(mime)) = (
        found.get("path").and_then(Value::as_str),
        found.get("contentType").and_then(Value::as_str),
    ) else {
        return envelope_with(
            &method,
            &path,
            params,
            Err(ApiError::not_found("The requested effect has no image")),
        );
    };
    let small = tokio::fs::metadata(file)
        .await
        .is_ok_and(|m| m.len() <= MAX_IMAGE_BYTES);
    match tokio::fs::read(file).await {
        Ok(bytes) if small => {
            let mime =
                HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream"));
            ([(header::CONTENT_TYPE, mime)], Body::from(bytes)).into_response()
        }
        _ => envelope_with(
            &method,
            &path,
            params,
            Err(ApiError::not_found("The requested effect has no image")),
        ),
    }
}

async fn compat_apply(State(ctx): State<ApiContext>, method: Method, Path(id): Path<String>) -> Response {
    let r = ctx
        .call(Request::ApplyEffect {
            id: id.clone(),
            values: Map::new(),
        })
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope_with(
        &method,
        &format!("/api/v1/lighting/effects/{id}/apply"),
        json!({ "id": id }),
        r,
    )
}

async fn compat_presets(State(ctx): State<ApiContext>, method: Method, Path(id): Path<String>) -> Response {
    let s = ctx.backend.snapshot();
    let path = format!("/api/v1/lighting/effects/{id}/presets");
    let params = json!({ "id": id });
    match s.effects.iter().find(|e| e.id == id) {
        Some(e) => envelope_with(&method, &path, params, Ok(json!({ "items": e.presets }))),
        None => envelope_with(&method, &path, params, Err(effect_not_found())),
    }
}

#[derive(Deserialize)]
struct CompatPreset {
    preset: String,
}

async fn compat_apply_preset(
    State(ctx): State<ApiContext>,
    method: Method,
    Path(id): Path<String>,
    Json(p): Json<CompatPreset>,
) -> Response {
    let params = json!({ "id": id, "preset": p.preset });
    let r = ctx
        .call(Request::ApplyPreset {
            effect: id.clone(),
            name: p.preset,
        })
        .await;
    envelope_with(
        &method,
        &format!("/api/v1/lighting/effects/{id}/presets"),
        params,
        r,
    )
}

async fn compat_next(State(ctx): State<ApiContext>, method: Method) -> Response {
    let r = ctx
        .call(Request::StepEffect { previous: false })
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope(&method, "/api/v1/lighting/next", r)
}

async fn compat_previous(State(ctx): State<ApiContext>, method: Method) -> Response {
    let r = ctx
        .call(Request::StepEffect { previous: true })
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope(&method, "/api/v1/lighting/previous", r)
}

/// A random effect, game integrations and developer effects excluded.
async fn compat_shuffle(State(ctx): State<ApiContext>, method: Method) -> Response {
    let r = ctx
        .call(Request::ShuffleEffect)
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope(&method, "/api/v1/lighting/shuffle", r)
}

#[derive(Deserialize)]
struct CompatLoadEffect {
    uid: String,
    html: String,
}

/// SignalRGB's developer endpoint: always needs the token (it runs a page the
/// caller wrote), even when the compatible API's other changes are open.
async fn compat_load_effect(
    State(ctx): State<ApiContext>,
    method: Method,
    headers: HeaderMap,
    body: Option<Json<CompatLoadEffect>>,
) -> Response {
    let path = "/api/v1/lighting/loadEffect";
    if !has_token(&headers, &ctx.token) {
        return envelope(
            &method,
            path,
            Err(ApiError {
                status: 401,
                code: "unauthorized".into(),
                message: "loadEffect is a developer endpoint: send Authorization: Bearer <token> (the \
                          api-token file in Lightloom's data directory)"
                    .into(),
            }),
        );
    }
    let Some(Json(b)) = body else {
        return envelope(
            &method,
            path,
            Err(ApiError::bad_request("send {\"uid\": <name>, \"html\": <page>}")),
        );
    };
    let params = json!({ "uid": b.uid });
    let r = ctx
        .call(Request::LoadEffect {
            uid: b.uid,
            html: b.html,
        })
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope_with(&method, path, params, r)
}

fn compat_playlist_item(p: &Playlist) -> Value {
    let path = format!("/api/v1/lighting/playlists/{}", p.id);
    json!({
        "id": p.id,
        "name": p.name,
        "effects_count": p.entries.len(),
        "last_update_time": p.updated_at,
        "image_url": Value::Null,
        "shuffle": p.shuffle,
        "type": "playlist",
        "links": { "play": format!("{path}/play"), "self": path },
    })
}

async fn compat_playlists(State(ctx): State<ApiContext>, method: Method) -> Response {
    let s = ctx.backend.snapshot();
    let items: Vec<Value> = s.playlists.iter().map(compat_playlist_item).collect();
    envelope(
        &method,
        "/api/v1/lighting/playlists",
        Ok(json!({ "items": items, "type": "playlists" })),
    )
}

fn playlist_not_found() -> ApiError {
    ApiError::not_found("The requested playlist was not found")
}

async fn compat_playlist(State(ctx): State<ApiContext>, method: Method, Path(id): Path<String>) -> Response {
    let s = ctx.backend.snapshot();
    let path = format!("/api/v1/lighting/playlists/{id}");
    let params = json!({ "id": id });
    let Some(p) = s.playlists.iter().find(|p| p.id == id) else {
        return envelope_with(&method, &path, params, Err(playlist_not_found()));
    };
    let mut data = compat_playlist_item(p);
    data["playing"] = json!(s.lighting.playlist.as_ref().is_some_and(|now| now.id == p.id));
    data["effects"] = p
        .entries
        .iter()
        .map(|entry| {
            let name = s
                .effects
                .iter()
                .find(|e| e.id == entry.effect)
                .map(|e| e.title.clone());
            json!({
                "id": entry.effect,
                "name": name,
                "preset": entry.preset,
                "duration": entry.duration,
            })
        })
        .collect();
    envelope_with(&method, &path, params, Ok(data))
}

#[derive(Deserialize, Default)]
struct CompatPlay {
    starting_effect_id: Option<String>,
}

async fn compat_play_playlist(
    State(ctx): State<ApiContext>,
    method: Method,
    Path(id): Path<String>,
    body: Option<Json<CompatPlay>>,
) -> Response {
    let start = body.and_then(|Json(b)| b.starting_effect_id);
    let params = json!({ "id": id, "starting_effect_id": start });
    let r = ctx
        .call(Request::PlayPlaylist {
            id: id.clone(),
            start,
        })
        .await
        .map(|_| compat_lighting_data(&ctx.backend.snapshot()));
    envelope_with(
        &method,
        &format!("/api/v1/lighting/playlists/{id}/play"),
        params,
        r,
    )
}

fn compat_layout_data(s: &Snapshot) -> Value {
    json!({ "current_layout": { "id": s.lighting.layout, "type": "layout" } })
}

async fn compat_current_layout(State(ctx): State<ApiContext>, method: Method) -> Response {
    envelope(
        &method,
        "/api/v1/scenes/current_layout",
        Ok(compat_layout_data(&ctx.backend.snapshot())),
    )
}

#[derive(Deserialize)]
struct CompatLayout {
    layout: String,
}

async fn compat_set_layout(
    State(ctx): State<ApiContext>,
    method: Method,
    Json(l): Json<CompatLayout>,
) -> Response {
    let params = json!({ "layout": l.layout });
    let r = ctx
        .call(Request::ApplyLayout { name: l.layout })
        .await
        .map(|_| compat_layout_data(&ctx.backend.snapshot()));
    envelope_with(&method, "/api/v1/scenes/current_layout", params, r)
}

async fn compat_layouts(State(ctx): State<ApiContext>, method: Method) -> Response {
    let s = ctx.backend.snapshot();
    let items: Vec<Value> = s
        .layouts
        .iter()
        .map(|n| json!({ "id": n, "type": "layout" }))
        .collect();
    envelope(
        &method,
        "/api/v1/scenes/layouts",
        Ok(json!({ "items": items, "type": "layouts" })),
    )
}

/// SignalRGB's game-integration listener on [`crate::COMPAT_CANVAS_EVENT_PORT`]:
/// `POST /canvas/event?sender=<s>&event=<e>`. Open without the token unless
/// `require_token` (games post here; the worst an event does is make an effect react).
pub fn canvas_event_router(ctx: ApiContext, port: u16, require_token: bool) -> Router {
    let g = Guard::new(port, Arc::clone(&ctx.token), require_token);
    Router::new()
        .route("/canvas/event", post(compat_canvas_event))
        .with_state(ctx)
        .layer(middleware::from_fn_with_state(g, guard))
}

async fn compat_canvas_event(State(ctx): State<ApiContext>, Query(q): Query<CanvasEventQuery>) -> Response {
    if q.sender.is_empty() {
        return (StatusCode::BAD_REQUEST, "Invalid Sender").into_response();
    }
    if q.event.is_empty() {
        return (StatusCode::BAD_REQUEST, "Invalid Event").into_response();
    }
    reply(
        ctx.call(Request::CanvasEvent {
            sender: q.sender,
            event: q.event,
        })
        .await,
    )
}
