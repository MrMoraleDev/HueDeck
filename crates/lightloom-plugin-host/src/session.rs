// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Device sessions: one plugin driving one physical device.
//!
//! [`spawn`] starts a dedicated plugin thread with its own QuickJS runtime
//! (synthesis D-rules: one runtime per plugin thread) that:
//!
//! 1. loads the plugin with the device host ([`RuntimeOptions::device`]);
//! 2. binds `device.productId()` and the `ControllableParameters` defaults,
//!    then the caller's parameter overrides;
//! 3. calls `Validate(endpoint)` for every collection of the device and opens
//!    the first one it accepts (the active endpoint until `set_endpoint`);
//! 4. optionally reads a feature report before `Initialize` ([`RestoreProbe`]);
//! 5. calls `Initialize()`, then `Render()` once per new canvas frame of the
//!    [`FrameStore`]: a slow plugin skips straight to the latest frame. Each
//!    `Render()` runs under a watchdog budget; consecutive timeouts fault the
//!    session;
//! 6. on stop calls `Shutdown(false)` (unless told not to), repeats the
//!    feature-report read and returns a [`SessionReport`].

use crate::catalog::CatalogEntry;
use crate::device::{lock_device, Brightness, DeviceContext, IoRecord, IoStats, LogSink, WriteMode};
use crate::runtime::{CallArgs, JsFailure, PluginRuntime, RuntimeOptions, PLUGIN_THREAD_STACK};
use lightloom_engine::{FrameStore, Placement, SamplerConfig};
use lightloom_hal::hid::{HidBackend, HidEndpointInfo, HidEndpointTuple};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// One physical device: every HID collection that belongs to it.
#[derive(Clone, Debug)]
pub struct SessionDevice {
    /// SignalRGB-shaped key (`vid:pid:serial`).
    pub key: String,
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// Collections in enumeration order.
    pub endpoints: Vec<HidEndpointInfo>,
}

/// Time budgets of the lifecycle calls.
#[derive(Clone, Copy, Debug)]
pub struct Budgets {
    /// Parse + evaluate the module.
    pub load: Duration,
    /// Each `Validate(endpoint)` call, and parameter binding.
    pub validate: Duration,
    /// `Initialize()`.
    pub initialize: Duration,
    /// `Render()` watchdog.
    pub render: Duration,
    /// `Shutdown()`.
    pub shutdown: Duration,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            load: Duration::from_secs(10),
            validate: Duration::from_secs(1),
            initialize: Duration::from_secs(10),
            render: Duration::from_millis(250),
            shutdown: Duration::from_secs(5),
        }
    }
}

/// A feature report read before `Initialize` and after `Shutdown`, to show
/// that the device ended in the state it started in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreProbe {
    /// Report id.
    pub report_id: u8,
    /// Length including the report id byte.
    pub len: usize,
}

/// How a session runs.
#[derive(Clone)]
pub struct SessionOptions {
    /// Lifecycle budgets.
    pub budgets: Budgets,
    /// Parameter overrides applied after the `ControllableParameters` defaults.
    pub parameters: serde_json::Map<String, serde_json::Value>,
    /// Canvas placement; `None` = the plugin's `DefaultPosition()` / `DefaultScale()`.
    pub placement: Option<Placement>,
    /// Global and layout brightness.
    pub brightness: Brightness,
    /// Live or dry-run writes.
    pub write_mode: WriteMode,
    /// Leave without calling `Shutdown(false)` (the device keeps the last frame).
    pub skip_shutdown: bool,
    /// Consecutive `Render()` timeouts that fault the session.
    pub max_render_timeouts: u32,
    /// Optional before/after feature-report comparison.
    pub restore_probe: Option<RestoreProbe>,
    /// Receives every plugin log line.
    pub log_sink: Option<LogSink>,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            budgets: Budgets::default(),
            parameters: serde_json::Map::new(),
            placement: None,
            brightness: Brightness::default(),
            write_mode: WriteMode::Live,
            skip_shutdown: false,
            max_render_timeouts: 3,
            restore_probe: None,
            log_sink: None,
        }
    }
}

impl std::fmt::Debug for SessionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionOptions")
            .field("budgets", &self.budgets)
            .field("parameters", &self.parameters)
            .field("write_mode", &self.write_mode)
            .field("skip_shutdown", &self.skip_shutdown)
            .field("restore_probe", &self.restore_probe)
            .finish_non_exhaustive()
    }
}

/// Percentiles of a latency sample, in milliseconds (nearest rank).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct LatencySummary {
    /// Samples.
    pub count: usize,
    /// Median.
    pub p50_ms: f64,
    /// 95th percentile.
    pub p95_ms: f64,
    /// 99th percentile.
    pub p99_ms: f64,
    /// Maximum.
    pub max_ms: f64,
    /// Mean.
    pub mean_ms: f64,
}

impl LatencySummary {
    /// Summary of samples given in microseconds.
    pub fn from_micros(samples: &[u32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let rank = |p: f64| {
            let idx = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
            sorted[idx.clamp(1, sorted.len()) - 1] as f64 / 1000.0
        };
        let sum: u64 = sorted.iter().map(|&v| v as u64).sum();
        Self {
            count: sorted.len(),
            p50_ms: rank(50.0),
            p95_ms: rank(95.0),
            p99_ms: rank(99.0),
            max_ms: *sorted.last().unwrap_or(&0) as f64 / 1000.0,
            mean_ms: sum as f64 / sorted.len() as f64 / 1000.0,
        }
    }
}

/// A lifecycle call that failed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PhaseFailure {
    /// `load`, `parameters`, `Validate #n`, `Initialize`, `Render`, `Shutdown`, …
    pub phase: String,
    /// What went wrong.
    pub error: JsFailure,
}

/// Before/after comparison of [`RestoreProbe`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RestoreCheck {
    /// Report id that was read.
    pub report_id: u8,
    /// Bytes read before `Initialize`.
    pub before: Vec<u8>,
    /// Bytes read after `Shutdown` (empty when the read failed).
    pub after: Vec<u8>,
    /// Offsets whose bytes differ (lengths compared up to the shorter one).
    pub differing_offsets: Vec<usize>,
    /// `before == after`.
    pub identical: bool,
}

impl RestoreCheck {
    fn new(report_id: u8, before: Vec<u8>, after: Vec<u8>) -> Self {
        let differing_offsets = before
            .iter()
            .zip(&after)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        let identical = before == after;
        Self {
            report_id,
            before,
            after,
            differing_offsets,
            identical,
        }
    }
}

/// Outcome of one session.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SessionReport {
    /// Device key.
    pub device: String,
    /// Plugin file.
    pub plugin: PathBuf,
    /// Plugin `Name()`.
    pub name: String,
    /// Collections offered to `Validate`.
    pub endpoints: usize,
    /// Collections `Validate` accepted.
    pub validated: usize,
    /// The collection the session opened.
    pub endpoint: Option<HidEndpointTuple>,
    /// Its path.
    pub endpoint_path: Option<String>,
    /// Lifecycle failures (at most the first five `Render` failures).
    pub failures: Vec<PhaseFailure>,
    /// `Initialize()` returned without throwing.
    pub initialized: bool,
    /// `Render()` calls.
    pub frames: u64,
    /// `Render()` calls that threw or timed out.
    pub render_errors: u64,
    /// Why the session stopped early, if it did.
    pub faulted: Option<String>,
    /// `Render()` wall time (plugin JS + sampling + HID writes).
    pub render: LatencySummary,
    /// Successful HID write / feature-send wall time.
    pub write: LatencySummary,
    /// Session start to the first successful (or dry-run) write.
    pub first_write_ms: Option<f64>,
    /// HID counters.
    pub io: IoStats,
    /// Dry-run writes and the first reads.
    pub io_log: Vec<IoRecord>,
    /// Before/after feature-report comparison.
    pub restore_check: Option<RestoreCheck>,
    /// `Shutdown(false)` ran.
    pub shutdown_called: bool,
    /// Last plugin log lines.
    pub log_tail: Vec<String>,
    /// Plugin log lines in total.
    pub log_lines: u64,
    /// QuickJS heap at the end of the session.
    pub memory_bytes: i64,
    /// Session wall time.
    pub elapsed_ms: f64,
}

impl SessionReport {
    fn new(device: &SessionDevice, plugin: &CatalogEntry) -> Self {
        Self {
            device: device.key.clone(),
            plugin: plugin.meta.source_path.clone(),
            name: plugin.meta.name.clone(),
            endpoints: device.endpoints.len(),
            validated: 0,
            endpoint: None,
            endpoint_path: None,
            failures: Vec::new(),
            initialized: false,
            frames: 0,
            render_errors: 0,
            faulted: None,
            render: LatencySummary::default(),
            write: LatencySummary::default(),
            first_write_ms: None,
            io: IoStats::default(),
            io_log: Vec::new(),
            restore_check: None,
            shutdown_called: false,
            log_tail: Vec::new(),
            log_lines: 0,
            memory_bytes: 0,
            elapsed_ms: 0.0,
        }
    }

    fn fail(&mut self, phase: impl Into<String>, error: JsFailure) {
        self.failures.push(PhaseFailure {
            phase: phase.into(),
            error,
        });
    }

    /// No lifecycle failure, no fault, no HID error.
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty() && self.faulted.is_none() && self.io.errors() == 0 && self.render_errors == 0
    }
}

/// Where a session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    /// Loading, validating, initialising.
    Starting,
    /// Rendering frames.
    Running,
    /// Shutting down.
    Stopping,
    /// Thread finished.
    Done,
}

#[derive(Debug)]
struct LiveStats {
    state: AtomicU8,
    frames: AtomicU64,
    render_errors: AtomicU64,
    hid_errors: AtomicU64,
    /// Microseconds from session start to the first write; 0 = none yet.
    first_write_us: AtomicU64,
}

/// Counters readable while the session runs.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct LiveSnapshot {
    /// Where the session is.
    pub state: SessionState,
    /// `Render()` calls so far.
    pub frames: u64,
    /// Failed `Render()` calls so far.
    pub render_errors: u64,
    /// Failed HID operations so far.
    pub hid_errors: u64,
    /// Session start to the first successful (or dry-run) write.
    pub first_write_ms: Option<f64>,
}

impl LiveStats {
    fn set_state(&self, s: SessionState) {
        self.state.store(s as u8, Ordering::Release);
    }

    fn snapshot(&self) -> LiveSnapshot {
        let state = match self.state.load(Ordering::Acquire) {
            0 => SessionState::Starting,
            1 => SessionState::Running,
            2 => SessionState::Stopping,
            _ => SessionState::Done,
        };
        LiveSnapshot {
            state,
            frames: self.frames.load(Ordering::Relaxed),
            render_errors: self.render_errors.load(Ordering::Relaxed),
            hid_errors: self.hid_errors.load(Ordering::Relaxed),
            first_write_ms: match self.first_write_us.load(Ordering::Relaxed) {
                0 => None,
                us => Some(us as f64 / 1000.0),
            },
        }
    }
}

/// A running session.
#[derive(Debug)]
pub struct SessionHandle {
    key: String,
    stop: Arc<AtomicBool>,
    live: Arc<LiveStats>,
    thread: JoinHandle<SessionReport>,
}

impl SessionHandle {
    /// Device key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Ask the session to finish its current frame, shut down and return.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    /// Counters so far.
    pub fn live(&self) -> LiveSnapshot {
        self.live.snapshot()
    }

    /// Whether the plugin thread has ended.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Wait for the plugin thread. A panic becomes a faulted report.
    pub fn join(self) -> Result<SessionReport, String> {
        self.thread.join().map_err(|p| {
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "plugin thread panicked".into())
        })
    }
}

/// Start a session for `device` driven by `plugin`, rendering frames from `frames`.
pub fn spawn(
    device: SessionDevice,
    plugin: CatalogEntry,
    backend: Arc<dyn HidBackend>,
    frames: Arc<FrameStore>,
    opts: SessionOptions,
) -> std::io::Result<SessionHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let live = Arc::new(LiveStats {
        state: AtomicU8::new(SessionState::Starting as u8),
        frames: AtomicU64::new(0),
        render_errors: AtomicU64::new(0),
        hid_errors: AtomicU64::new(0),
        first_write_us: AtomicU64::new(0),
    });
    let key = device.key.clone();
    let (stop2, live2) = (Arc::clone(&stop), Arc::clone(&live));
    let thread = std::thread::Builder::new()
        .name(format!("lightloom-plugin {key}"))
        .stack_size(PLUGIN_THREAD_STACK)
        .spawn(move || {
            let report = run(device, plugin, backend, frames, opts, &stop2, &live2);
            live2.set_state(SessionState::Done);
            report
        })?;
    Ok(SessionHandle {
        key,
        stop,
        live,
        thread,
    })
}

fn run(
    device: SessionDevice,
    plugin: CatalogEntry,
    backend: Arc<dyn HidBackend>,
    frames: Arc<FrameStore>,
    opts: SessionOptions,
    stop: &AtomicBool,
    live: &LiveStats,
) -> SessionReport {
    let started = Instant::now();
    let budgets = opts.budgets;
    let meta = &plugin.meta;
    let mut report = SessionReport::new(&device, &plugin);

    let mut ctx = DeviceContext::new(backend, device.endpoints.clone(), opts.write_mode);
    ctx.set_name(meta.name.clone());
    ctx.set_log_sink(opts.log_sink.clone());
    ctx.set_brightness(opts.brightness);
    ctx.set_sampler(SamplerConfig {
        default_scale: meta.default_scale,
        ..SamplerConfig::default()
    });
    ctx.set_geometry(meta.size, meta.led_positions.clone());
    ctx.set_placement(
        opts.placement
            .clone()
            .unwrap_or_else(|| Placement::from_defaults(meta.default_position, meta.default_scale)),
    );
    let shared = ctx.shared();

    let ropts = RuntimeOptions {
        plugin_root: Some(plugin.root.clone()),
        device: Some(Arc::clone(&shared)),
        ..RuntimeOptions::default()
    };
    let rt = match PluginRuntime::new(&meta.source_path, &ropts) {
        Ok(rt) => rt,
        Err(e) => {
            report.fail("host", e);
            return finish(report, &shared, None, started);
        }
    };
    let loaded = rt.load(budgets.load);
    if let Some(e) = loaded.error {
        report.fail("load", e);
        return finish(report, &shared, Some(&rt), started);
    }
    if let Err(e) = rt.prepare_device(device.vid, device.pid, budgets.validate) {
        report.fail("parameters", e);
        return finish(report, &shared, Some(&rt), started);
    }
    if !opts.parameters.is_empty() {
        let values = serde_json::Value::Object(opts.parameters.clone());
        if let Err(e) = rt.set_globals(&values, budgets.validate) {
            report.fail("parameters", e);
            return finish(report, &shared, Some(&rt), started);
        }
    }

    // Validate every collection; the first accepted one becomes the active endpoint.
    let mut chosen = None;
    for (i, ep) in device.endpoints.iter().enumerate() {
        match rt.validate(&ep.endpoint_tuple(), budgets.validate) {
            Ok(true) => {
                report.validated += 1;
                chosen.get_or_insert(i);
            }
            Ok(false) => {}
            Err(e) => report.fail(format!("Validate #{i}"), e),
        }
    }
    let Some(index) = chosen else {
        report.faulted = Some("no collection passed Validate(endpoint)".into());
        return finish(report, &shared, Some(&rt), started);
    };
    if let Err(e) = lock_device(&shared).open(index) {
        report.faulted = Some(format!("cannot open {}: {e}", device.endpoints[index].path));
        return finish(report, &shared, Some(&rt), started);
    }
    report.endpoint = Some(device.endpoints[index].endpoint_tuple());
    report.endpoint_path = Some(device.endpoints[index].path.clone());

    let before = opts
        .restore_probe
        .and_then(|p| lock_device(&shared).probe_feature(p.report_id, p.len));

    match rt.call_export("Initialize", CallArgs::None, budgets.initialize) {
        Ok(_) => report.initialized = true,
        Err(e) => report.fail("Initialize", e),
    }

    if report.initialized {
        live.set_state(SessionState::Running);
        let mut render_us = Vec::new();
        let mut last_seq = 0;
        let mut consecutive_timeouts = 0u32;
        while !stop.load(Ordering::Acquire) {
            let Some(seq) = frames.wait_for_newer(last_seq, Duration::from_millis(100)) else {
                continue;
            };
            last_seq = seq;
            let (_, frame) = frames.latest();
            lock_device(&shared).set_frame(frame);
            let t = Instant::now();
            let result = rt.call_export("Render", CallArgs::None, budgets.render);
            render_us.push(t.elapsed().as_micros().min(u32::MAX as u128) as u32);
            report.frames += 1;
            live.frames.fetch_add(1, Ordering::Relaxed);
            match result {
                Ok(_) => consecutive_timeouts = 0,
                Err(e) => {
                    report.render_errors += 1;
                    live.render_errors.fetch_add(1, Ordering::Relaxed);
                    let timeout = e.class == "Timeout";
                    if report.render_errors <= 5 {
                        report.fail("Render", e);
                    }
                    if timeout {
                        consecutive_timeouts += 1;
                        if consecutive_timeouts >= opts.max_render_timeouts {
                            report.faulted = Some(format!(
                                "Render() exceeded its {} ms budget {consecutive_timeouts} times in a row",
                                budgets.render.as_millis()
                            ));
                            break;
                        }
                    } else {
                        consecutive_timeouts = 0;
                    }
                }
            }
            let dev = lock_device(&shared);
            live.hid_errors.store(dev.stats().errors(), Ordering::Relaxed);
            if let Some(d) = dev.first_write() {
                live.first_write_us
                    .store((d.as_micros() as u64).max(1), Ordering::Relaxed);
            }
        }
        report.render = LatencySummary::from_micros(&render_us);
    }

    live.set_state(SessionState::Stopping);
    if !opts.skip_shutdown {
        match rt.call_export("Shutdown", CallArgs::Bool(false), budgets.shutdown) {
            Ok(called) => report.shutdown_called = called,
            Err(e) => report.fail("Shutdown", e),
        }
    }
    if let (Some(probe), Some(before)) = (opts.restore_probe, before) {
        let after = lock_device(&shared)
            .probe_feature(probe.report_id, probe.len)
            .unwrap_or_default();
        report.restore_check = Some(RestoreCheck::new(probe.report_id, before, after));
    }
    finish(report, &shared, Some(&rt), started)
}

fn finish(
    mut report: SessionReport,
    shared: &crate::device::SharedDevice,
    rt: Option<&PluginRuntime>,
    started: Instant,
) -> SessionReport {
    if let Some(rt) = rt {
        report.memory_bytes = rt.memory_used();
    }
    let mut dev = lock_device(shared);
    report.io = dev.stats().clone();
    report.write = LatencySummary::from_micros(dev.write_micros());
    report.first_write_ms = dev.first_write().map(|d| d.as_secs_f64() * 1000.0);
    report.io_log = dev.io_log().to_vec();
    let (tail, lines) = dev.log_tail();
    report.log_tail = tail;
    report.log_lines = lines;
    dev.close();
    report.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_summary_uses_nearest_rank() {
        let samples: Vec<u32> = (1..=100).map(|v| v * 1000).collect();
        let s = LatencySummary::from_micros(&samples);
        assert_eq!(s.count, 100);
        assert_eq!(
            (s.p50_ms, s.p95_ms, s.p99_ms, s.max_ms),
            (50.0, 95.0, 99.0, 100.0)
        );
        assert!((s.mean_ms - 50.5).abs() < 1e-9);
        assert_eq!(LatencySummary::from_micros(&[]), LatencySummary::default());
    }

    #[test]
    fn restore_check_lists_differences() {
        let c = RestoreCheck::new(0x52, vec![1, 2, 3], vec![1, 9, 3]);
        assert_eq!((c.identical, c.differing_offsets.clone()), (false, vec![1]));
        assert!(RestoreCheck::new(1, vec![4], vec![4]).identical);
    }
}
