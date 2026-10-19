// SPDX-License-Identifier: GPL-3.0-or-later
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
//!    Changes sent through [`SessionHandle::control`] apply before the next
//!    `Render()`: parameter values (then `on<name>Changed()`), brightness,
//!    placement (then `onBrightnessChanged()` for a new device brightness);
//! 6. on stop calls `Shutdown(false)` (unless told not to), repeats the
//!    feature-report reads and returns a [`SessionReport`].
//!
//! An SMBus device ([`SessionDevice::smbus`]) has no HID collections: steps 3
//! and 4 are skipped, `bus` is the fixed-address bus of its address (through a
//! [`GuardedBus`]), a `TransactionLock` still held when `Render()` returns is
//! released by the host, and `Render()` runs at most once per
//! [`SMBUS_FRAME_INTERVAL`] whatever the canvas rate.

use crate::bus::{lock_guard, SharedGuard};
use crate::catalog::CatalogEntry;
use crate::children::ChildSetup;
pub use crate::children::{ChannelInfo, EndpointKind, EndpointView};
use crate::device::{
    lock_device, panic_text, Brightness, DeviceContext, IoRecord, IoStats, LogSink, SharedDevice, WriteMode,
};
use crate::runtime::{Budget, CallArgs, JsFailure, PluginRuntime, RuntimeOptions, PLUGIN_THREAD_STACK};
use crate::smbus::{BusHandle, GuardedBus, Phase, SmbusStats};
use crate::stats::LatencyHistogram;
use huedeck_engine::{ComponentInstance, FrameStore, Placement, SamplerConfig};
use huedeck_hal::hid::{HidBackend, HidEndpointInfo, HidEndpointTuple, NoHid};
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Shortest time between two `Render()` calls of an SMBus session (about
/// 30 fps): the pace OpenRGB drives these controllers at
/// (`RGBController_MSIGPU::TimeToSend`, 33 ms). The latest frame is rendered
/// when the time comes, so nothing is lost but the frames in between.
pub const SMBUS_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// One physical device: every HID collection that belongs to it, or the
/// SMBus address it answers at.
#[derive(Clone, Debug)]
pub struct SessionDevice {
    /// SignalRGB-shaped key (`vid:pid:serial`), or `smbus:<bus key>:<addr>`.
    pub key: String,
    /// USB vendor id (SMBus: the bus controller's PCI vendor).
    pub vid: u16,
    /// USB product id (SMBus: the bus controller's PCI device).
    pub pid: u16,
    /// Collections in enumeration order (none for SMBus devices).
    pub endpoints: Vec<HidEndpointInfo>,
    /// An SMBus device: its bus and address.
    pub smbus: Option<SmbusTarget>,
}

/// Where an SMBus device lives.
#[derive(Clone, Debug)]
pub struct SmbusTarget {
    /// The bus (shared with discovery and the other devices on it).
    pub bus: BusHandle,
    /// The device's 7-bit address.
    pub addr: u8,
}

impl SessionDevice {
    /// A HID device.
    pub fn hid(key: impl Into<String>, vid: u16, pid: u16, endpoints: Vec<HidEndpointInfo>) -> Self {
        Self {
            key: key.into(),
            vid,
            pid,
            endpoints,
            smbus: None,
        }
    }

    /// The SMBus device at `addr` on `bus`: key `smbus:<bus key>:<addr>`,
    /// VID:PID the bus controller's PCI vendor and device.
    pub fn smbus(bus: BusHandle, addr: u8) -> Self {
        let d = bus.descriptor();
        Self {
            key: format!("smbus:{}:{addr:02x}", d.key()),
            vid: d.pci.vendor,
            pid: d.pci.device,
            endpoints: Vec::new(),
            smbus: Some(SmbusTarget { bus, addr }),
        }
    }
}

/// Time budgets of the lifecycle calls: JavaScript time, time blocked in
/// `device.pause()` and HID I/O excluded ([`Budget`]), under the wall-clock caps.
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
    /// Wall-clock cap of one `Render()`, blocked time included (plugins set
    /// devices up lazily from `Render()`, taking seconds once).
    pub render_wall: Duration,
    /// Wall-clock cap of `Validate`, `Initialize` and `Shutdown`.
    pub call_wall: Duration,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            load: Duration::from_secs(10),
            validate: Duration::from_secs(1),
            initialize: Duration::from_secs(10),
            render: Duration::from_millis(250),
            shutdown: Duration::from_secs(5),
            render_wall: Duration::from_secs(30),
            call_wall: Duration::from_secs(30),
        }
    }
}

impl Budgets {
    fn call(&self, js: Duration) -> Budget {
        Budget {
            js,
            wall: self.call_wall.max(js),
        }
    }

    fn render_budget(&self) -> Budget {
        Budget {
            js: self.render,
            wall: self.render_wall.max(self.render),
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
    pub system_info: Option<Arc<huedeck_hal::SystemInfo>>,
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
    /// Frame capture to the first write of the `Render()` that used it.
    pub canvas_to_wire: LatencySummary,
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
    /// Worth knowing, not failures: calls a stop request cut short.
    pub notes: Vec<String>,
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
    /// The session was stopped because its device went away (set by
    /// [`crate::manager::DeviceManager`]); I/O errors are then expected.
    pub device_removed: bool,
    /// SMBus counters (SMBus devices only).
    pub smbus: Option<SmbusStats>,
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
            canvas_to_wire: LatencySummary::default(),
            first_write_ms: None,
            io: IoStats::default(),
            io_log: Vec::new(),
            restore_checks: Vec::new(),
            restore_notes: Vec::new(),
            notes: Vec::new(),
            shutdown_called: false,
            subdevices: Vec::new(),
            channels: Vec::new(),
            log_tail: Vec::new(),
            log_lines: 0,
            memory_bytes: 0,
            device_removed: false,
            smbus: None,
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
    /// restore check found the device as it was before. A session whose device
    /// was removed while it ran is judged without the failures removal causes
    /// (HID errors, `Render()` / `Shutdown()` / change-hook failures).
    pub fn is_clean(&self) -> bool {
        if self.device_removed {
            return self.faulted.is_none()
                && self
                    .failures
                    .iter()
                    .all(|f| f.phase == "Render" || f.phase == "Shutdown" || f.phase.starts_with("on"));
        }
        self.failures.is_empty()
            && self.faulted.is_none()
            && self.io.errors() == 0
            && self.smbus.as_ref().is_none_or(|s| s.errors == 0 && s.denied == 0)
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
    io_errors: AtomicU64,
    hid_disconnects: AtomicU64,
    hid_disconnected: AtomicBool,
    /// Microseconds from session start to the first write; 0 = none yet.
    first_write_us: AtomicU64,
    /// The device and its children as canvas endpoints (empty until `Initialize()` ran).
    endpoints: std::sync::Mutex<Arc<Vec<EndpointView>>>,
    /// The device's channels (empty until `Initialize()` ran).
    channels: std::sync::Mutex<Arc<Vec<ChannelInfo>>>,
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
    /// Failed I/O operations so far: HID operations, or for SMBus devices
    /// failed or refused bus operations.
    pub io_errors: u64,
    /// Failed HID operations whose device is gone (see [`IoStats::disconnects`]).
    pub disconnects: u64,
    /// The latest HID operation failed because the device is gone.
    pub disconnected: bool,
    /// Session start to the first successful (or dry-run) write.
    pub first_write_ms: Option<f64>,
}

impl LiveStats {
    fn set_state(&self, s: SessionState) {
        self.state.store(s as u8, Ordering::Release);
    }

    fn endpoints(&self) -> Arc<Vec<EndpointView>> {
        Arc::clone(&self.endpoints.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Replace the endpoint views and channel list when they changed.
    fn publish_endpoints(&self, (views, channels): (Vec<EndpointView>, Vec<ChannelInfo>)) {
        let mut slot = self.endpoints.lock().unwrap_or_else(|e| e.into_inner());
        if **slot != views {
            *slot = Arc::new(views);
        }
        drop(slot);
        let mut slot = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        if **slot != channels {
            *slot = Arc::new(channels);
        }
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
            io_errors: self.io_errors.load(Ordering::Relaxed),
            disconnects: self.hid_disconnects.load(Ordering::Relaxed),
            disconnected: self.hid_disconnected.load(Ordering::Relaxed),
            first_write_ms: match self.first_write_us.load(Ordering::Relaxed) {
                0 => None,
                us => Some(us as f64 / 1000.0),
            },
        }
    }
}

/// A change applied to a running session before its next `Render()`.
#[derive(Clone, Debug)]
pub enum Control {
    /// New values of plugin parameters (`ControllableParameters` or
    /// `device.addProperty` names): assigned to their globals, then each
    /// parameter's `on<name>Changed()` export is called, if there is one.
    Parameters(serde_json::Map<String, serde_json::Value>),
    /// Global and layout brightness.
    Brightness(Brightness),
    /// Where the device sits on the canvas; a new device brightness then
    /// calls the plugin's `onBrightnessChanged()`.
    Placement(Placement),
    /// Where a subdevice (`<device key>:<id>`) or a component instance (its id) sits.
    ChildPlacement {
        /// The child's uid.
        uid: String,
        /// Its placement.
        placement: Placement,
    },
}

/// A running session.
#[derive(Debug)]
pub struct SessionHandle {
    key: String,
    started: Instant,
    control: Sender<Control>,
    stop: Arc<AtomicBool>,
    live: Arc<LiveStats>,
    thread: JoinHandle<SessionReport>,
}

impl SessionHandle {
    /// Device key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// When the session was spawned.
    pub fn started(&self) -> Instant {
        self.started
    }

    /// Queue `control` for the session (applied before the next `Render()`).
    /// `false` when the session has ended.
    pub fn control(&self, control: Control) -> bool {
        self.control.send(control).is_ok()
    }

    /// The device and its children as canvas endpoints, as of the last update
    /// (after `Initialize()`, after placement changes and about once a second).
    pub fn endpoints(&self) -> Arc<Vec<EndpointView>> {
        self.live.endpoints()
    }

    /// The device's channels, updated with [`SessionHandle::endpoints`].
    pub fn channels(&self) -> Arc<Vec<ChannelInfo>> {
        Arc::clone(&self.live.channels.lock().unwrap_or_else(|e| e.into_inner()))
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
        io_errors: AtomicU64::new(0),
        hid_disconnects: AtomicU64::new(0),
        hid_disconnected: AtomicBool::new(false),
        first_write_us: AtomicU64::new(0),
        endpoints: std::sync::Mutex::new(Arc::new(Vec::new())),
        channels: std::sync::Mutex::new(Arc::new(Vec::new())),
    });
    let key = device.key.clone();
    let started = Instant::now();
    let (control, controls) = channel();
    let (stop2, live2) = (Arc::clone(&stop), Arc::clone(&live));
    let thread = std::thread::Builder::new()
        .name(format!("huedeck-plugin {key}"))
        .stack_size(PLUGIN_THREAD_STACK)
        .spawn(move || {
            let report = run(device, plugin, backend, frames, opts, &stop2, &live2, &controls);
            drop(stop2);
            live2.set_state(SessionState::Done);
            report
        })?;
    Ok(SessionHandle {
        key,
        started,
        control,
        stop,
        live,
        thread,
    })
}

#[allow(clippy::too_many_arguments)]
fn run(
    device: SessionDevice,
    plugin: CatalogEntry,
    backend: Arc<dyn HidBackend>,
    frames: Arc<FrameStore>,
    opts: SessionOptions,
    stop: &Arc<AtomicBool>,
    live: &LiveStats,
    controls: &Receiver<Control>,
) -> SessionReport {
    let started = Instant::now();
    let budgets = opts.budgets;
    let meta = &plugin.meta;
    let mut report = SessionReport::new(&device, &plugin);

    // SMBus devices never open a HID collection.
    let backend: Arc<dyn HidBackend> = if device.smbus.is_some() {
        Arc::new(NoHid)
    } else {
        backend
    };
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
    if let Some(t) = &device.smbus {
        ctx.set_device_info([
            t.bus.descriptor().name.clone(),
            meta.name.clone(),
            format!("0x{:02x}", t.addr),
        ]);
    }
    let shared = ctx.shared();
    let guard: Option<SharedGuard> = device.smbus.as_ref().map(|t| {
        let mut g = GuardedBus::new(t.bus.clone(), Phase::Run { addr: t.addr }, opts.write_mode);
        g.deny_registers(meta.smbus_denied_registers.iter().copied());
        g.set_batch_writes(meta.smbus_batch_writes);
        let dev = Arc::clone(&shared);
        g.set_log_sink(Some(Arc::new(move |line: &str| lock_device(&dev).log(line))));
        g.shared()
    });
    if let Err(e) = geometry {
        report.faulted = Some(format!("Size() export rejected: {e}"));
        return finish(report, &shared, None, started);
    }

    // Device sessions get the real host modules: an unimplemented `@SignalRGB/*`
    // module throws HostApiMissing instead of answering with stub data.
    let ropts = RuntimeOptions {
        plugin_root: Some(plugin.root.clone()),
        device: Some(Arc::clone(&shared)),
        smbus: guard.clone(),
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
            guard.as_ref(),
            &frames,
            &opts,
            stop,
            live,
            controls,
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
                rt.call_export("Shutdown", CallArgs::Bool(false), budgets.call(budgets.shutdown))
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
    if let Some(g) = &guard {
        let mut g = lock_guard(g);
        g.force_unlock();
        report.smbus = Some(g.stats().clone());
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
    guard: Option<&SharedGuard>,
    frames: &FrameStore,
    opts: &SessionOptions,
    stop: &Arc<AtomicBool>,
    live: &LiveStats,
    controls: &Receiver<Control>,
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

    if let Some(target) = &device.smbus {
        // An SMBus device has no collections to validate or reports to probe:
        // the scan found it, the guard binds the session to its address.
        report.endpoint_path = Some(format!("{} @0x{:02x}", target.bus.descriptor().name, target.addr));
    } else {
        // Validate every collection; the first accepted one becomes the active endpoint,
        // and set_endpoint may later switch among the accepted ones only.
        let mut validated = Vec::new();
        for (i, ep) in device.endpoints.iter().enumerate() {
            if stop.load(Ordering::Acquire) {
                report.notes.push("stopped while validating endpoints".into());
                return;
            }
            match rt.validate(&ep.endpoint_tuple(), budgets.call(budgets.validate)) {
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
                    report.restore_notes.push(
                        "the report descriptor declares no feature reports (or could not be read)".into(),
                    );
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
    }

    if stop.load(Ordering::Acquire) {
        report.notes.push("stopped before Initialize".into());
        return;
    }
    lifecycle.initialize_called = true;
    // A stop request cuts Initialize short (Shutdown still runs).
    match rt.call_export_until(
        "Initialize",
        CallArgs::None,
        budgets.call(budgets.initialize),
        stop,
    ) {
        Ok(_) => report.initialized = true,
        Err(e) if e.is_stop() => report
            .notes
            .push("Initialize was cut short by a stop request".into()),
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
    {
        let dev = lock_device(shared);
        live.publish_endpoints((dev.endpoint_views(&device.key), dev.children().channel_infos()));
    }
    let mut last_seq = 0;
    let mut consecutive_timeouts = 0u32;
    let mut lock_warned = false;
    let mut last_render: Option<Instant> = None;
    while !stop.load(Ordering::Acquire) {
        if device.smbus.is_some() {
            let left = last_render.map_or(Duration::ZERO, |t| {
                SMBUS_FRAME_INTERVAL.saturating_sub(t.elapsed())
            });
            if !left.is_zero() {
                std::thread::sleep(left);
                continue;
            }
        }
        let Some(seq) = frames.wait_for_newer(last_seq, Duration::from_millis(100)) else {
            continue;
        };
        // Asked to stop while waiting: no new frame.
        if stop.load(Ordering::Acquire) {
            break;
        }
        last_seq = seq;
        let mut controlled = false;
        while let Ok(control) = controls.try_recv() {
            apply_control(rt, shared, control, &budgets, stop, report);
            controlled = true;
        }
        if controlled || report.frames.is_multiple_of(30) {
            let dev = lock_device(shared);
            live.publish_endpoints((dev.endpoint_views(&device.key), dev.children().channel_infos()));
        }
        let (_, frame, captured) = frames.latest_stamped();
        lock_device(shared).set_frame_at(frame, captured);
        let t = Instant::now();
        last_render = Some(t);
        let result = rt.call_export_until("Render", CallArgs::None, budgets.render_budget(), stop);
        // Cut short by a stop request (it outlived STOP_GRACE): not the plugin's
        // fault, and not a whole frame.
        if matches!(&result, Err(e) if e.is_stop()) {
            break;
        }
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
                            "Render() exceeded its budget ({} ms of JavaScript, {} ms in all) \
                             {consecutive_timeouts} times in a row",
                            budgets.render.as_millis(),
                            budgets.render_wall.as_millis()
                        ));
                        break;
                    }
                } else {
                    consecutive_timeouts = 0;
                }
            }
        }
        let mut bus_errors = None;
        if let Some(g) = guard {
            let mut g = lock_guard(g);
            let left_locked = g.force_unlock();
            bus_errors = Some(g.stats().errors + g.stats().denied);
            drop(g);
            if left_locked && !lock_warned {
                lock_warned = true;
                lock_device(shared).log("bus: Transaction Lock wasn't unlocked before Render() returned!");
            }
        }
        let dev = lock_device(shared);
        live.io_errors.store(
            bus_errors.unwrap_or_else(|| dev.stats().errors()),
            Ordering::Relaxed,
        );
        live.hid_disconnects
            .store(dev.stats().disconnects, Ordering::Relaxed);
        live.hid_disconnected
            .store(dev.stats().disconnected, Ordering::Relaxed);
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

/// Apply one [`Control`] between two `Render()` calls.
fn apply_control(
    rt: &PluginRuntime,
    shared: &SharedDevice,
    control: Control,
    budgets: &Budgets,
    stop: &Arc<AtomicBool>,
    report: &mut SessionReport,
) {
    let hook = |name: &str, report: &mut SessionReport| match rt.call_export_until(
        name,
        CallArgs::None,
        budgets.call(budgets.validate),
        stop,
    ) {
        Ok(_) => {}
        Err(e) if e.is_stop() => report
            .notes
            .push(format!("{name} was cut short by a stop request")),
        Err(e) => report.fail(name, e),
    };
    match control {
        Control::Parameters(values) => {
            let names: Vec<String> = values.keys().cloned().collect();
            if let Err(e) = rt.set_globals(&serde_json::Value::Object(values), budgets.validate) {
                report.fail("parameters", e);
                return;
            }
            for name in names {
                hook(&format!("on{name}Changed"), report);
            }
        }
        Control::Brightness(b) => lock_device(shared).set_brightness(b),
        Control::ChildPlacement { uid, placement } => {
            if !lock_device(shared).set_child_placement(&uid, placement) {
                report.notes.push(format!("no endpoint {uid} to place"));
            }
        }
        Control::Placement(p) => {
            let changed = {
                let mut dev = lock_device(shared);
                let before = dev.device_brightness();
                dev.set_placement(p);
                dev.device_brightness() != before
            };
            if changed {
                hook("onBrightnessChanged", report);
            }
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
    report.canvas_to_wire = dev.canvas_to_wire();
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
