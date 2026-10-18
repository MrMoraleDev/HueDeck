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
//! 4. optionally reads feature reports before `Initialize` ([`RestoreCheckMode`]):
//!    given ones, or every feature report the report descriptor declares;
//!    Subdevices and channels the plugin creates get the caller's placements
//!    and channel components ([`SessionOptions::channel_components`]);
//! 5. calls `Initialize()`, then `Render()` once per new canvas frame of the
//!    [`FrameStore`]: a slow plugin skips straight to the latest frame. Each
//!    `Render()` runs under a watchdog budget; consecutive timeouts fault the
//!    session;
//! 6. on stop calls `Shutdown(false)` (unless told not to), repeats the
//!    feature-report reads and returns a [`SessionReport`].

use crate::catalog::CatalogEntry;
use crate::children::ChildSetup;
use crate::device::{
    lock_device, panic_text, Brightness, DeviceContext, IoRecord, IoStats, LogSink, SharedDevice, WriteMode,
};
use crate::runtime::{CallArgs, JsFailure, PluginRuntime, RuntimeOptions, PLUGIN_THREAD_STACK};
use crate::stats::LatencyHistogram;
use lightloom_engine::{ComponentInstance, FrameStore, Placement, SamplerConfig};
use lightloom_hal::hid::{HidBackend, HidEndpointInfo, HidEndpointTuple};
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
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

/// Which feature reports a session compares before `Initialize` and after `Shutdown`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RestoreCheckMode {
    /// No comparison.
    #[default]
    Off,
    /// Every feature report the open collection's report descriptor declares.
    DeclaredFeatureReports,
    /// These reports only.
    Reports(Vec<RestoreProbe>),
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
    /// Before/after feature-report comparison.
    pub restore_check: RestoreCheckMode,
    /// Receives every plugin log line.
    pub log_sink: Option<LogSink>,
    /// Components on the plugin's channels, by channel name, in chain order
    /// (SignalRGB keeps them per device and channel name). A channel without
    /// an entry gets the placeholder `Default Strip - N`.
    pub channel_components: BTreeMap<String, Vec<ComponentInstance>>,
    /// Placements of subdevices (`<device key>:<subdevice id>`) and component
    /// instances (`ComponentId`); the others are fitted to the canvas.
    pub placements: BTreeMap<String, Placement>,
    /// Motherboard / BIOS / memory facts for `@SignalRGB/systeminfo`.
    pub system_info: Option<Arc<lightloom_hal::SystemInfo>>,
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
            restore_check: RestoreCheckMode::Off,
            log_sink: None,
            channel_components: BTreeMap::new(),
            placements: BTreeMap::new(),
            system_info: None,
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
            .field("restore_check", &self.restore_check)
            .field(
                "channel_components",
                &self.channel_components.keys().collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

pub use crate::stats::LatencySummary;

/// A lifecycle call that failed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PhaseFailure {
    /// `load`, `parameters`, `Validate #n`, `Initialize`, `Render`, `Shutdown`, …
    pub phase: String,
    /// What went wrong.
    pub error: JsFailure,
}

/// Before/after comparison of one feature report ([`RestoreCheckMode`]).
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

/// A subdevice the plugin created.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SubdeviceSummary {
    /// The plugin's id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Grid size in cells.
    pub size: [u32; 2],
}

/// A component instance on a channel.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ComponentSummary {
    /// `ComponentId`.
    pub id: String,
    /// `DisplayName`.
    pub name: String,
    /// `LedCount`.
    pub led_count: u32,
}

/// A channel the plugin added.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ChannelSummary {
    /// Channel name.
    pub name: String,
    /// The plugin's LED limit.
    pub led_limit: u32,
    /// LEDs the plugin sees (`channel.ledCount`).
    pub led_count: usize,
    /// Components in chain order.
    pub components: Vec<ComponentSummary>,
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
    /// Before/after feature-report comparisons, one per report read.
    pub restore_checks: Vec<RestoreCheck>,
    /// Why requested reports were not compared (unreadable, none declared).
    pub restore_notes: Vec<String>,
    /// `Shutdown(false)` ran.
    pub shutdown_called: bool,
    /// Subdevices at the end of the session.
    pub subdevices: Vec<SubdeviceSummary>,
    /// Channels at the end of the session.
    pub channels: Vec<ChannelSummary>,
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
            restore_checks: Vec::new(),
            restore_notes: Vec::new(),
            shutdown_called: false,
            subdevices: Vec::new(),
            channels: Vec::new(),
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

    /// No lifecycle failure, no fault, no HID error, and every requested
    /// restore check found the device as it was before.
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
            && self.faulted.is_none()
            && self.io.errors() == 0
            && self.render_errors == 0
            && self.restore_checks.iter().all(|c| c.identical)
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
    ctx.set_placement(
        opts.placement
            .clone()
            .unwrap_or_else(|| Placement::from_defaults(meta.default_position, meta.default_scale)),
    );
    let geometry = ctx.set_geometry(meta.size, meta.led_positions.clone());
    ctx.set_child_setup(ChildSetup {
        device_key: device.key.clone(),
        channel_components: opts.channel_components.clone(),
        placements: opts.placements.clone(),
    });
    ctx.set_subdevice_controller(meta.subdevice_controller || meta.supports_subdevices);
    ctx.set_system_info(opts.system_info.as_deref());
    let shared = ctx.shared();
    if let Err(e) = geometry {
        report.faulted = Some(format!("Size() export rejected: {e}"));
        return finish(report, &shared, None, started);
    }

    // Device sessions get the real host modules: an unimplemented `@SignalRGB/*`
    // module throws HostApiMissing instead of answering with stub data.
    let ropts = RuntimeOptions {
        plugin_root: Some(plugin.root.clone()),
        device: Some(Arc::clone(&shared)),
        module_stubs: false,
        ..RuntimeOptions::default()
    };
    let rt = match PluginRuntime::new(&meta.source_path, &ropts) {
        Ok(rt) => rt,
        Err(e) => {
            report.fail("host", e);
            return finish(report, &shared, None, started);
        }
    };

    // Everything up to the end of the render loop runs under catch_unwind: a
    // panic (in a native, the engine or the host) must not skip Shutdown(false).
    let mut lifecycle = Lifecycle::default();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        drive(
            &rt,
            &device,
            &shared,
            &frames,
            &opts,
            stop,
            live,
            &mut report,
            &mut lifecycle,
        )
    }));
    // A panic that unwound through the engine leaves its lock poisoned: every
    // later engine call would panic again (dropping it too). Probe once.
    let mut healthy = true;
    if let Err(panic) = outcome {
        report.faulted = Some(format!("plugin thread panicked: {}", panic_text(panic.as_ref())));
        healthy = catch_unwind(AssertUnwindSafe(|| rt.memory_used())).is_ok();
    }
    report.render = lifecycle.render.summary();

    live.set_state(SessionState::Stopping);
    if lifecycle.initialize_called && !opts.skip_shutdown {
        if healthy {
            let shutdown = catch_unwind(AssertUnwindSafe(|| {
                rt.call_export("Shutdown", CallArgs::Bool(false), budgets.shutdown)
            }));
            match shutdown {
                Ok(Ok(called)) => report.shutdown_called = called,
                Ok(Err(e)) => report.fail("Shutdown", e),
                Err(panic) => {
                    healthy = false;
                    report.fail(
                        "Shutdown",
                        JsFailure {
                            class: "HostPanic".into(),
                            message: panic_text(panic.as_ref()),
                        },
                    );
                }
            }
        } else {
            report.fail(
                "Shutdown",
                JsFailure {
                    class: "HostPanic".into(),
                    message: "not attempted: the plugin engine is unusable after a panic".into(),
                },
            );
        }
    }
    for (probe, bytes) in lifecycle.before {
        let after = lock_device(&shared)
            .probe_feature(probe.report_id, probe.len)
            .unwrap_or_default();
        report
            .restore_checks
            .push(RestoreCheck::new(probe.report_id, bytes, after));
    }
    if !healthy {
        // Dropping a poisoned engine panics; leak it (once, in an already failed session).
        std::mem::forget(rt);
        return finish(report, &shared, None, started);
    }
    finish(report, &shared, Some(&rt), started)
}

/// What [`drive`] leaves behind for the shutdown phase.
#[derive(Default)]
struct Lifecycle {
    /// `Initialize()` was called (even if it threw): `Shutdown(false)` is due.
    initialize_called: bool,
    /// Feature reports read before `Initialize`, for the restore check.
    before: Vec<(RestoreProbe, Vec<u8>)>,
    /// `Render()` wall times.
    render: LatencyHistogram,
}

/// Load → parameters → Validate → open → restore probes → Initialize → render loop.
#[allow(clippy::too_many_arguments)]
fn drive(
    rt: &PluginRuntime,
    device: &SessionDevice,
    shared: &SharedDevice,
    frames: &FrameStore,
    opts: &SessionOptions,
    stop: &AtomicBool,
    live: &LiveStats,
    report: &mut SessionReport,
    lifecycle: &mut Lifecycle,
) {
    let budgets = opts.budgets;
    let loaded = rt.load(budgets.load);
    if let Some(e) = loaded.error {
        report.fail("load", e);
        return;
    }
    if let Err(e) = rt.prepare_device(device.vid, device.pid, budgets.validate) {
        report.fail("parameters", e);
        return;
    }
    if !opts.parameters.is_empty() {
        let values = serde_json::Value::Object(opts.parameters.clone());
        if let Err(e) = rt.set_globals(&values, budgets.validate) {
            report.fail("parameters", e);
            return;
        }
    }

    // Validate every collection; the first accepted one becomes the active endpoint,
    // and set_endpoint may later switch among the accepted ones only.
    let mut validated = Vec::new();
    for (i, ep) in device.endpoints.iter().enumerate() {
        match rt.validate(&ep.endpoint_tuple(), budgets.validate) {
            Ok(true) => validated.push(i),
            Ok(false) => {}
            Err(e) => report.fail(format!("Validate #{i}"), e),
        }
    }
    report.validated = validated.len();
    let Some(&index) = validated.first() else {
        report.faulted = Some("no collection passed Validate(endpoint)".into());
        return;
    };
    {
        let mut dev = lock_device(shared);
        dev.set_validated(validated.clone());
        if let Err(e) = dev.open(index) {
            drop(dev);
            report.faulted = Some(format!("cannot open {}: {e}", device.endpoints[index].path));
            return;
        }
    }
    report.endpoint = Some(device.endpoints[index].endpoint_tuple());
    report.endpoint_path = Some(device.endpoints[index].path.clone());

    let probes: Vec<RestoreProbe> = match &opts.restore_check {
        RestoreCheckMode::Off => Vec::new(),
        RestoreCheckMode::Reports(list) => list.clone(),
        RestoreCheckMode::DeclaredFeatureReports => {
            let declared = lock_device(shared).feature_reports();
            if declared.is_empty() {
                report
                    .restore_notes
                    .push("the report descriptor declares no feature reports (or could not be read)".into());
            }
            declared
                .iter()
                .map(|r| RestoreProbe {
                    report_id: r.id,
                    len: r.len,
                })
                .collect()
        }
    };
    for p in probes {
        match lock_device(shared).probe_feature(p.report_id, p.len) {
            Some(bytes) => lifecycle.before.push((p, bytes)),
            None => report.restore_notes.push(format!(
                "feature report 0x{:02X} ({} bytes) could not be read before Initialize",
                p.report_id, p.len
            )),
        }
    }

    lifecycle.initialize_called = true;
    match rt.call_export("Initialize", CallArgs::None, budgets.initialize) {
        Ok(_) => report.initialized = true,
        Err(e) => report.fail("Initialize", e),
    }
    if let Some(fatal) = lock_device(shared).fatal() {
        report.faulted = Some(fatal.to_string());
        return;
    }
    if !report.initialized {
        return;
    }

    live.set_state(SessionState::Running);
    let mut last_seq = 0;
    let mut consecutive_timeouts = 0u32;
    while !stop.load(Ordering::Acquire) {
        let Some(seq) = frames.wait_for_newer(last_seq, Duration::from_millis(100)) else {
            continue;
        };
        last_seq = seq;
        let (_, frame) = frames.latest();
        lock_device(shared).set_frame(frame);
        let t = Instant::now();
        let result = rt.call_export("Render", CallArgs::None, budgets.render);
        lifecycle.render.record(t.elapsed());
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
        let dev = lock_device(shared);
        live.hid_errors.store(dev.stats().errors(), Ordering::Relaxed);
        if let Some(d) = dev.first_write() {
            live.first_write_us
                .store((d.as_micros() as u64).max(1), Ordering::Relaxed);
        }
        if let Some(fatal) = dev.fatal() {
            report.faulted = Some(fatal.to_string());
            break;
        }
    }
}

fn finish(
    mut report: SessionReport,
    shared: &SharedDevice,
    rt: Option<&PluginRuntime>,
    started: Instant,
) -> SessionReport {
    if let Some(rt) = rt {
        report.memory_bytes = rt.memory_used();
    }
    let mut dev = lock_device(shared);
    report.io = dev.stats().clone();
    report.write = dev.write_latency();
    report.first_write_ms = dev.first_write().map(|d| d.as_secs_f64() * 1000.0);
    report.io_log = dev.io_log().to_vec();
    let (tail, lines) = dev.log_tail();
    report.log_tail = tail;
    report.log_lines = lines;
    let children = dev.children();
    report.subdevices = children
        .subdevices()
        .iter()
        .map(|s| SubdeviceSummary {
            id: s.id.clone(),
            name: s.name.clone(),
            size: s.size(),
        })
        .collect();
    let counts = children.led_counts();
    report.channels = children
        .channels()
        .iter()
        .zip(counts)
        .map(|(ch, led_count)| ChannelSummary {
            name: ch.name.clone(),
            led_limit: ch.led_limit,
            led_count,
            components: ch
                .components
                .iter()
                .map(|c| ComponentSummary {
                    id: c.instance.id.clone(),
                    name: c.instance.component.display_name.clone(),
                    led_count: c.instance.component.led_count,
                })
                .collect(),
        })
        .collect();
    dev.close();
    report.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_check_lists_differences() {
        let c = RestoreCheck::new(0x52, vec![1, 2, 3], vec![1, 9, 3]);
        assert_eq!((c.identical, c.differing_offsets.clone()), (false, vec![1]));
        assert!(RestoreCheck::new(1, vec![4], vec![4]).identical);
    }
}
