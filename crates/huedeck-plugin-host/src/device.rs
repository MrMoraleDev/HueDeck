// SPDX-License-Identifier: GPL-3.0-or-later
//! Native backing of the compat `device` object in a real device session.
//!
//! [`DeviceContext`] owns the open HID endpoint of one physical device, the
//! canvas view that `device.color(x, y)` samples, and I/O statistics. The JS
//! host (`js/host.js` in device mode) normalises arguments into plain byte
//! arrays and calls the natives [`install`] puts on
//! `globalThis.__huedeckNative`; the host module captures that object and
//! deletes the global before any plugin code runs. Subdevices and channels
//! (with their component instances) live in [`Children`], sampled from the
//! same frame as the device.
//!
//! I/O failures never throw into plugin code (hid-usb.md): writes return `-1`,
//! reads return a zero-filled buffer with `getLastReadSize() == 0`. Failures
//! are counted in [`IoStats`] and logged once per operation kind.
//!
//! `rawusb` and `hybrid` plugins (and HID plugins after
//! `device.setProtocol("hybrid")`) also reach the device's raw USB side
//! ([`DeviceContext::set_usb`]): bulk and control transfers through the
//! interfaces WinUSB owns. OUT transfers return the bytes sent or `-1`, IN
//! transfers the bytes received, empty on timeout or failure.

use crate::battery::{BatteryStatus, DEFAULT_WARNING_LEVEL};
use crate::children::{ChildSetup, Children};
use crate::runtime::Deadline;
use crate::stats::{LatencyHistogram, LatencySummary};
use huedeck_engine::{
    apply_brightness, compute_offsets, effective_brightness, DeviceGeometry, DirectLeds, Frame, OffsetTable,
    Placement, Rgb, SamplerConfig,
};
use huedeck_hal::hid::{HidBackend, HidEndpointInfo, HidEndpointTuple, HidHandle, ReportSpec};
use huedeck_hal::usb::{
    UsbBackend, UsbDeviceInfo, UsbHandle, DEFAULT_TIMEOUT_MS, ENDPOINT_IN, MAX_TIMEOUT_MS,
};
use huedeck_hal::{HalError, HalResult};
use rquickjs::{Ctx, Function, Object};
use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Longest single `device.pause(ms)` honoured, so a plugin cannot park its thread indefinitely.
pub const MAX_PAUSE: Duration = Duration::from_millis(1000);
/// A pause checks for a stop request this often.
const PAUSE_SLICE: Duration = Duration::from_millis(10);
/// A blocking read checks for a stop request this often.
const READ_SLICE: Duration = Duration::from_millis(100);
/// Largest report buffer a plugin may request or send.
pub const MAX_REPORT_LEN: usize = 1 << 16;
/// Largest LED grid side `Size()` / `device.setSize` may declare.
pub const MAX_GRID_SIDE: u32 = 1024;
/// Largest LED grid (cells) `Size()` / `device.setSize` may declare.
pub const MAX_GRID_CELLS: u32 = 65_536;
/// Log lines kept for the session report.
const LOG_TAIL: usize = 50;
/// I/O records kept for the session report (dry-run writes, first reads).
const IO_LOG_LIMIT: usize = 64;
/// Distinct messages [`DeviceContext::log_once`] remembers.
const LOG_ONCE_LIMIT: usize = 256;
/// Longest log line kept or forwarded (characters).
const MAX_LOG_CHARS: usize = 4096;
/// Longest id or name a plugin may pass (characters): subdevices, channels, components.
pub const MAX_ID_CHARS: usize = 256;
/// One pulse of the alert override (see [`DeviceContext::set_alert`]).
pub const ALERT_PERIOD: Duration = Duration::from_millis(1600);
/// Dimmest point of the alert pulse (fraction of the alert colour).
const ALERT_FLOOR: f32 = 0.15;

/// `s` cut to at most `max` characters (strings copied out of the engine escape its heap cap).
pub(crate) fn capped(mut s: String, max: usize) -> String {
    if let Some((i, _)) = s.char_indices().nth(max) {
        s.truncate(i);
    }
    s
}

/// A device context shared between the session thread and the JS natives.
pub type SharedDevice = Arc<Mutex<DeviceContext>>;

/// Lock a [`SharedDevice`], recovering from a poisoned lock.
pub fn lock_device(device: &SharedDevice) -> MutexGuard<'_, DeviceContext> {
    device.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether writes reach the hardware.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteMode {
    /// Writes reach the device.
    #[default]
    Live,
    /// Output and feature writes are recorded and reported as successful but
    /// never sent. Reads still reach the device: plugins need their restore image.
    DryRun,
}

/// Kind of a recorded I/O operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IoKind {
    /// Output report (`device.write`).
    Write,
    /// SET_REPORT(Feature) (`device.send_report`).
    SendFeature,
    /// GET_REPORT(Feature) (`device.get_report`).
    GetFeature,
    /// GET_REPORT(Input) (`device.input_report`).
    GetInput,
    /// Interrupt IN read (`device.read`).
    Read,
    /// Raw USB bulk OUT (`bytes` = `[endpoint, data…]`).
    BulkOut,
    /// Raw USB bulk IN (`bytes` = `[endpoint, data received…]`).
    BulkIn,
    /// Raw USB control OUT (`bytes` = the 8-byte setup packet, then the data).
    ControlOut,
    /// Raw USB control IN (`bytes` = the setup packet, then the data received).
    ControlIn,
}

/// Which transport `device.*` I/O uses: the plugin's `Type()` at start,
/// `device.setProtocol(...)` later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    /// HID reports only.
    #[default]
    Hid,
    /// Raw USB transfers only (no HID endpoint).
    RawUsb,
    /// HID reports and raw USB transfers.
    Hybrid,
}

/// A control transfer's setup fields (`wLength` comes from the data).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlSetup {
    /// `bmRequestType`.
    pub request_type: u8,
    /// `bRequest`.
    pub request: u8,
    /// `wValue`.
    pub value: u16,
    /// `wIndex`.
    pub index: u16,
}

impl ControlSetup {
    /// The setup packet (little-endian, `wLength` = `length`) followed by `data`.
    pub fn packet(&self, length: usize, data: &[u8]) -> Vec<u8> {
        let length = u16::try_from(length).unwrap_or(u16::MAX);
        let mut out = vec![self.request_type, self.request];
        out.extend_from_slice(&self.value.to_le_bytes());
        out.extend_from_slice(&self.index.to_le_bytes());
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(data);
        out
    }
}

/// The raw USB side of a device.
struct UsbSide {
    backend: Arc<dyn UsbBackend>,
    info: UsbDeviceInfo,
    handle: Option<Box<dyn UsbHandle>>,
}

/// One recorded I/O operation (payload includes the report id byte).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct IoRecord {
    /// What happened.
    pub kind: IoKind,
    /// Bytes written, or bytes the device returned.
    pub bytes: Vec<u8>,
    /// `true` when the write was suppressed by [`WriteMode::DryRun`].
    pub dry_run: bool,
}

/// I/O counters of one session.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct IoStats {
    /// Successful output-report writes.
    pub writes: u64,
    /// Failed output-report writes.
    pub write_errors: u64,
    /// Successful feature-report sends.
    pub feature_sends: u64,
    /// Failed feature-report sends.
    pub feature_send_errors: u64,
    /// Successful feature-report reads.
    pub feature_gets: u64,
    /// Failed feature-report reads.
    pub feature_get_errors: u64,
    /// Successful input reads (interrupt IN or GET_REPORT(Input)).
    pub reads: u64,
    /// Failed input reads.
    pub read_errors: u64,
    /// Input reads that timed out without data.
    pub read_timeouts: u64,
    /// Successful raw USB OUT transfers (bulk and control).
    pub usb_writes: u64,
    /// Failed raw USB OUT transfers (timeouts included).
    pub usb_write_errors: u64,
    /// Successful raw USB IN transfers.
    pub usb_reads: u64,
    /// Failed raw USB IN transfers.
    pub usb_read_errors: u64,
    /// Raw USB IN transfers that timed out without data.
    pub usb_timeouts: u64,
    /// Calls that needed an open endpoint while none was open.
    pub no_endpoint: u64,
    /// Failed operations (also counted above) whose device is gone: unplugged,
    /// or plugged back in behind a stale handle.
    pub disconnects: u64,
    /// The latest I/O failed because the device is gone (any success clears it).
    pub disconnected: bool,
    /// Text of the most recent failure.
    pub last_error: Option<String>,
}

impl IoStats {
    /// Failed HID and raw USB operations of any kind.
    pub fn errors(&self) -> u64 {
        self.write_errors
            + self.usb_write_errors
            + self.usb_read_errors
            + self.feature_send_errors
            + self.feature_get_errors
            + self.read_errors
            + self.no_endpoint
    }
}

/// Global and layout brightness sliders (`0..=100`); the device slider is
/// [`Placement::brightness`]. Sampled colours carry the product of all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brightness {
    /// Global slider.
    pub global: u8,
    /// Layout slider.
    pub layout: u8,
}

impl Default for Brightness {
    fn default() -> Self {
        Self {
            global: 100,
            layout: 100,
        }
    }
}

/// Receives every plugin log line (after it is stored in the session's tail).
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

/// State behind the compat `device` object of one physical device.
pub struct DeviceContext {
    backend: Arc<dyn HidBackend>,
    endpoints: Vec<HidEndpointInfo>,
    active: Option<usize>,
    handle: Option<Box<dyn HidHandle>>,
    mode: WriteMode,
    last_read_size: usize,
    name: String,
    frame: Arc<Frame>,
    size: [u32; 2],
    led_positions: Vec<[u32; 2]>,
    placement: Placement,
    sampler: SamplerConfig,
    grid: OffsetTable,
    /// `LightingMode::Forced` colour of the placement.
    forced: Option<huedeck_engine::Rgb>,
    /// Direct colours per grid cell (row-major), from [`DirectLeds`]; win
    /// over the canvas and the forced colour.
    direct: Option<Vec<huedeck_engine::Rgb>>,
    brightness: Brightness,
    factor: f32,
    stats: IoStats,
    write_latency: LatencyHistogram,
    /// Frame capture to the first write of its `Render()`.
    canvas_to_wire: LatencyHistogram,
    /// Capture time of a frame not yet written out.
    pending_frame: Option<Instant>,
    validated: Vec<usize>,
    created: Instant,
    first_write: Option<Duration>,
    io_log: Vec<IoRecord>,
    log_tail: VecDeque<String>,
    log_lines: u64,
    log_sink: Option<LogSink>,
    warned: BTreeSet<&'static str>,
    logged: BTreeSet<String>,
    fatal: Option<String>,
    children: Children,
    system_info: String,
    /// The plugin call's watchdog (device sessions): long reads end early on a stop request.
    deadline: Option<Deadline>,
    /// `getDeviceInfo()` strings of a device without HID endpoints (SMBus).
    info_override: Option<Vec<String>>,
    /// The `battery` feature: `None` until the plugin adds it.
    battery: Option<BatteryStatus>,
    /// `battery.lowBatteryWarningLevel` (percent, the user's setting).
    battery_warning_level: u8,
    /// Alert override colour: every LED of the device and its children pulses it.
    alert: Option<Rgb>,
    /// The alert colour at the current frame's pulse phase.
    alert_now: Option<[u8; 3]>,
    /// The raw USB side (rawusb / hybrid devices).
    usb: Option<UsbSide>,
    /// Transport of `device.*` I/O.
    protocol: Protocol,
}

impl std::fmt::Debug for DeviceContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceContext")
            .field("name", &self.name)
            .field("endpoints", &self.endpoints.len())
            .field("active", &self.active)
            .field("mode", &self.mode)
            .field("size", &self.size)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

/// The message of a caught panic.
pub(crate) fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "panic without a message".into())
}

fn clamp_len(len: i32) -> usize {
    usize::try_from(len).unwrap_or(0).min(MAX_REPORT_LEN)
}

fn to_bytes(values: &[i32]) -> Vec<u8> {
    values.iter().take(MAX_REPORT_LEN).map(|&v| v as u8).collect()
}

/// A plugin's transfer timeout: [`DEFAULT_TIMEOUT_MS`] when negative, at most [`MAX_TIMEOUT_MS`].
fn usb_timeout(ms: i32) -> u32 {
    u32::try_from(ms).map_or(DEFAULT_TIMEOUT_MS, |t| t.min(MAX_TIMEOUT_MS))
}

/// `[bmRequestType, bRequest, wValue, wIndex, timeout]` from the JS host.
fn control_args(args: &[i32]) -> (ControlSetup, u32) {
    let at = |i: usize| args.get(i).copied().unwrap_or(0);
    (
        ControlSetup {
            request_type: at(0) as u8,
            request: at(1) as u8,
            value: at(2) as u16,
            index: at(3) as u16,
        },
        usb_timeout(args.get(4).copied().unwrap_or(-1)),
    )
}

fn pause_duration(ms: f64) -> Duration {
    if ms.is_finite() && ms > 0.0 {
        let max_ms = MAX_PAUSE.as_secs_f64() * 1000.0;
        Duration::from_micros((ms.min(max_ms) * 1000.0) as u64)
    } else {
        Duration::ZERO
    }
}

/// `size` when it is a usable LED grid: each side 1..=[`MAX_GRID_SIDE`],
/// at most [`MAX_GRID_CELLS`] cells.
pub fn checked_grid(size: [u32; 2]) -> Result<[u32; 2], String> {
    let [w, h] = size;
    let cells = u64::from(w) * u64::from(h);
    if w == 0 || h == 0 || w > MAX_GRID_SIDE || h > MAX_GRID_SIDE || cells > u64::from(MAX_GRID_CELLS) {
        return Err(format!(
            "LED grid {w}x{h} is outside 1..={MAX_GRID_SIDE} per side / {MAX_GRID_CELLS} cells"
        ));
    }
    Ok(size)
}

impl DeviceContext {
    /// Context for the collections `endpoints` of one physical device; nothing
    /// is opened yet. The canvas starts black, 1x1 cell, at the default placement.
    pub fn new(backend: Arc<dyn HidBackend>, endpoints: Vec<HidEndpointInfo>, mode: WriteMode) -> Self {
        let placement = Placement::default();
        let sampler = SamplerConfig::default();
        let frame = Arc::new(Frame::new(sampler.canvas_width, sampler.canvas_height));
        let grid = compute_offsets(&DeviceGeometry::full_grid(1, 1), &placement, &sampler);
        let mut ctx = Self {
            backend,
            endpoints,
            active: None,
            handle: None,
            mode,
            last_read_size: 0,
            name: String::new(),
            frame,
            size: [1, 1],
            led_positions: vec![[0, 0]],
            placement,
            sampler,
            grid,
            forced: None,
            direct: None,
            brightness: Brightness::default(),
            factor: 1.0,
            stats: IoStats::default(),
            write_latency: LatencyHistogram::new(),
            canvas_to_wire: LatencyHistogram::new(),
            pending_frame: None,
            validated: Vec::new(),
            created: Instant::now(),
            first_write: None,
            io_log: Vec::new(),
            log_tail: VecDeque::new(),
            log_lines: 0,
            log_sink: None,
            warned: BTreeSet::new(),
            logged: BTreeSet::new(),
            fatal: None,
            children: Children::default(),
            system_info: String::new(),
            deadline: None,
            info_override: None,
            battery: None,
            battery_warning_level: DEFAULT_WARNING_LEVEL,
            alert: None,
            alert_now: None,
            usb: None,
            protocol: Protocol::Hid,
        };
        ctx.update_factor();
        ctx
    }

    /// Wrap into a [`SharedDevice`].
    pub fn shared(self) -> SharedDevice {
        Arc::new(Mutex::new(self))
    }

    /// Forward plugin log lines to `sink` as well.
    pub fn set_log_sink(&mut self, sink: Option<LogSink>) {
        self.log_sink = sink;
    }

    /// Display name used in logs (`Name()` export or `device.setName`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the display name.
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Every collection of the device, in enumeration order.
    pub fn endpoints(&self) -> &[HidEndpointInfo] {
        &self.endpoints
    }

    /// The open collection, if any.
    pub fn active_endpoint(&self) -> Option<&HidEndpointInfo> {
        self.active.map(|i| &self.endpoints[i])
    }

    /// Open collection `index`. The previous one stays open (and active)
    /// unless the new open succeeds.
    pub fn open(&mut self, index: usize) -> huedeck_hal::HalResult<()> {
        let info = self
            .endpoints
            .get(index)
            .ok_or_else(|| huedeck_hal::HalError::InvalidArgument(format!("no endpoint #{index}")))?;
        let handle = self.backend.open(&info.path)?;
        // Keep what the backend learned at open time (report lengths, declared feature reports).
        self.endpoints[index].report_lengths = handle.info().report_lengths;
        self.endpoints[index].feature_reports = handle.info().feature_reports.clone();
        self.handle = Some(handle);
        self.active = Some(index);
        Ok(())
    }

    /// Collections `Validate(endpoint)` accepted, in order: the only ones
    /// `set_endpoint` may switch to.
    pub fn set_validated(&mut self, indices: Vec<usize>) {
        self.validated = indices;
    }

    /// Close the open collection and the raw USB handle.
    pub fn close(&mut self) {
        self.handle = None;
        self.active = None;
        self.close_usb();
    }

    /// Feature reports the open collection's report descriptor declares
    /// (empty when nothing is open or the backend could not read it).
    pub fn feature_reports(&self) -> Vec<ReportSpec> {
        self.active_endpoint()
            .map(|e| e.feature_reports.clone())
            .unwrap_or_default()
    }

    /// Whether writes reach the hardware.
    pub fn write_mode(&self) -> WriteMode {
        self.mode
    }

    /// Let writes reach the hardware or record them only (a save preview runs dry).
    pub fn set_write_mode(&mut self, mode: WriteMode) {
        self.mode = mode;
    }

    /// I/O counters so far.
    pub fn stats(&self) -> &IoStats {
        &self.stats
    }

    /// Latency of successful writes and feature sends.
    pub fn write_latency(&self) -> LatencySummary {
        self.write_latency.summary()
    }

    /// Frame capture to the first write that followed it (new frames only).
    pub fn canvas_to_wire(&self) -> LatencySummary {
        self.canvas_to_wire.summary()
    }

    /// Time from context creation to the first successful (or dry-run) write.
    pub fn first_write(&self) -> Option<Duration> {
        self.first_write
    }

    /// Recorded I/O (dry-run writes and the first reads), oldest first.
    pub fn io_log(&self) -> &[IoRecord] {
        &self.io_log
    }

    /// The last plugin log lines and the total number of lines.
    pub fn log_tail(&self) -> (Vec<String>, u64) {
        (self.log_tail.iter().cloned().collect(), self.log_lines)
    }

    /// LED grid size (`Size()` or `device.setSize`).
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// LED positions (`LedPositions()` or `device.setControllableLeds`).
    pub fn led_positions(&self) -> &[[u32; 2]] {
        &self.led_positions
    }

    // ------------------------------------------------------------------ canvas

    /// The frame `device.color(x, y)` samples until the next call.
    pub fn set_frame(&mut self, frame: Arc<Frame>) {
        self.set_frame_at(frame, Instant::now());
    }

    /// [`DeviceContext::set_frame`] with the frame's capture time: the first
    /// write after a new frame records its canvas-to-wire latency (a frame
    /// republished unchanged, to keep plugins ticking, is not new).
    pub fn set_frame_at(&mut self, frame: Arc<Frame>, captured: Instant) {
        if !Arc::ptr_eq(&self.frame, &frame) {
            self.pending_frame = Some(captured);
        }
        self.frame = frame;
        self.update_alert();
    }

    // ------------------------------------------------------------------ battery and alert

    /// What the plugin reported through the `battery` feature (`None`: it never added it).
    pub fn battery(&self) -> Option<BatteryStatus> {
        self.battery
    }

    /// `device.addFeature("battery")`: the device has a battery (level unknown yet).
    pub fn enable_battery(&mut self) {
        self.battery.get_or_insert_with(BatteryStatus::default);
    }

    /// `battery.setBatteryLevel(level)`.
    pub fn set_battery_level(&mut self, level: f64) {
        self.battery
            .get_or_insert_with(BatteryStatus::default)
            .set_level(level);
    }

    /// `battery.setBatteryState(state)`.
    pub fn set_battery_state(&mut self, raw: i32) {
        self.battery
            .get_or_insert_with(BatteryStatus::default)
            .set_state(raw);
    }

    /// `battery.lowBatteryWarningLevel`, percent.
    pub fn battery_warning_level(&self) -> u8 {
        self.battery_warning_level
    }

    /// Set `battery.lowBatteryWarningLevel` (clamped to `0..=100`).
    pub fn set_battery_warning_level(&mut self, level: u8) {
        self.battery_warning_level = level.min(100);
    }

    /// Override every LED of the device and its children (forced colours and
    /// brightness sliders included) with `color`, pulsing over [`ALERT_PERIOD`];
    /// `None` ends the override. The caller decides when (a low battery).
    pub fn set_alert(&mut self, color: Option<Rgb>) {
        self.alert = color;
        self.update_alert();
    }

    /// The alert override is on.
    pub fn alert(&self) -> Option<Rgb> {
        self.alert
    }

    /// The alert colour at the pulse phase of now (once per frame, so every LED
    /// of a frame agrees).
    fn update_alert(&mut self) {
        self.alert_now = self.alert.map(|c| {
            let period = ALERT_PERIOD.as_secs_f32();
            let phase = (self.created.elapsed().as_secs_f32() % period) / period;
            let wave = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
            let k = ALERT_FLOOR + (1.0 - ALERT_FLOOR) * wave;
            let scale = |v: u8| (f32::from(v) * k).round() as u8;
            [scale(c.r), scale(c.g), scale(c.b)]
        });
    }

    /// Replace every colour of a flat RGB stream with the alert colour.
    fn alert_stream(&self, mut rgb: Vec<u8>) -> Vec<u8> {
        if let Some(c) = self.alert_now {
            for px in rgb.as_chunks_mut::<3>().0 {
                px.copy_from_slice(&c);
            }
        }
        rgb
    }

    /// Sampler parameters (canvas size, pivot, sample point, `DefaultScale()`).
    pub fn set_sampler(&mut self, sampler: SamplerConfig) {
        self.sampler = sampler;
        self.children.set_sampler(sampler);
        self.rebuild_grid();
    }

    /// Where the device sits on the canvas (and whether it shows a forced colour).
    pub fn set_placement(&mut self, placement: Placement) {
        self.forced = placement.forced();
        self.placement = placement;
        self.update_factor();
        self.rebuild_grid();
    }

    /// Global and layout brightness.
    pub fn set_brightness(&mut self, brightness: Brightness) {
        self.brightness = brightness;
        self.children.set_brightness(brightness);
        self.update_factor();
    }

    /// Where the device sits on the canvas.
    pub fn placement(&self) -> &Placement {
        &self.placement
    }

    /// A child endpoint's new placement (see [`Children::set_placement`]).
    pub fn set_child_placement(&mut self, uid: &str, placement: Placement) -> bool {
        self.children.set_placement(uid, placement)
    }

    /// The device and its children as canvas endpoints; `key` is the device's uid.
    pub fn endpoint_views(&self, key: &str) -> Vec<crate::children::EndpointView> {
        let mut out = vec![crate::children::EndpointView {
            uid: key.to_string(),
            kind: crate::children::EndpointKind::Device,
            name: self.name().to_string(),
            channel: None,
            product: None,
            size: self.size(),
            leds: self.led_positions().len(),
            placement: self.placement.clone(),
        }];
        out.extend(self.children.views());
        out
    }

    /// The device brightness slider (`device.getBrightness()`), `0..=100`.
    pub fn device_brightness(&self) -> u8 {
        self.placement.brightness.min(100)
    }

    /// Set the LED grid (`Size()`), rejecting grids outside [`checked_grid`].
    pub fn set_geometry(&mut self, size: [u32; 2], led_positions: Vec<[u32; 2]>) -> Result<(), String> {
        self.size = checked_grid(size)?;
        self.led_positions = led_positions;
        self.rebuild_grid();
        Ok(())
    }

    fn set_size(&mut self, w: i32, h: i32) {
        let size = [u32::try_from(w).unwrap_or(0), u32::try_from(h).unwrap_or(0)];
        let leds = std::mem::take(&mut self.led_positions);
        if let Err(e) = self.set_geometry(size, leds.clone()) {
            self.led_positions = leds;
            self.log(&format!("device.setSize ignored: {e}"));
        }
    }

    fn set_leds(&mut self, flat: &[i32]) {
        self.led_positions = flat
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| [u32::try_from(p[0]).unwrap_or(0), u32::try_from(p[1]).unwrap_or(0)])
            .collect();
    }

    fn update_factor(&mut self) {
        self.factor = effective_brightness(
            self.brightness.global,
            self.brightness.layout,
            self.placement.brightness,
        );
    }

    fn rebuild_grid(&mut self) {
        let geometry = DeviceGeometry::full_grid(self.size[0], self.size[1]);
        self.grid = compute_offsets(&geometry, &self.placement, &self.sampler);
    }

    /// `device.color(x, y)`: the canvas colour under LED cell `(x, y)` with the
    /// brightness product applied; black outside the grid or off-canvas.
    pub fn color(&self, x: i32, y: i32) -> [u8; 3] {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return [0, 0, 0];
        };
        if x >= self.size[0] || y >= self.size[1] {
            return [0, 0, 0];
        }
        if let Some(c) = self.alert_now {
            return c;
        }
        let index = (y * self.size[0] + x) as usize;
        if let Some(cells) = &self.direct {
            let c = apply_brightness(cells.get(index).copied().unwrap_or_default(), self.factor);
            return [c.r, c.g, c.b];
        }
        if let Some(c) = self.forced {
            let c = apply_brightness(c, self.factor);
            return [c.r, c.g, c.b];
        }
        let ok = self.frame.width() == self.sampler.canvas_width
            && self.frame.height() == self.sampler.canvas_height;
        match self.grid.offsets().get(index) {
            Some(&off) if ok && off >= 0 => {
                let c = apply_brightness(self.frame.pixel_at_offset(off as usize), self.factor);
                [c.r, c.g, c.b]
            }
            _ => [0, 0, 0],
        }
    }

    /// The colour of every LED of the device (`LedPositions()` order; every
    /// cell of the grid, row by row, when it lists none), as `device.color(x, y)`
    /// returns it for the current frame.
    pub fn led_colors(&self) -> Vec<[u8; 3]> {
        if self.led_positions.is_empty() {
            let [w, h] = self.size;
            return (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .map(|(x, y)| self.color(x as i32, y as i32))
                .collect();
        }
        self.led_positions
            .iter()
            .map(|&[x, y]| self.color(x as i32, y as i32))
            .collect()
    }

    /// Take this frame's direct colours from `direct` for the device (uid
    /// `key`, colours in `LedPositions()` order) and its children. Endpoints
    /// without an entry sample the canvas again.
    pub fn apply_direct(&mut self, direct: &DirectLeds, key: &str) {
        self.direct = direct.get(key).map(|colors| {
            let [w, h] = self.size;
            let mut cells = vec![huedeck_engine::Rgb::BLACK; (w * h) as usize];
            for (p, c) in self.led_positions.iter().zip(colors.iter()) {
                if p[0] < w && p[1] < h {
                    cells[(p[1] * w + p[0]) as usize] = *c;
                }
            }
            cells
        });
        self.children.apply_direct(direct);
    }

    // ------------------------------------------------------------------ subdevices and channels

    /// Channel components and placements decided by the caller (before the plugin runs).
    pub fn set_child_setup(&mut self, setup: ChildSetup) {
        self.children.set_setup(setup);
    }

    /// What `@SignalRGB/systeminfo` reports (`None`: nothing, the plugin sees `undefined`).
    pub fn set_system_info(&mut self, info: Option<&huedeck_hal::SystemInfo>) {
        self.system_info = info
            .and_then(|i| serde_json::to_string(i).ok())
            .unwrap_or_default();
    }

    /// The subdevices and channels the plugin created.
    pub fn children(&self) -> &Children {
        &self.children
    }

    /// `SubdeviceController()` export / `device.SetIsSubdeviceController(flag)`.
    pub fn set_subdevice_controller(&mut self, flag: bool) {
        self.children.set_subdevice_controller(flag);
    }

    fn log_result(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.log_once(e);
        }
    }

    fn create_subdevice(&mut self, id: &str) {
        match self.children.create_subdevice(id) {
            Ok(line) if line.is_empty() => {}
            Ok(line) => self.log(&line),
            Err(e) => self.log_once(e),
        }
    }

    fn remove_subdevice(&mut self, id: &str) {
        let line = self.children.remove_subdevice(id);
        self.log(&line);
    }

    /// `device.subdeviceColor(id, x, y)`, black (logged once) for an unknown
    /// subdevice or a cell outside its grid.
    pub fn subdevice_color(&mut self, id: &str, x: i32, y: i32) -> [u8; 3] {
        match self.children.subdevice_color(&self.frame, id, x, y) {
            Ok(c) => self.alert_now.unwrap_or(c),
            Err(e) => {
                // One line per subdevice: out-of-bounds messages differ by coordinates.
                self.log_once_keyed(format!("subdeviceColor:{id}"), &e);
                [0, 0, 0]
            }
        }
    }

    fn add_channel(&mut self, name: &str, limit: i32, default_count: i32) {
        match self.children.add_channel(name, limit, default_count) {
            Ok(lines) => {
                for line in lines {
                    self.log(&line);
                }
            }
            Err(e) => self.log_once(e),
        }
    }

    fn remove_channel(&mut self, name: &str) {
        let line = self.children.remove_channel(name);
        self.log(&line);
    }

    /// `channel(name).getColors()` as flat RGB.
    pub fn channel_colors(&self, name: &str) -> Vec<u8> {
        self.alert_stream(self.children.channel_colors(&self.frame, name))
    }

    fn component_colors(&self, name: &str, id: &str) -> Vec<u8> {
        self.alert_stream(self.children.component_colors(&self.frame, name, id))
    }

    fn component_data(&self, name: &str, id: &str) -> String {
        self.children
            .component(name, id)
            .map(|c| c.instance.to_json().to_string())
            .unwrap_or_default()
    }

    fn component_ids(&self, name: &str) -> Vec<String> {
        self.children
            .channel(name)
            .map(|c| c.components.iter().map(|x| x.instance.id.clone()).collect())
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------ logging

    /// A failure the session must not render through (a panic in a device
    /// native); the session stops and shuts the plugin down.
    pub fn fatal(&self) -> Option<&str> {
        self.fatal.as_deref()
    }

    /// Record a fatal failure (the first one wins) and log it.
    pub fn set_fatal(&mut self, msg: String) {
        if self.fatal.is_none() {
            self.log(&msg);
            self.fatal = Some(msg);
        }
    }

    /// Store and forward one plugin log line (cut at [`MAX_LOG_CHARS`]).
    pub fn log(&mut self, text: &str) {
        let text = match text.char_indices().nth(MAX_LOG_CHARS) {
            Some((i, _)) => &text[..i],
            None => text,
        };
        self.log_lines += 1;
        if self.log_tail.len() == LOG_TAIL {
            self.log_tail.pop_front();
        }
        self.log_tail.push_back(text.chars().take(500).collect());
        tracing::info!(target: "huedeck::plugin", device = %self.name, "{text}");
        if let Some(sink) = &self.log_sink {
            sink(text);
        }
    }

    /// Log `text` the first time it occurs (plugins repeat bad calls every frame).
    fn log_once(&mut self, text: String) {
        let key = capped(text.clone(), MAX_ID_CHARS);
        self.log_once_keyed(key, &text);
    }

    /// Log `text` the first time `key` occurs.
    fn log_once_keyed(&mut self, key: String, text: &str) {
        if self.logged.len() < LOG_ONCE_LIMIT && self.logged.insert(key) {
            self.log(text);
        }
    }

    /// The answer of a read whose call is past its deadline: nothing was read.
    fn cut_off_read(&mut self, len: usize) -> Vec<u8> {
        self.last_read_size = 0;
        self.warn_once(
            "cut-off",
            "device I/O skipped: the plugin call is past its time budget or the session is stopping",
        );
        vec![0u8; len]
    }

    /// A write or switch whose call is past its deadline: not attempted.
    fn cut_off_write(&mut self) {
        self.warn_once(
            "cut-off",
            "device I/O skipped: the plugin call is past its time budget or the session is stopping",
        );
    }

    fn warn_once(&mut self, key: &'static str, text: &str) {
        if self.warned.insert(key) {
            self.log(text);
        }
    }

    fn fail(&mut self, key: &'static str, err: &huedeck_hal::HalError) {
        if matches!(err, huedeck_hal::HalError::Disconnected) {
            self.stats.disconnects += 1;
            self.stats.disconnected = true;
        }
        self.stats.last_error = Some(format!("{key}: {err}"));
        self.warn_once(
            key,
            &format!("HID {key} failed: {err} (further failures are counted, not logged)"),
        );
    }

    fn no_endpoint(&mut self, key: &'static str) {
        self.stats.no_endpoint += 1;
        self.stats.last_error = Some(format!("{key}: no endpoint is open"));
        self.warn_once(
            "no-endpoint",
            &format!("device.{key} called with no open HID endpoint"),
        );
    }

    fn record(&mut self, kind: IoKind, bytes: &[u8], dry_run: bool) {
        if self.io_log.len() < IO_LOG_LIMIT {
            self.io_log.push(IoRecord {
                kind,
                bytes: bytes.to_vec(),
                dry_run,
            });
        }
    }

    fn mark_write(&mut self, started: Option<Instant>) {
        if self.first_write.is_none() {
            self.first_write = Some(self.created.elapsed());
        }
        if let Some(captured) = self.pending_frame.take() {
            self.canvas_to_wire.record(captured.elapsed());
        }
        if let Some(t) = started {
            self.write_latency.record(t.elapsed());
        }
    }

    // ------------------------------------------------------------------ HID I/O

    /// `device.write(data, len)`: bytes written, or `-1`.
    pub fn write(&mut self, data: &[u8]) -> i32 {
        if data.is_empty() {
            return 0;
        }
        if self.mode == WriteMode::DryRun {
            self.record(IoKind::Write, data, true);
            self.stats.writes += 1;
            self.mark_write(None);
            return data.len() as i32;
        }
        let Some(handle) = self.handle.as_mut() else {
            self.no_endpoint("write");
            return -1;
        };
        let started = Instant::now();
        match handle.write(data) {
            Ok(n) => {
                self.stats.writes += 1;
                self.stats.disconnected = false;
                self.mark_write(Some(started));
                n as i32
            }
            Err(e) => {
                self.stats.write_errors += 1;
                self.fail("write", &e);
                -1
            }
        }
    }

    /// `device.send_report(data, len)`: bytes sent, or `-1`.
    pub fn send_feature(&mut self, data: &[u8]) -> i32 {
        if data.is_empty() {
            return 0;
        }
        if self.mode == WriteMode::DryRun {
            self.record(IoKind::SendFeature, data, true);
            self.stats.feature_sends += 1;
            self.mark_write(None);
            return data.len() as i32;
        }
        let Some(handle) = self.handle.as_mut() else {
            self.no_endpoint("send_report");
            return -1;
        };
        let started = Instant::now();
        match handle.send_feature_report(data) {
            Ok(n) => {
                self.stats.feature_sends += 1;
                self.stats.disconnected = false;
                self.mark_write(Some(started));
                n as i32
            }
            Err(e) => {
                self.stats.feature_send_errors += 1;
                self.fail("send_report", &e);
                -1
            }
        }
    }

    /// `device.get_report([id], len)`: `len` bytes, report id first; zero-filled
    /// with `getLastReadSize() == 0` on failure.
    pub fn get_feature(&mut self, report_id: u8, len: usize) -> Vec<u8> {
        self.last_read_size = 0;
        let mut buf = vec![0u8; len];
        if len == 0 {
            return buf;
        }
        let Some(handle) = self.handle.as_mut() else {
            self.no_endpoint("get_report");
            return buf;
        };
        buf[0] = report_id;
        let feature_len = handle.info().report_lengths.feature;
        let mut result = handle.get_feature_report(&mut buf);
        if result.is_err() && feature_len > len {
            // Windows may refuse GET_FEATURE buffers shorter than the collection's
            // longest feature report; retry with that length and keep the prefix.
            let mut long = vec![0u8; feature_len];
            long[0] = report_id;
            result = handle.get_feature_report(&mut long);
            buf.copy_from_slice(&long[..len]);
        }
        match result {
            Ok(n) => {
                self.stats.feature_gets += 1;
                self.stats.disconnected = false;
                // The real size, even beyond `len`: a plugin checking getLastReadSize()
                // must be able to tell a longer report from the one it expects.
                self.last_read_size = n;
                if n > len {
                    self.warn_once(
                        "feature-longer",
                        &format!("GET_REPORT(Feature 0x{report_id:02X}) returned {n} bytes, {len} requested"),
                    );
                }
                self.record(IoKind::GetFeature, &buf[..n.min(len)], false);
                buf
            }
            Err(e) => {
                self.stats.feature_get_errors += 1;
                self.fail("get_report", &e);
                vec![0u8; len]
            }
        }
    }

    /// Host-side GET_REPORT(Feature) that bypasses plugin bookkeeping (no
    /// statistics, no I/O log): the restore check before `Initialize` and after
    /// `Shutdown`. `None` when nothing is open or the read fails.
    pub fn probe_feature(&mut self, report_id: u8, len: usize) -> Option<Vec<u8>> {
        let handle = self.handle.as_mut()?;
        let len = len.clamp(1, MAX_REPORT_LEN);
        let feature_len = handle.info().report_lengths.feature;
        let mut buf = vec![0u8; len.max(feature_len)];
        buf[0] = report_id;
        let mut n = handle.get_feature_report(&mut buf[..len]).ok();
        if n.is_none() && feature_len > len {
            buf.fill(0);
            buf[0] = report_id;
            n = handle.get_feature_report(&mut buf).ok();
        }
        let n = n?.min(len);
        buf.truncate(n);
        Some(buf)
    }

    /// `device.input_report([id], len)`: like [`DeviceContext::get_feature`] for GET_REPORT(Input).
    pub fn get_input(&mut self, report_id: u8, len: usize) -> Vec<u8> {
        self.last_read_size = 0;
        let mut buf = vec![0u8; len];
        if len == 0 {
            return buf;
        }
        let Some(handle) = self.handle.as_mut() else {
            self.no_endpoint("input_report");
            return buf;
        };
        buf[0] = report_id;
        match handle.get_input_report(&mut buf) {
            Ok(n) => {
                let n = n.min(len);
                self.stats.reads += 1;
                self.stats.disconnected = false;
                self.last_read_size = n;
                self.record(IoKind::GetInput, &buf[..n], false);
                buf
            }
            Err(e) => {
                self.stats.read_errors += 1;
                self.fail("input_report", &e);
                vec![0u8; len]
            }
        }
    }

    /// `device.read(_, len, timeout)`: `len` bytes, report id first
    /// (zero-padded after the report); `getLastReadSize()` is the byte count.
    /// An empty array on timeout or failure (hid-usb.md).
    pub fn read(&mut self, len: usize, timeout_ms: i32) -> Vec<u8> {
        self.last_read_size = 0;
        let mut buf = vec![0u8; len];
        if len == 0 {
            return buf;
        }
        let Some(handle) = self.handle.as_mut() else {
            self.no_endpoint("read");
            return buf;
        };
        // Wait in slices so a stop request (or the call's deadline) ends a long
        // read early; the backends keep one pending read, so nothing is lost.
        let end = Instant::now() + Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0));
        let result = loop {
            let left = end.saturating_duration_since(Instant::now());
            let slice = left.min(READ_SLICE).as_millis() as i32;
            match handle.read_timeout(&mut buf, slice) {
                Ok(0)
                    if !left.is_zero()
                        && !self
                            .deadline
                            .as_ref()
                            .is_some_and(|d| d.stop_requested() || d.expired()) =>
                {
                    continue
                }
                other => break other,
            }
        };
        match result {
            Ok(0) => {
                self.stats.read_timeouts += 1;
                self.stats.disconnected = false;
                Vec::new()
            }
            Ok(n) => {
                let n = n.min(len);
                self.stats.reads += 1;
                self.stats.disconnected = false;
                self.last_read_size = n;
                buf
            }
            Err(e) => {
                self.stats.read_errors += 1;
                self.fail("read", &e);
                Vec::new()
            }
        }
    }

    /// `device.getLastReadSize()`.
    pub fn last_read_size(&self) -> usize {
        self.last_read_size
    }

    /// `device.clearReadBuffer()` / `device.flush()`.
    pub fn clear_read_buffer(&mut self) {
        if let Some(handle) = self.handle.as_mut() {
            if let Err(e) = handle.clear_read_buffer() {
                self.stats.read_errors += 1;
                self.fail("clearReadBuffer", &e);
            }
        }
    }

    /// `device.set_endpoint(interface, usage, usage_page[, collection])`:
    /// switch to the first collection `Validate` accepted that matches the
    /// tuple (`collection` is a wildcard when `None`). `false` when nothing
    /// matches or the open fails; the previous collection then stays active.
    pub fn set_endpoint(
        &mut self,
        interface: i32,
        usage: u16,
        usage_page: u16,
        collection: Option<u16>,
    ) -> bool {
        let found = self.validated.iter().copied().find(|&i| {
            self.endpoints[i]
                .endpoint_tuple()
                .matches(interface, usage, usage_page, collection)
        });
        let Some(index) = found else {
            self.log(&format!(
                "set_endpoint({interface}, {usage:#06x}, {usage_page:#06x}, {collection:?}): no validated endpoint matches"
            ));
            return false;
        };
        if self.active == Some(index) {
            return true;
        }
        match self.open(index) {
            Ok(()) => true,
            Err(e) => {
                self.fail("set_endpoint", &e);
                false
            }
        }
    }

    /// `device.getHidEndpoints()` as `[interface, usage, usage_page, collection]` rows.
    pub fn endpoint_tuples(&self) -> Vec<Vec<i32>> {
        self.endpoints
            .iter()
            .map(|e| {
                let t: HidEndpointTuple = e.endpoint_tuple();
                vec![
                    t.interface,
                    t.usage as i32,
                    t.usage_page as i32,
                    t.collection as i32,
                ]
            })
            .collect()
    }

    /// `device.getHidInfo()` as `[readLength, writeLength]` of the open collection.
    pub fn hid_info(&self) -> Vec<i32> {
        let lengths = self
            .active_endpoint()
            .map(|e| e.report_lengths)
            .unwrap_or_default();
        vec![lengths.input as i32, lengths.output as i32]
    }

    /// `device.getHidReportFields()` (HueDeck extension): the reports and
    /// report fields of the open collection, from its report descriptor, as
    /// JSON; `null` when the backend cannot read the descriptor.
    pub fn report_fields(&mut self) -> String {
        let Some(info) = self.active_endpoint().cloned() else {
            return "null".into();
        };
        let Some(handle) = self.handle.as_mut() else {
            return "null".into();
        };
        let descriptor = match handle.report_descriptor() {
            Ok(bytes) => bytes,
            Err(e) => {
                self.log_once(format!("getHidReportFields(): no report descriptor ({e})"));
                return "null".into();
            }
        };
        let parsed = match huedeck_hid_desc::parse(&descriptor) {
            Ok(parsed) => parsed,
            Err(e) => {
                self.log_once(format!(
                    "getHidReportFields(): report descriptor not parsed ({e})"
                ));
                return "null".into();
            }
        };
        let Some(tlc) = parsed
            .find(info.usage_page, info.usage)
            .or_else(|| parsed.collections.first())
        else {
            return "null".into();
        };
        report_fields_json(tlc).to_string()
    }

    // ------------------------------------------------------------------ raw USB I/O

    /// The raw USB side of the device (its WinUSB-reachable interfaces); the
    /// handle opens with [`DeviceContext::open_usb`].
    pub fn set_usb(&mut self, backend: Arc<dyn UsbBackend>, info: UsbDeviceInfo) {
        self.usb = Some(UsbSide {
            backend,
            info,
            handle: None,
        });
    }

    /// The raw USB device, if the device has one.
    pub fn usb_info(&self) -> Option<&UsbDeviceInfo> {
        self.usb.as_ref().map(|u| &u.info)
    }

    /// The transport `device.*` I/O uses now.
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// The transport the plugin's `Type()` asks for (before `Initialize`).
    pub fn set_protocol_kind(&mut self, protocol: Protocol) {
        self.protocol = protocol;
    }

    /// Whether the raw USB handle is open.
    pub fn usb_open(&self) -> bool {
        self.usb.as_ref().is_some_and(|u| u.handle.is_some())
    }

    /// Open the raw USB device (claims nothing: transfers claim the WinUSB
    /// interface they need). [`HalError::NeedsDriver`] when Windows bound no
    /// user-mode driver to it.
    pub fn open_usb(&mut self) -> HalResult<()> {
        let usb = self
            .usb
            .as_mut()
            .ok_or_else(|| HalError::NotFound("this device has no raw USB side".into()))?;
        if usb.handle.is_some() {
            return Ok(());
        }
        if let Some(problem) = usb.info.driver_problem() {
            return Err(HalError::NeedsDriver(problem));
        }
        let handle = usb.backend.open(&usb.info.path)?;
        // Keep what the backend learned at open time (endpoint lists).
        usb.info = handle.info().clone();
        usb.handle = Some(handle);
        Ok(())
    }

    /// Close the raw USB handle (its interfaces are released).
    pub fn close_usb(&mut self) {
        if let Some(u) = self.usb.as_mut() {
            u.handle = None;
        }
    }

    /// `device.setProtocol(name)`: `"hid"` closes the raw USB handle of a
    /// hybrid session, `"hybrid"` opens it (SignalRGB semantics: NZXT Kraken
    /// LCD streaming). `false` when the switch did not happen.
    pub fn set_protocol(&mut self, name: &str) -> bool {
        match name.trim().to_ascii_lowercase().as_str() {
            "hid" => match self.protocol {
                Protocol::RawUsb => {
                    self.log_once("setProtocol(\"hid\") ignored: a rawusb plugin has no HID endpoint".into());
                    false
                }
                _ => {
                    self.close_usb();
                    self.protocol = Protocol::Hid;
                    true
                }
            },
            "hybrid" => {
                if self.protocol == Protocol::RawUsb {
                    self.log_once(
                        "setProtocol(\"hybrid\") ignored: a rawusb plugin has no HID endpoint".into(),
                    );
                    return false;
                }
                if self.usb.is_none() {
                    self.log_once(
                        "setProtocol(\"hybrid\"): this device has no raw USB side; bulk/control transfers fail"
                            .into(),
                    );
                    return false;
                }
                self.protocol = Protocol::Hybrid;
                match self.open_usb() {
                    Ok(()) => true,
                    Err(e) => {
                        self.fail_usb("setProtocol", &e);
                        false
                    }
                }
            }
            other => {
                self.log_once(format!(
                    "setProtocol(\"{other}\"): unknown protocol (hid or hybrid)"
                ));
                false
            }
        }
    }

    fn fail_usb(&mut self, key: &'static str, err: &HalError) {
        if matches!(err, HalError::Disconnected) {
            self.stats.disconnects += 1;
            self.stats.disconnected = true;
        }
        self.stats.last_error = Some(format!("{key}: {err}"));
        let text = match err {
            HalError::NeedsDriver(problem) => format!("USB {key}: {problem}"),
            _ => format!("USB {key} failed: {err} (further failures are counted, not logged)"),
        };
        self.warn_once(key, &text);
    }

    /// The open raw USB handle, or the reason there is none (counted and logged once).
    fn usb_handle(&mut self, key: &'static str) -> Option<&mut Box<dyn UsbHandle>> {
        if self.protocol == Protocol::Hid {
            self.stats.no_endpoint += 1;
            self.stats.last_error = Some(format!("{key}: no raw USB session"));
            self.warn_once(
                "no-usb",
                &format!("device.{key} needs Type() \"rawusb\"/\"hybrid\" or device.setProtocol(\"hybrid\")"),
            );
            return None;
        }
        if !self.usb_open() {
            self.stats.no_endpoint += 1;
            let why = self
                .usb_info()
                .and_then(UsbDeviceInfo::driver_problem)
                .unwrap_or_else(|| "the raw USB device is not open".into());
            self.stats.last_error = Some(format!("{key}: {why}"));
            self.warn_once("no-usb", &format!("device.{key}: {why}"));
            return None;
        }
        self.usb.as_mut().and_then(|u| u.handle.as_mut())
    }

    /// `device.bulk_transfer(endpoint < 0x80, data, len[, timeout])`: bytes sent, or `-1`.
    pub fn bulk_out(&mut self, endpoint: u8, data: &[u8], timeout_ms: u32) -> i32 {
        let mut bytes = vec![endpoint & !ENDPOINT_IN];
        bytes.extend_from_slice(data);
        if self.mode == WriteMode::DryRun {
            self.record(IoKind::BulkOut, &bytes, true);
            self.stats.usb_writes += 1;
            self.mark_write(None);
            return data.len() as i32;
        }
        let Some(handle) = self.usb_handle("bulk_transfer") else {
            return -1;
        };
        let started = Instant::now();
        let mut buf = data.to_vec();
        match handle.bulk_transfer(endpoint & !ENDPOINT_IN, &mut buf, timeout_ms) {
            Ok(n) => {
                self.stats.usb_writes += 1;
                self.stats.disconnected = false;
                self.mark_write(Some(started));
                n as i32
            }
            Err(e) => {
                self.stats.usb_write_errors += 1;
                self.fail_usb("bulk_transfer", &e);
                -1
            }
        }
    }

    /// `device.bulk_transfer(endpoint >= 0x80, buffer, len[, timeout])`: the
    /// bytes received (`getLastReadSize()` of them); empty on timeout or failure.
    pub fn bulk_in(&mut self, endpoint: u8, len: usize, timeout_ms: u32) -> Vec<u8> {
        self.last_read_size = 0;
        if len == 0 {
            return Vec::new();
        }
        let Some(handle) = self.usb_handle("bulk_transfer") else {
            return Vec::new();
        };
        let mut buf = vec![0u8; len];
        let result = handle.bulk_transfer(endpoint | ENDPOINT_IN, &mut buf, timeout_ms);
        self.usb_read_result("bulk_transfer", result, buf, |n, buf| {
            let mut bytes = vec![endpoint | ENDPOINT_IN];
            bytes.extend_from_slice(&buf[..n]);
            (IoKind::BulkIn, bytes)
        })
    }

    /// `device.control_transfer(bmRequestType without 0x80, …)`: bytes sent, or `-1`.
    /// Standard requests (`SET_ADDRESS`, `SET_CONFIGURATION`, `SET_FEATURE`, …)
    /// change the device's USB state, not its lighting: refused.
    pub fn control_out(&mut self, setup: ControlSetup, data: &[u8], timeout_ms: u32) -> i32 {
        let bytes = setup.packet(data.len(), data);
        if setup.request_type & 0x60 == 0 {
            self.stats.usb_write_errors += 1;
            self.stats.last_error = Some("control_transfer: standard OUT request refused".into());
            self.log_once(format!(
                "control_transfer(0x{:02x}, 0x{:02x}, …): standard OUT requests change the device's USB state; refused",
                setup.request_type, setup.request
            ));
            return -1;
        }
        if self.mode == WriteMode::DryRun {
            self.record(IoKind::ControlOut, &bytes, true);
            self.stats.usb_writes += 1;
            self.mark_write(None);
            return data.len() as i32;
        }
        let Some(handle) = self.usb_handle("control_transfer") else {
            return -1;
        };
        let started = Instant::now();
        let mut buf = data.to_vec();
        match handle.control_transfer(
            setup.request_type,
            setup.request,
            setup.value,
            setup.index,
            &mut buf,
            timeout_ms,
        ) {
            Ok(n) => {
                self.stats.usb_writes += 1;
                self.stats.disconnected = false;
                self.mark_write(Some(started));
                n as i32
            }
            Err(e) => {
                self.stats.usb_write_errors += 1;
                self.fail_usb("control_transfer", &e);
                -1
            }
        }
    }

    /// `device.control_transfer(bmRequestType with 0x80, …, len, timeout)`:
    /// the bytes of the data stage; empty on timeout or failure.
    pub fn control_in(&mut self, setup: ControlSetup, len: usize, timeout_ms: u32) -> Vec<u8> {
        self.last_read_size = 0;
        let Some(handle) = self.usb_handle("control_transfer") else {
            return Vec::new();
        };
        let mut buf = vec![0u8; len];
        let result = handle.control_transfer(
            setup.request_type | ENDPOINT_IN,
            setup.request,
            setup.value,
            setup.index,
            &mut buf,
            timeout_ms,
        );
        self.usb_read_result("control_transfer", result, buf, |n, buf| {
            (IoKind::ControlIn, setup.packet(len, &buf[..n]))
        })
    }

    fn usb_read_result(
        &mut self,
        key: &'static str,
        result: HalResult<usize>,
        mut buf: Vec<u8>,
        record: impl FnOnce(usize, &[u8]) -> (IoKind, Vec<u8>),
    ) -> Vec<u8> {
        match result {
            Ok(n) => {
                let n = n.min(buf.len());
                self.stats.usb_reads += 1;
                self.stats.disconnected = false;
                self.last_read_size = n;
                let (kind, bytes) = record(n, &buf);
                self.record(kind, &bytes, false);
                buf.truncate(n);
                buf
            }
            Err(HalError::Timeout(_)) => {
                self.stats.usb_timeouts += 1;
                self.stats.disconnected = false;
                Vec::new()
            }
            Err(e) => {
                self.stats.usb_read_errors += 1;
                self.fail_usb(key, &e);
                Vec::new()
            }
        }
    }

    /// What `device.getDeviceInfo()` reports for a device without HID
    /// endpoints: `[manufacturer, product, serial]`.
    pub fn set_device_info(&mut self, info: [String; 3]) {
        self.info_override = Some(info.to_vec());
    }

    /// `device.getDeviceInfo()` strings: `[manufacturer, product, serial]`.
    pub fn device_info(&self) -> Vec<String> {
        if let Some(info) = &self.info_override {
            return info.clone();
        }
        let info = self.active_endpoint().or_else(|| self.endpoints.first());
        info.map(|e| vec![e.manufacturer.clone(), e.product.clone(), e.serial.clone()])
            .or_else(|| {
                self.usb_info()
                    .map(|u| vec![u.manufacturer.clone(), u.product.clone(), u.serial.clone()])
            })
            .unwrap_or_else(|| vec![String::new(), String::new(), String::new()])
    }
}

fn report_kind(kind: huedeck_hid_desc::ReportKind) -> &'static str {
    match kind {
        huedeck_hid_desc::ReportKind::Input => "input",
        huedeck_hid_desc::ReportKind::Output => "output",
        huedeck_hid_desc::ReportKind::Feature => "feature",
    }
}

/// What `device.getHidReportFields()` returns for one top-level collection.
fn report_fields_json(tlc: &huedeck_hid_desc::TopLevelCollection) -> serde_json::Value {
    let reports: Vec<serde_json::Value> = tlc
        .reports
        .iter()
        .map(|r| {
            serde_json::json!({
                "kind": report_kind(r.kind),
                "reportId": r.id,
                "bytes": r.byte_length(),
            })
        })
        .collect();
    let fields: Vec<serde_json::Value> = tlc
        .fields
        .iter()
        .map(|f| {
            serde_json::json!({
                "kind": report_kind(f.kind),
                "reportId": f.report_id,
                "bitOffset": f.bit_offset,
                "bitSize": f.bit_size,
                "count": f.count,
                "flags": f.flags,
                "logicalMin": f.logical_min,
                "logicalMax": f.logical_max,
                "collection": [f.collection.0, f.collection.1],
                "usages": f.usages.iter().map(|u| [u.page, u.min, u.max]).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({
        "usagePage": tlc.usage_page,
        "usage": tlc.usage,
        "reports": reports,
        "fields": fields,
    })
}

/// Install `globalThis.__huedeckNative`, backed by `device`. Natives that
/// block (HID I/O, `pause`) suspend `deadline`'s JavaScript budget meanwhile.
pub(crate) fn install(ctx: &Ctx<'_>, device: &SharedDevice, deadline: &Deadline) -> rquickjs::Result<()> {
    lock_device(device).deadline = Some(deadline.clone());
    let natives = Object::new(ctx.clone())?;
    // A panic must never unwind through rquickjs: with the `parallel` feature it
    // would poison the runtime's lock and every later call (Shutdown included)
    // would panic too. Each native therefore catches panics, marks the device
    // fatal (the session stops and shuts down) and throws a JS InternalError.
    //
    // Blocking natives (HID I/O) suspend the JavaScript budget while they wait.
    // QuickJS consults the interrupt handler only every ~10,000 operations, so
    // a loop around a blocking call could outlive its wall-clock cap or a stop
    // request for minutes: once the call is past its deadline, these natives
    // answer `else` at once instead, the loop spins, and the interrupt follows.
    macro_rules! native {
        (blocking $name:literal else $fallback:expr, |$d:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {
            native!(@make true, $name, $fallback, |$d $(, $arg : $ty)*| $body)
        };
        ($name:literal, |$d:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {
            native!(@make false, $name, unreachable!(), |$d $(, $arg : $ty)*| $body)
        };
        (@make $blocking:expr, $name:literal, $fallback:expr, |$d:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let shared = device.clone();
            let deadline = deadline.clone();
            let f = Function::new(ctx.clone(), move |cx: Ctx<'_>, $($arg: $ty),*| -> rquickjs::Result<_> {
                let cut_off = $blocking && deadline.expired();
                let _blocked = ($blocking && !cut_off).then(|| deadline.suspend());
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut guard = lock_device(&shared);
                    let $d: &mut DeviceContext = &mut guard;
                    if cut_off {
                        $fallback
                    } else {
                        $body
                    }
                }));
                result.map_err(|panic| {
                    let msg = format!("host panic in device.{}: {}", $name, panic_text(panic.as_ref()));
                    lock_device(&shared).set_fatal(msg.clone());
                    rquickjs::Exception::throw_internal(&cx, &msg)
                })
            })?;
            natives.set($name, f)?;
        }};
    }
    native!(blocking "hidWrite" else { d.cut_off_write(); -1 }, |d, data: Vec<i32>| d.write(&to_bytes(&data)));
    native!(blocking "hidSendFeature" else { d.cut_off_write(); -1 }, |d, data: Vec<i32>| d
        .send_feature(&to_bytes(&data)));
    native!(blocking "hidGetFeature" else d.cut_off_read(clamp_len(len)), |d, id: i32, len: i32| d
        .get_feature(id as u8, clamp_len(len)));
    native!(blocking "hidGetInput" else d.cut_off_read(clamp_len(len)), |d, id: i32, len: i32| d
        .get_input(id as u8, clamp_len(len)));
    native!(blocking "hidRead" else d.cut_off_read(0), |d, len: i32, timeout: i32| d
        .read(clamp_len(len), timeout));
    native!("lastReadSize", |d| d.last_read_size() as i32);
    native!(blocking "clearReadBuffer" else d.cut_off_write(), |d| d.clear_read_buffer());
    native!(blocking "setEndpoint" else { d.cut_off_write(); false }, |d, iface: i32, usage: i32, page: i32, col: i32| {
        match (u16::try_from(usage), u16::try_from(page)) {
            (Ok(usage), Ok(page)) => d.set_endpoint(iface, usage, page, u16::try_from(col).ok()),
            _ => false,
        }
    });
    native!("endpoints", |d| d.endpoint_tuples());
    native!("hidInfo", |d| d.hid_info());
    native!(blocking "hidReportFields" else "null".to_string(), |d| d.report_fields());
    native!("deviceInfo", |d| d.device_info());
    native!("color", |d, x: i32, y: i32| d.color(x, y).to_vec());
    native!("brightness", |d| d.device_brightness() as i32);
    native!("setSize", |d, w: i32, h: i32| d.set_size(w, h));
    native!("setLeds", |d, flat: Vec<i32>| d.set_leds(&flat));
    native!("setName", |d, name: String| d
        .set_name(capped(name, MAX_ID_CHARS)));
    native!("log", |d, text: String| d.log(&text));
    native!("systemInfo", |d| d.system_info.clone());
    // Raw USB (hid-usb.md §2: bulk_transfer, control_transfer, setProtocol).
    native!(blocking "usbBulkOut" else { d.cut_off_write(); -1 }, |d, ep: i32, data: Vec<i32>, timeout: i32| d
        .bulk_out(ep as u8, &to_bytes(&data), usb_timeout(timeout)));
    native!(blocking "usbBulkIn" else d.cut_off_read(0), |d, ep: i32, len: i32, timeout: i32| d
        .bulk_in(ep as u8, clamp_len(len), usb_timeout(timeout)));
    native!(blocking "usbControlOut" else { d.cut_off_write(); -1 }, |d, setup: Vec<i32>, data: Vec<i32>| {
        let (setup, timeout) = control_args(&setup);
        d.control_out(setup, &to_bytes(&data), timeout)
    });
    native!(blocking "usbControlIn" else d.cut_off_read(0), |d, setup: Vec<i32>, len: i32| {
        let (setup, timeout) = control_args(&setup);
        d.control_in(setup, clamp_len(len).min(usize::from(u16::MAX)), timeout)
    });
    native!(blocking "setProtocol" else { d.cut_off_write(); false }, |d, name: String| d.set_protocol(&name));
    native!("protocol", |d| match d.protocol() {
        Protocol::Hid => "hid",
        Protocol::RawUsb => "rawusb",
        Protocol::Hybrid => "hybrid",
    }
    .to_string());
    // The `battery` feature (docs/host-api/hid-usb.md §addFeature).
    native!("batteryEnable", |d| d.enable_battery());
    native!("batterySetLevel", |d, level: f64| d.set_battery_level(level));
    native!("batterySetState", |d, state: i32| d.set_battery_state(state));
    // [level (-1: never set), state, lowBatteryWarningLevel]
    native!("batteryGet", |d| {
        let b = d.battery.unwrap_or_default();
        vec![
            b.level.map_or(-1, i32::from),
            b.state.raw(),
            i32::from(d.battery_warning_level),
        ]
    });
    // Subdevices (components-channels.md §2).
    native!("subCreate", |d, id: String| d
        .create_subdevice(&capped(id, MAX_ID_CHARS)));
    native!("subRemove", |d, id: String| d.remove_subdevice(&id));
    native!("subList", |d| d.children.subdevice_ids());
    native!("subName", |d, id: String, name: String| {
        let r = d.children.set_subdevice_name(&id, capped(name, MAX_ID_CHARS));
        d.log_result(r)
    });
    native!("subSize", |d, id: String, w: i32, h: i32| {
        let r = d.children.set_subdevice_size(&id, w, h);
        d.log_result(r)
    });
    native!("subLeds", |d, id: String, names: Vec<String>, flat: Vec<i32>| {
        let r = d.children.set_subdevice_leds(&id, names, &flat);
        d.log_result(r)
    });
    native!("subLedMap", |d, id: String, map: Vec<i32>| {
        let r = d.children.set_subdevice_led_map(&id, &map);
        d.log_result(r)
    });
    native!("subImage", |d, id: String, image: String| {
        let r = d.children.set_subdevice_image(&id, image);
        d.log_result(r)
    });
    native!("subColor", |d, id: String, x: i32, y: i32| d
        .subdevice_color(&id, x, y)
        .to_vec());
    native!("setSubdeviceController", |d, flag: bool| d
        .set_subdevice_controller(flag));
    // Channels and their components (components-channels.md §1, §4).
    native!("setLedLimit", |d, n: i32| d
        .children
        .set_led_limit(u32::try_from(n).unwrap_or(0)));
    native!("ledCount", |d| d.children.total_led_count() as i32);
    native!("chAdd", |d, name: String, limit: i32, default_count: i32| d
        .add_channel(&capped(name, MAX_ID_CHARS), limit, default_count));
    native!("chRemove", |d, name: String| d.remove_channel(&name));
    native!("chNames", |d| d.children.channel_names());
    native!("chExists", |d, name: String| d.children.channel(&name).is_some());
    native!(
        "chLedCount",
        |d, name: String| d.children.channel_led_count(&name).unwrap_or(0) as i32
    );
    native!("chLedLimit", |d, name: String| d
        .children
        .channel(&name)
        .map_or(0, |c| c.led_limit as i32));
    native!("chSetLimit", |d, name: String, n: i32| {
        d.children.set_channel_limit(&name, n);
    });
    native!("chOnlyDefaults", |d, name: String| d
        .children
        .channel(&name)
        .is_none_or(|c| c.only_default_components()));
    native!("chColors", |d, name: String| d.channel_colors(&name));
    native!("chComponents", |d, name: String| d.component_ids(&name));
    native!("chComponentColors", |d, name: String, id: String| d
        .component_colors(&name, &id));
    native!("chComponentData", |d, name: String, id: String| d
        .component_data(&name, &id));
    // `device.pause(ms)` blocks the plugin thread (SignalRGB semantics) without holding the
    // lock, in slices so that a stop request ends it early.
    let d = deadline.clone();
    natives.set(
        "pause",
        Function::new(ctx.clone(), move |ms: f64| {
            if d.expired() {
                return;
            }
            let _blocked = d.suspend();
            let end = Instant::now() + pause_duration(ms);
            loop {
                let left = end.saturating_duration_since(Instant::now());
                if left.is_zero() || d.stop_requested() {
                    break;
                }
                std::thread::sleep(left.min(PAUSE_SLICE));
            }
        })?,
    )?;
    ctx.globals().set("__huedeckNative", natives)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use huedeck_engine::{Rgb, Scale};
    use huedeck_hal::mock::MockHid;
    use huedeck_hal::ReportLengths;

    /// The LampArray fixture the plugin's JavaScript tests use is what
    /// `getHidReportFields()` returns for the fixture descriptor.
    #[test]
    fn report_fields_match_the_lamparray_fixture() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/hid-lamparray/fixtures");
        let text = std::fs::read_to_string(dir.join("descriptor.hex")).unwrap();
        let hex: String = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .collect();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let parsed = huedeck_hid_desc::parse(&bytes).unwrap();
        let tlc = parsed.find(0x59, 0x01).unwrap();
        let mut expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("layout.json")).unwrap()).unwrap();
        expected.as_object_mut().unwrap().remove("$comment");
        assert_eq!(report_fields_json(tlc), expected);
    }

    fn msi_like() -> (Arc<MockHid>, HidEndpointInfo) {
        let info = MockHid::endpoint_info(
            0x1462,
            0x7C91,
            "S",
            0,
            0x0001,
            0x0000,
            0,
            ReportLengths {
                input: 64,
                output: 64,
                feature: 725,
            },
        );
        let mock = MockHid::new().add_endpoint(info.clone());
        let mut report = vec![0u8; 185];
        report[0] = 0x52;
        report[1] = 7;
        mock.set_feature_report(&info.path, report).unwrap();
        (Arc::new(mock), info)
    }

    #[test]
    fn feature_reports_round_trip_and_count() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock.clone(), vec![info], WriteMode::Live);
        assert_eq!(ctx.get_feature(0x52, 185).len(), 185, "no endpoint: zero-filled");
        assert_eq!(ctx.stats().no_endpoint, 1);
        ctx.open(0).unwrap();
        let image = ctx.get_feature(0x52, 185);
        assert_eq!((image[0], image[1], ctx.last_read_size()), (0x52, 7, 185));
        assert_eq!(ctx.send_feature(&image), 185);
        assert_eq!(mock.recorder().feature_sends(), vec![image]);
        assert_eq!(ctx.stats().feature_sends, 1);
        assert!(ctx.first_write().is_some());
        // Unknown report id: the mock fails, the plugin sees zeros and size 0.
        let missing = ctx.get_feature(0x99, 8);
        assert_eq!((missing, ctx.last_read_size()), (vec![0u8; 8], 0));
        assert_eq!(ctx.stats().feature_get_errors, 1);
    }

    #[test]
    fn dry_run_records_instead_of_sending() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock.clone(), vec![info], WriteMode::DryRun);
        ctx.open(0).unwrap();
        assert_eq!(
            ctx.get_feature(0x52, 185)[0],
            0x52,
            "reads still reach the device"
        );
        assert_eq!(ctx.send_feature(&[0x52, 1, 2]), 3);
        assert_eq!(ctx.write(&[0, 9]), 2);
        assert!(mock.recorder().feature_sends().is_empty());
        assert!(mock.recorder().writes().is_empty());
        let kinds: Vec<_> = ctx.io_log().iter().map(|r| (r.kind, r.dry_run)).collect();
        assert_eq!(
            kinds,
            vec![
                (IoKind::GetFeature, false),
                (IoKind::SendFeature, true),
                (IoKind::Write, true)
            ]
        );
    }

    #[test]
    fn color_samples_the_placed_grid_with_brightness() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock, vec![info], WriteMode::Live);
        ctx.set_geometry([6, 1], (0..6).map(|x| [x, 0]).collect())
            .unwrap();
        ctx.set_placement(Placement {
            scale: Some(Scale::uniform(8.0)),
            ..Placement::default()
        });
        let mut frame = Frame::new(320, 200);
        // LED 2 covers canvas x 16..24; its cell centre is (20, 4).
        frame.set_pixel(20, 4, Rgb::new(200, 100, 50));
        ctx.set_frame(Arc::new(frame));
        assert_eq!(ctx.color(2, 0), [200, 100, 50]);
        assert_eq!(ctx.color(0, 0), [0, 0, 0]);
        assert_eq!(ctx.color(6, 0), [0, 0, 0], "outside the grid");
        assert_eq!(ctx.color(-1, 0), [0, 0, 0]);
        ctx.set_brightness(Brightness {
            global: 50,
            layout: 100,
        });
        assert_eq!(ctx.color(2, 0), [100, 50, 25]);
    }

    #[test]
    fn a_forced_placement_ignores_the_canvas() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock, vec![info], WriteMode::Live);
        ctx.set_geometry([2, 1], vec![[0, 0], [1, 0]]).unwrap();
        ctx.set_frame(Arc::new(Frame::solid(320, 200, Rgb::new(1, 2, 3))));
        ctx.set_placement(Placement {
            enabled: false,
            lighting_mode: huedeck_engine::LightingMode::Forced,
            forced_color: "#ff8000".into(),
            brightness: 50,
            ..Placement::default()
        });
        assert_eq!(
            ctx.color(1, 0),
            [128, 64, 0],
            "forced, scaled, even when disabled"
        );
        assert_eq!(ctx.color(2, 0), [0, 0, 0], "outside the grid");
        ctx.set_placement(Placement::default());
        assert_eq!(ctx.color(0, 0), [1, 2, 3]);
    }

    #[test]
    fn direct_colours_win_over_canvas_and_forced_colour() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock, vec![info], WriteMode::Live);
        // Two LEDs, listed right to left.
        ctx.set_geometry([3, 1], vec![[2, 0], [0, 0]]).unwrap();
        ctx.set_frame(Arc::new(Frame::solid(320, 200, Rgb::new(1, 2, 3))));
        ctx.set_placement(Placement {
            lighting_mode: huedeck_engine::LightingMode::Forced,
            forced_color: "#ff8000".into(),
            ..Placement::default()
        });
        let direct = DirectLeds::new();
        let owner = direct.new_owner();
        direct.write(
            owner,
            "dev",
            2,
            0,
            &[Rgb::new(10, 20, 30), Rgb::new(40, 50, 60)],
            Duration::from_secs(60),
        );
        ctx.apply_direct(&direct, "other");
        assert_eq!(ctx.color(2, 0), [255, 128, 0], "another endpoint's colours");
        ctx.apply_direct(&direct, "dev");
        assert_eq!(ctx.color(2, 0), [10, 20, 30], "LED 0 sits at cell (2, 0)");
        assert_eq!(ctx.color(0, 0), [40, 50, 60]);
        assert_eq!(ctx.color(1, 0), [0, 0, 0], "no LED there");
        direct.release(owner);
        ctx.apply_direct(&direct, "dev");
        assert_eq!(ctx.color(2, 0), [255, 128, 0], "back to the forced colour");
    }

    #[test]
    fn the_alert_overrides_forced_colours_and_brightness_and_ends() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock, vec![info], WriteMode::Live);
        ctx.set_geometry([2, 1], vec![[0, 0], [1, 0]]).unwrap();
        ctx.set_frame(Arc::new(Frame::solid(320, 200, Rgb::new(1, 2, 3))));
        ctx.set_placement(Placement {
            lighting_mode: huedeck_engine::LightingMode::Forced,
            forced_color: "#00ff00".into(),
            brightness: 0,
            ..Placement::default()
        });
        ctx.set_alert(Some(Rgb::new(255, 0, 0)));
        let [r, g, b] = ctx.color(1, 0);
        assert!(r > 0 && g == 0 && b == 0, "{r} {g} {b}");
        assert_eq!(ctx.color(2, 0), [0, 0, 0], "outside the grid");
        assert_eq!(ctx.alert_stream(vec![9; 6]), vec![r, 0, 0, r, 0, 0]);
        ctx.set_alert(None);
        assert_eq!(ctx.color(1, 0), [0, 0, 0], "forced green at brightness 0");
        assert_eq!(ctx.alert_stream(vec![9; 3]), vec![9; 3]);
    }

    #[test]
    fn battery_setters_create_the_report() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock, vec![info], WriteMode::Live);
        assert_eq!(ctx.battery(), None);
        ctx.enable_battery();
        assert_eq!(ctx.battery().unwrap().level, None);
        ctx.set_battery_state(2);
        ctx.set_battery_level(55.0);
        let b = ctx.battery().unwrap();
        assert_eq!(
            (b.level, b.state),
            (Some(55), crate::battery::BatteryState::Charging)
        );
        ctx.set_battery_warning_level(140);
        assert_eq!(ctx.battery_warning_level(), 100);
    }

    #[test]
    fn set_endpoint_switches_between_validated_collections_only() {
        // (vid, pid, serial, interface, usage_page, usage, collection, lengths)
        let a = MockHid::endpoint_info(1, 2, "S", 1, 0xFF00, 1, 1, ReportLengths::default());
        let b = MockHid::endpoint_info(1, 2, "S", 1, 0xFF00, 2, 2, ReportLengths::default());
        let c = MockHid::endpoint_info(1, 2, "S", 1, 0xFF00, 3, 3, ReportLengths::default());
        let mock = Arc::new(
            MockHid::new()
                .add_endpoint(a.clone())
                .add_endpoint(b.clone())
                .add_endpoint(c.clone()),
        );
        let mut ctx = DeviceContext::new(mock, vec![a, b.clone(), c], WriteMode::Live);
        ctx.set_validated(vec![0, 1]);
        ctx.open(0).unwrap();
        assert!(
            ctx.set_endpoint(1, 2, 0xFF00, None),
            "3-argument form: collection is a wildcard"
        );
        assert_eq!(
            ctx.active_endpoint().map(|e| e.path.as_str()),
            Some(b.path.as_str())
        );
        assert!(ctx.set_endpoint(1, 1, 0xFF00, Some(1)));
        assert!(!ctx.set_endpoint(1, 1, 0xFF00, Some(2)), "collection mismatch");
        assert!(
            !ctx.set_endpoint(1, 3, 0xFF00, None),
            "collection 3 was not validated"
        );
        assert_eq!(ctx.active_endpoint().map(|e| e.collection), Some(1));
        assert_eq!(
            ctx.endpoint_tuples(),
            vec![
                vec![1, 1, 0xFF00, 1],
                vec![1, 2, 0xFF00, 2],
                vec![1, 3, 0xFF00, 3]
            ]
        );
    }

    #[test]
    fn a_failed_switch_keeps_the_open_collection() {
        let a = MockHid::endpoint_info(1, 2, "S", 1, 0xFF00, 1, 1, ReportLengths::default());
        // `gone` is validated but no longer present: its open fails.
        let gone = MockHid::endpoint_info(1, 2, "S", 1, 0xFF00, 2, 2, ReportLengths::default());
        let mock = Arc::new(MockHid::new().add_endpoint(a.clone()));
        let mut ctx = DeviceContext::new(mock.clone(), vec![a.clone(), gone], WriteMode::Live);
        ctx.set_validated(vec![0, 1]);
        ctx.open(0).unwrap();
        assert!(!ctx.set_endpoint(1, 2, 0xFF00, None));
        assert_eq!(
            ctx.active_endpoint().map(|e| e.path.as_str()),
            Some(a.path.as_str())
        );
        assert_eq!(ctx.write(&[0, 1]), 2, "writes still reach the old collection");
        assert_eq!(mock.recorder().writes(), vec![vec![0, 1]]);
    }

    #[test]
    fn read_timeouts_return_nothing() {
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock.clone(), vec![info.clone()], WriteMode::Live);
        ctx.open(0).unwrap();
        assert!(ctx.read(65, 0).is_empty());
        assert_eq!((ctx.last_read_size(), ctx.stats().read_timeouts), (0, 1));
        mock.script_read(&info.path, vec![0x01, 0xAA]).unwrap();
        let r = ctx.read(4, 0);
        assert_eq!((r, ctx.last_read_size()), (vec![0x01, 0xAA, 0, 0], 2));
    }

    #[test]
    fn plugin_strings_are_capped_on_char_boundaries() {
        assert_eq!(capped("abcdef".into(), 3), "abc");
        assert_eq!(capped("àèìòù".into(), 2), "àè");
        assert_eq!(capped("ab".into(), 5), "ab");
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock, vec![info], WriteMode::Live);
        ctx.log(&"y".repeat(MAX_LOG_CHARS * 2));
        let (tail, lines) = ctx.log_tail();
        assert_eq!(lines, 1);
        assert!(tail[0].chars().count() <= MAX_LOG_CHARS);
    }

    #[test]
    fn pause_is_clamped_without_panicking() {
        assert_eq!(pause_duration(-5.0), Duration::ZERO);
        assert_eq!(pause_duration(f64::NAN), Duration::ZERO);
        assert_eq!(pause_duration(f64::INFINITY), Duration::ZERO);
        assert_eq!(pause_duration(5.0), Duration::from_millis(5));
        assert_eq!(pause_duration(0.5), Duration::from_micros(500));
        assert_eq!(pause_duration(1e12), MAX_PAUSE);
        assert_eq!(pause_duration(1e23), MAX_PAUSE);
    }

    #[test]
    fn grids_are_bounded() {
        assert!(checked_grid([6, 1]).is_ok());
        assert!(checked_grid([1024, 64]).is_ok());
        assert!(checked_grid([0, 1]).is_err());
        assert!(checked_grid([1025, 1]).is_err());
        assert!(checked_grid([70_000, 70_000]).is_err());
        assert!(checked_grid([512, 512]).is_err(), "262144 cells");
        let (mock, info) = msi_like();
        let mut ctx = DeviceContext::new(mock, vec![info], WriteMode::Live);
        ctx.set_geometry([6, 1], (0..6).map(|x| [x, 0]).collect())
            .unwrap();
        ctx.set_size(70_000, 70_000);
        assert_eq!(ctx.size(), [6, 1], "an oversized setSize is ignored");
        assert_eq!(ctx.led_positions().len(), 6);
    }
}
