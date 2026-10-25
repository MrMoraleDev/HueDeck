// SPDX-License-Identifier: GPL-3.0-or-later
//! [`CefRenderer`]: effects rendered by the `lightloom-renderer` process (CEF).
//!
//! The client starts `lightloom-renderer --serve` on an effect and publishes every
//! new frame of the shared-memory ring the renderer writes into the
//! [`FrameStore`] as soon as it appears (the ring sequence is polled every
//! 2 ms). It also:
//!
//! * republishes the last frame when the page paints nothing for two frame
//!   periods: Chromium paints only what changes, and plugins must keep
//!   rendering (keep-alive packets, slow devices);
//! * restarts a renderer that exits or never produces a ring, holding the last
//!   frame meanwhile, after a growing delay (1 s doubling to 30 s, back to 1 s
//!   after a renderer ran for a minute);
//! * asks the renderer to paint at 15 fps once it delivers fewer than 20 fps
//!   over 60 frames at a target of 30 or more (VMs, RDP, a busy GPU);
//! * forwards control values ([`CefRenderer::set_properties`]) over the
//!   renderer's control channel; a restarted renderer gets the latest values
//!   on its command line;
//! * forwards input data for the page's `window.engine`
//!   ([`CefRenderer::push_engine_data`]: audio, screen zones, meters, sensors).
//!
//! Transport per platform (`lightloom_renderer::ipc`; the ring layout and the
//! JSON control lines are the same everywhere):
//!
//! * Windows: the ring is `frames-<n>.shm` in the work dir, the control
//!   channel is the renderer's stdin.
//! * Unix (Linux): every run gets a private 0700 directory on a tmpfs
//!   (`$XDG_RUNTIME_DIR`, else `/dev/shm`, else the work dir) holding the
//!   ring and a unix socket the client listens on; the renderer gets
//!   `--control unix:PATH` and connects. Lines sent before it connects are
//!   buffered (up to [`PENDING_MAX`] bytes) and flushed on accept. If the
//!   socket cannot be bound (path too long for `sun_path`), stdin is used.
//!   The directory is removed when the run ends.

use crate::{RenderError, RendererClient};
use lightloom_engine::{Frame, FrameStore};
use lightloom_renderer::ipc::{ControlAddr, PrivateDir};
use lightloom_renderer::ring::RingReader;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often the ring sequence is polled.
const POLL: Duration = Duration::from_millis(2);
/// How long a new renderer gets to create its ring.
const RING_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest wait before restarting a renderer.
const MAX_RESTART_DELAY: Duration = Duration::from_secs(30);
/// A renderer that ran this long resets the restart delay.
const HEALTHY_AFTER: Duration = Duration::from_secs(60);
/// Frames the downshift guard averages over.
const DOWNSHIFT_WINDOW: usize = 60;
/// Control bytes kept for a renderer that has not connected yet; beyond it `send` fails.
pub const PENDING_MAX: usize = 4 * 1024 * 1024;
/// A socket write blocked this long (renderer not reading) fails instead of stalling the caller.
#[cfg(unix)]
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// How to run the renderer.
#[derive(Clone, Debug)]
pub struct CefRendererConfig {
    /// The `lightloom-renderer` executable (CEF runtime files next to it).
    pub renderer: PathBuf,
    /// The effect HTML file.
    pub effect: PathBuf,
    /// Paint rate, frames per second.
    pub fps: u32,
    /// Control values overriding the effect's defaults.
    pub properties: Map<String, Value>,
    /// `--software-compositing` (VM / RDP fallback).
    pub software_compositing: bool,
    /// Where the ring, the CEF profile and the renderer's logs go.
    pub work_dir: PathBuf,
}

/// What the client has seen so far.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CefStatus {
    /// Renderer processes started.
    pub starts: u32,
    /// Frames painted by the renderer and published.
    pub frames: u64,
    /// Held frames republished to keep plugins rendering.
    pub republished: u64,
    /// The renderer was asked to paint at 15 fps.
    pub downshifted: bool,
    /// A renderer is running.
    pub running: bool,
    /// Why the last renderer ended (exit status or start error).
    pub last_exit: Option<String>,
    /// Longest gap between the last frame of a renderer and the first frame of
    /// the one that replaced it, in milliseconds (devices held the last frame).
    pub max_recovery_ms: Option<f64>,
    /// The current (or last) renderer's frame ring file.
    pub ring: Option<String>,
    /// Its control channel: `stdin` or `unix:PATH`.
    pub control: Option<String>,
}

/// Where control lines go.
enum Link {
    /// No renderer is running.
    Closed,
    /// A renderer was started but has not connected to its socket yet: lines wait here.
    Pending(Vec<u8>),
    /// The renderer's stdin, or the accepted unix socket.
    Open(Box<dyn Write + Send>),
}

struct Shared {
    stop: AtomicBool,
    status: Mutex<CefStatus>,
    control: Mutex<Link>,
    properties: Mutex<Map<String, Value>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    /// Send one control line; `false` when no renderer is listening (or one that is still
    /// connecting has [`PENDING_MAX`] bytes waiting already).
    fn send(&self, line: &str) -> bool {
        match &mut *lock(&self.control) {
            Link::Closed => false,
            Link::Pending(buf) if buf.len() + line.len() > PENDING_MAX => false,
            Link::Pending(buf) => {
                buf.extend_from_slice(line.as_bytes());
                true
            }
            Link::Open(w) => w.write_all(line.as_bytes()).and_then(|()| w.flush()).is_ok(),
        }
    }

    /// The renderer connected: flush what waited, then write straight through.
    #[cfg_attr(not(unix), allow(dead_code))]
    fn connect(&self, mut w: Box<dyn Write + Send>) {
        let mut link = lock(&self.control);
        let pending = match std::mem::replace(&mut *link, Link::Closed) {
            Link::Pending(buf) => buf,
            // The run ended meanwhile: leave the link closed.
            Link::Closed => return,
            Link::Open(_) => Vec::new(),
        };
        if w.write_all(&pending).and_then(|()| w.flush()).is_ok() {
            *link = Link::Open(w);
        }
    }

    /// Sleep up to `d`, returning early (`true`) on stop.
    fn sleep(&self, d: Duration) -> bool {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if self.stop.load(Ordering::Acquire) {
                return true;
            }
            thread::sleep(Duration::from_millis(20).min(end.saturating_duration_since(Instant::now())));
        }
        self.stop.load(Ordering::Acquire)
    }
}

/// Effects rendered by `lightloom-renderer` (see the module docs).
pub struct CefRenderer {
    cfg: CefRendererConfig,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for CefRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CefRenderer")
            .field("effect", &self.cfg.effect)
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl CefRenderer {
    /// A client for `cfg` (nothing starts before [`RendererClient::start`]).
    pub fn new(cfg: CefRendererConfig) -> Self {
        let properties = cfg.properties.clone();
        Self {
            cfg,
            shared: Arc::new(Shared {
                stop: AtomicBool::new(false),
                status: Mutex::new(CefStatus::default()),
                control: Mutex::new(Link::Closed),
                properties: Mutex::new(properties),
            }),
            thread: None,
        }
    }

    /// What has happened so far.
    pub fn status(&self) -> CefStatus {
        lock(&self.shared.status).clone()
    }

    /// Input data for the page's `window.engine` (`audio`, `zone`, `vision`,
    /// `sensors`); `false` when no renderer is running.
    pub fn push_engine_data(&self, data: Map<String, Value>) -> bool {
        let line = serde_json::json!({ "cmd": "engine", "data": data }).to_string() + "\n";
        self.shared.send(&line)
    }

    /// A handle pushing `window.engine` data from another thread (an input
    /// source running at its own rate); it follows renderer restarts.
    pub fn engine_sink(&self) -> EngineDataSink {
        EngineDataSink(Arc::clone(&self.shared))
    }

    /// New control values: sent to the running renderer and kept for restarts.
    /// `false` when no renderer is running (the values still apply to the next).
    pub fn set_properties(&self, values: Map<String, Value>) -> bool {
        lock(&self.shared.properties).extend(values.clone());
        let line = serde_json::json!({ "cmd": "set", "values": values }).to_string() + "\n";
        self.shared.send(&line)
    }

    /// `onCanvasApiEvent({sender, event})` in the page; `false` when no renderer is running.
    pub fn send_event(&self, sender: &str, event: &str) -> bool {
        let line = serde_json::json!({ "cmd": "event", "sender": sender, "event": event }).to_string() + "\n";
        self.shared.send(&line)
    }

    /// `onCanvasTapped(x, y)` in the page; `false` when no renderer is running.
    pub fn tap(&self, x: f64, y: f64) -> bool {
        let line = serde_json::json!({ "cmd": "tap", "x": x, "y": y }).to_string() + "\n";
        self.shared.send(&line)
    }
}

/// Pushes `window.engine` data to a [`CefRenderer`] (see [`CefRenderer::engine_sink`]).
#[derive(Clone)]
pub struct EngineDataSink(Arc<Shared>);

impl std::fmt::Debug for EngineDataSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EngineDataSink")
    }
}

impl EngineDataSink {
    /// Same as [`CefRenderer::push_engine_data`]; `false` when no renderer is running.
    pub fn push(&self, data: Map<String, Value>) -> bool {
        let line = serde_json::json!({ "cmd": "engine", "data": data }).to_string()
            + "
";
        self.0.send(&line)
    }
}

impl RendererClient for CefRenderer {
    fn name(&self) -> &str {
        "cef"
    }

    fn start(&mut self, store: Arc<FrameStore>) -> Result<(), RenderError> {
        if self.thread.is_some() {
            return Err(RenderError::AlreadyRunning);
        }
        let ring = lightloom_protocol::shm::FrameHeader::new(
            lightloom_protocol::shm::CANVAS_WIDTH,
            lightloom_protocol::shm::CANVAS_HEIGHT,
        );
        if (store.width(), store.height()) != (ring.width, ring.height) {
            return Err(RenderError::SizeMismatch {
                renderer: (ring.width, ring.height),
                store: (store.width(), store.height()),
            });
        }
        std::fs::create_dir_all(&self.cfg.work_dir)?;
        self.shared.stop.store(false, Ordering::Release);
        let (cfg, shared) = (self.cfg.clone(), Arc::clone(&self.shared));
        let handle = thread::Builder::new()
            .name("lightloom-cef-client".into())
            .spawn(move || supervise(&cfg, &shared, &store))?;
        self.thread = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }

    fn is_running(&self) -> bool {
        lock(&self.shared.status).running
    }

    fn status_line(&self) -> Option<String> {
        let st = self.status();
        Some(format!(
            "renderer: {} start{}, {} frames painted, {} held frames republished{}{}{}",
            st.starts,
            if st.starts == 1 { "" } else { "s" },
            st.frames,
            st.republished,
            if st.downshifted {
                ", downshifted to 15 fps"
            } else {
                ""
            },
            st.max_recovery_ms
                .map(|ms| format!(", longest recovery {ms:.0} ms"))
                .unwrap_or_default(),
            st.last_exit
                .map(|e| format!("; last renderer: {e}"))
                .unwrap_or_default()
        ))
    }
}

impl Drop for CefRenderer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start, watch and restart renderers until stopped.
fn supervise(cfg: &CefRendererConfig, shared: &Shared, store: &FrameStore) {
    let mut delay = Duration::from_secs(1);
    let mut held: Option<(Arc<Frame>, Instant)> = None;
    while !shared.stop.load(Ordering::Acquire) {
        let started = Instant::now();
        let outcome = run_once(cfg, shared, store, &mut held);
        {
            let mut st = lock(&shared.status);
            st.running = false;
            st.last_exit = Some(outcome);
        }
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        if started.elapsed() >= HEALTHY_AFTER {
            delay = Duration::from_secs(1);
        }
        tracing::warn!(target: "lightloom::render", delay_s = delay.as_secs(), "effect renderer ended; restarting");
        // The last frame stays in the store: devices hold it until the restart.
        if shared.sleep(delay) {
            break;
        }
        delay = (delay * 2).min(MAX_RESTART_DELAY);
    }
}

/// One run's transport (see the module docs).
struct RunChannel {
    ring: PathBuf,
    control: ControlAddr,
    /// Unix: the socket the renderer connects to, until it has.
    #[cfg(unix)]
    listener: Option<lightloom_renderer::ipc::ControlListener>,
    /// Unix: the run's private tmpfs directory (removed on drop, after the listener).
    dir: Option<PrivateDir>,
}

impl RunChannel {
    #[cfg(unix)]
    fn new(cfg: &CefRendererConfig, _starts: u32) -> std::io::Result<Self> {
        use lightloom_renderer::ipc;
        let dir = PrivateDir::create(&ipc::shm_base_dir(&cfg.work_dir), "lightloom-render")?;
        let listener = match ipc::ControlListener::bind(&dir.join(ipc::CONTROL_SOCKET)) {
            Ok(l) => Some(l),
            Err(e) => {
                tracing::warn!(target: "lightloom::render", dir = %dir.path().display(), error = %e,
                    "no control socket; using the renderer's stdin");
                None
            }
        };
        Ok(Self {
            ring: dir.join(ipc::RING_FILE),
            control: listener
                .as_ref()
                .map_or(ControlAddr::Stdin, ipc::ControlListener::addr),
            listener,
            dir: Some(dir),
        })
    }

    #[cfg(not(unix))]
    fn new(cfg: &CefRendererConfig, starts: u32) -> std::io::Result<Self> {
        let ring = cfg.work_dir.join(format!("frames-{starts}.shm"));
        let _ = std::fs::remove_file(&ring);
        Ok(Self {
            ring,
            control: ControlAddr::Stdin,
            dir: None,
        })
    }

    /// Accept the renderer's connection once it is there (no-op with stdin).
    fn poll_accept(&mut self, shared: &Shared) {
        #[cfg(unix)]
        if let Some(listener) = &self.listener {
            match listener.try_accept() {
                Ok(None) => {}
                Ok(Some(stream)) => {
                    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                    shared.connect(Box::new(stream));
                    self.listener = None;
                }
                Err(e) => {
                    tracing::warn!(target: "lightloom::render", error = %e, "control socket accept failed");
                    self.listener = None;
                }
            }
        }
        #[cfg(not(unix))]
        let _ = shared;
    }
}

impl Drop for RunChannel {
    fn drop(&mut self) {
        // Windows: the ring is a plain file in the work dir. Unix: the private dir goes with it.
        if self.dir.is_none() {
            let _ = std::fs::remove_file(&self.ring);
        }
    }
}

fn spawn(cfg: &CefRendererConfig, shared: &Shared, channel: &RunChannel) -> std::io::Result<Child> {
    let log = std::fs::File::create(cfg.work_dir.join("renderer.log"))?;
    let mut cmd = Command::new(&cfg.renderer);
    cmd.arg("--effect")
        .arg(&cfg.effect)
        .arg("--out")
        .arg(&channel.ring)
        .arg("--serve")
        .arg("--fps")
        .arg(cfg.fps.clamp(1, 240).to_string())
        .arg("--cef-cache")
        .arg(cfg.work_dir.join("cef-cache"))
        .arg("--cef-log")
        .arg(cfg.work_dir.join("cef.log"))
        .arg("--stats")
        .arg(cfg.work_dir.join("renderer-report.json"));
    if cfg.software_compositing {
        cmd.arg("--software-compositing");
    }
    for (name, value) in lock(&shared.properties).iter() {
        let v = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        cmd.arg("--prop").arg(format!("{name}={v}"));
    }
    if channel.control == ControlAddr::Stdin {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.arg("--control")
            .arg(channel.control.to_string())
            .stdin(Stdio::null());
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::from(log));
    cmd.spawn()
}

/// One renderer process, from start to exit (or stop). Returns why it ended.
fn run_once(
    cfg: &CefRendererConfig,
    shared: &Shared,
    store: &FrameStore,
    held: &mut Option<(Arc<Frame>, Instant)>,
) -> String {
    let starts = {
        let mut st = lock(&shared.status);
        st.starts += 1;
        st.starts
    };
    let mut channel = match RunChannel::new(cfg, starts) {
        Ok(c) => c,
        Err(e) => return format!("cannot create the frame ring directory: {e}"),
    };
    {
        let mut st = lock(&shared.status);
        st.ring = Some(channel.ring.display().to_string());
        st.control = Some(channel.control.to_string());
    }
    let mut child = match spawn(cfg, shared, &channel) {
        Ok(c) => c,
        Err(e) => return format!("cannot start {}: {e}", cfg.renderer.display()),
    };
    *lock(&shared.control) = match child.stdin.take() {
        Some(stdin) => Link::Open(Box::new(stdin)),
        None => Link::Pending(Vec::new()),
    };
    lock(&shared.status).running = true;

    let outcome = pump(cfg, shared, store, &mut child, &mut channel, held);

    // Stop the process: `quit`, then closing the control channel, then kill.
    shared.send("{\"cmd\":\"quit\"}\n");
    *lock(&shared.control) = Link::Closed;
    let deadline = Instant::now() + Duration::from_secs(5);
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.to_string(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                break child
                    .wait()
                    .map(|s| format!("killed ({s})"))
                    .unwrap_or_else(|e| format!("killed ({e})"));
            }
        }
    };
    drop(channel);
    format!("{outcome}; exit {exit}")
}

/// Read frames until the renderer exits or the client stops.
fn pump(
    cfg: &CefRendererConfig,
    shared: &Shared,
    store: &FrameStore,
    child: &mut Child,
    channel: &mut RunChannel,
    held: &mut Option<(Arc<Frame>, Instant)>,
) -> String {
    let since = Instant::now();
    let reader = loop {
        channel.poll_accept(shared);
        if shared.stop.load(Ordering::Acquire) {
            return "stopped".into();
        }
        if let Ok(Some(status)) = child.try_wait() {
            return format!("renderer exited before its frame ring was ready ({status})");
        }
        if let Ok(r) = RingReader::open_file(&channel.ring) {
            break r;
        }
        if since.elapsed() > RING_TIMEOUT {
            return "the renderer created no frame ring".into();
        }
        thread::sleep(Duration::from_millis(20));
    };
    let geometry = reader.geometry();
    let (width, height) = (geometry.width, geometry.height);
    let mut buf = vec![0u8; geometry.slot_bytes()];
    let period = Duration::from_secs_f64(1.0 / f64::from(cfg.fps.clamp(1, 240)));
    let mut last_seq = 0u64;
    let mut last_publish = Instant::now();
    let mut last_paint: Option<Instant> = None;
    let mut intervals: VecDeque<Duration> = VecDeque::with_capacity(DOWNSHIFT_WINDOW);
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return "stopped".into();
        }
        if let Ok(Some(status)) = child.try_wait() {
            return format!("renderer exited ({status})");
        }
        channel.poll_accept(shared);
        match reader.read_latest(&mut buf) {
            Ok(Some(seq)) if seq != last_seq => {
                last_seq = seq;
                let now = Instant::now();
                if let Some(frame) = Frame::from_bgra(width, height, buf.clone()) {
                    let frame = Arc::new(frame);
                    store.publish_arc_at(Arc::clone(&frame), now);
                    let mut st = lock(&shared.status);
                    st.frames += 1;
                    // First frame of a replacement renderer: how long devices held the old one.
                    if last_paint.is_none() {
                        if let Some((_, previous)) = held.as_ref() {
                            let gap = (now - *previous).as_secs_f64() * 1000.0;
                            st.max_recovery_ms = Some(st.max_recovery_ms.map_or(gap, |m| m.max(gap)));
                        }
                    }
                    drop(st);
                    *held = Some((frame, now));
                    last_publish = now;
                }
                if let Some(prev) = last_paint.replace(now) {
                    if intervals.len() == DOWNSHIFT_WINDOW {
                        intervals.pop_front();
                    }
                    intervals.push_back(now - prev);
                    downshift_if_slow(cfg, shared, &intervals);
                }
            }
            _ => {
                // Nothing new painted: keep plugins rendering with the held frame.
                if last_publish.elapsed() >= period * 2 {
                    if let Some((frame, at)) = held.as_ref() {
                        store.publish_arc_at(Arc::clone(frame), *at);
                        lock(&shared.status).republished += 1;
                    }
                    last_publish = Instant::now();
                }
                thread::sleep(POLL);
            }
        }
    }
}

/// Paint at 15 fps once a 30+ fps target is missed badly (< 20 fps over a full window).
fn downshift_if_slow(cfg: &CefRendererConfig, shared: &Shared, intervals: &VecDeque<Duration>) {
    if cfg.fps < 30 || intervals.len() < DOWNSHIFT_WINDOW || lock(&shared.status).downshifted {
        return;
    }
    let mean = intervals.iter().sum::<Duration>() / intervals.len() as u32;
    if mean > Duration::from_millis(50) {
        tracing::warn!(target: "lightloom::render", mean_ms = mean.as_millis() as u64, "effect paints below 20 fps; asking for 15 fps");
        if shared.send("{\"cmd\":\"fps\",\"value\":15}\n") {
            lock(&shared.status).downshifted = true;
        }
    }
}
