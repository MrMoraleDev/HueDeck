// SPDX-License-Identifier: Apache-2.0 OR MIT
//! [`CefRenderer`]: effects rendered by the `huedeck-renderer` process (CEF).
//!
//! The client starts `huedeck-renderer --serve` on an effect and publishes every
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
//!   renderer's stdin; a restarted renderer gets the latest values on its
//!   command line.

use crate::{RenderError, RendererClient};
use huedeck_engine::{Frame, FrameStore};
use huedeck_renderer::ring::RingReader;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
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

/// How to run the renderer.
#[derive(Clone, Debug)]
pub struct CefRendererConfig {
    /// The `huedeck-renderer` executable (CEF runtime files next to it).
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
}

struct Shared {
    stop: AtomicBool,
    status: Mutex<CefStatus>,
    stdin: Mutex<Option<ChildStdin>>,
    properties: Mutex<Map<String, Value>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    /// Send one control line; `false` when no renderer is listening.
    fn send(&self, line: &str) -> bool {
        let mut stdin = lock(&self.stdin);
        match stdin.as_mut() {
            Some(s) => s.write_all(line.as_bytes()).and_then(|()| s.flush()).is_ok(),
            None => false,
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

/// Effects rendered by `huedeck-renderer` (see the module docs).
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
                stdin: Mutex::new(None),
                properties: Mutex::new(properties),
            }),
            thread: None,
        }
    }

    /// What has happened so far.
    pub fn status(&self) -> CefStatus {
        lock(&self.shared.status).clone()
    }

    /// New control values: sent to the running renderer and kept for restarts.
    /// `false` when no renderer is running (the values still apply to the next).
    pub fn set_properties(&self, values: Map<String, Value>) -> bool {
        lock(&self.shared.properties).extend(values.clone());
        let line = serde_json::json!({ "cmd": "set", "values": values }).to_string() + "\n";
        self.shared.send(&line)
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
        let ring = huedeck_protocol::shm::FrameHeader::new(
            huedeck_protocol::shm::CANVAS_WIDTH,
            huedeck_protocol::shm::CANVAS_HEIGHT,
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
            .name("huedeck-cef-client".into())
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
        tracing::warn!(target: "huedeck::render", delay_s = delay.as_secs(), "effect renderer ended; restarting");
        // The last frame stays in the store: devices hold it until the restart.
        if shared.sleep(delay) {
            break;
        }
        delay = (delay * 2).min(MAX_RESTART_DELAY);
    }
}

fn spawn(cfg: &CefRendererConfig, shared: &Shared, ring: &Path) -> std::io::Result<Child> {
    let log = std::fs::File::create(cfg.work_dir.join("renderer.log"))?;
    let mut cmd = Command::new(&cfg.renderer);
    cmd.arg("--effect")
        .arg(&cfg.effect)
        .arg("--out")
        .arg(ring)
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
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
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
    let ring = cfg.work_dir.join(format!("frames-{starts}.shm"));
    let _ = std::fs::remove_file(&ring);
    let mut child = match spawn(cfg, shared, &ring) {
        Ok(c) => c,
        Err(e) => return format!("cannot start {}: {e}", cfg.renderer.display()),
    };
    *lock(&shared.stdin) = child.stdin.take();
    lock(&shared.status).running = true;

    let outcome = pump(cfg, shared, store, &mut child, &ring, held);

    // Stop the process: `quit`, then closing stdin, then kill.
    shared.send("{\"cmd\":\"quit\"}\n");
    *lock(&shared.stdin) = None;
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
    let _ = std::fs::remove_file(&ring);
    format!("{outcome}; exit {exit}")
}

/// Read frames until the renderer exits or the client stops.
fn pump(
    cfg: &CefRendererConfig,
    shared: &Shared,
    store: &FrameStore,
    child: &mut Child,
    ring: &Path,
    held: &mut Option<(Arc<Frame>, Instant)>,
) -> String {
    let since = Instant::now();
    let reader = loop {
        if shared.stop.load(Ordering::Acquire) {
            return "stopped".into();
        }
        if let Ok(Some(status)) = child.try_wait() {
            return format!("renderer exited before its frame ring was ready ({status})");
        }
        if let Ok(r) = RingReader::open_file(ring) {
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
        tracing::warn!(target: "huedeck::render", mean_ms = mean.as_millis() as u64, "effect paints below 20 fps; asking for 15 fps");
        if shared.send("{\"cmd\":\"fps\",\"value\":15}\n") {
            lock(&shared.status).downshifted = true;
        }
    }
}
