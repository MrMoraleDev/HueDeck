// SPDX-License-Identifier: GPL-3.0-or-later
//! The built-in LampArray session: a LampArray of the OS backend
//! (`lightloom_hal::lamparray`, Windows Dynamic Lighting) driven like a plugin
//! device, without JavaScript.
//!
//! [`SessionDevice::lamparray`] makes the device; [`builtin_entry`] the
//! catalogue entry that stands in for its plugin (`builtin:lamparray`), with
//! the grid the lamps give: one cell per [`GRID_CELLS`]-th of the longer side
//! of the bounding box, as the `hid-lamparray` plugin lays the same device
//! out, so a device keeps its canvas footprint whichever route drives it.
//! [`crate::session::spawn`] runs [`run`] for such a device: the session
//! samples the canvas at the lamp positions through the same
//! [`DeviceContext`] a plugin's `device.color(x, y)` reads (placement,
//! brightness sliders, forced colours, direct LEDs and the alert pulse all
//! apply) and sends one `SetColorsForIndices` per new frame.
//!
//! Windows lets only the foreground app or the ambient lighting controller
//! the user put first in Settings set colours. While the handle reports the
//! device unavailable the session skips writes and shows as paused
//! ([`crate::session::LiveSnapshot::paused`]); once it is available again the
//! next frame is sent in full. Writes honour the device's minimum update
//! interval (a frame that comes too early waits for the next one), unchanged
//! colours are resent once a second only, and a stop releases the device to
//! the OS (the next controller, or autonomous mode); with
//! [`ShutdownMode::Off`] it turns the lamps off first. `Keep` cannot be
//! honoured once the device is handed back, and a save is refused (no device
//! memory).

use crate::catalog::CatalogEntry;
use crate::device::{lock_device, DeviceContext, SharedDevice, WriteMode};
use crate::meta::{DeviceType, PluginMeta};
use crate::runtime::JsFailure;
use crate::save::{SaveOutcome, ShutdownMode};
use crate::session::{Control, LiveStats, SessionDevice, SessionOptions, SessionReport, SessionState};
use crate::stats::LatencyHistogram;
use lightloom_engine::{FrameStore, Placement, SamplerConfig};
use lightloom_hal::hid::NoHid;
use lightloom_hal::lamparray::{LampArrayBackend, LampArrayInfo, LampArrayKind};
use lightloom_hal::HalError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Plugin path of the built-in LampArray driver (no file behind it).
pub const BUILTIN_PATH: &str = "builtin:lamparray";
/// Plugin name of the built-in LampArray driver.
pub const BUILTIN_NAME: &str = "Windows Dynamic Lighting";
/// Grid cells along the longer side of the bounding box (as `plugins/hid-lamparray`).
pub const GRID_CELLS: u32 = 32;
/// Unchanged colours are sent again after this long.
pub const KEEPALIVE: Duration = Duration::from_secs(1);

/// A LampArray of the OS backend.
#[derive(Clone)]
pub struct LampArrayTarget {
    /// The backend that enumerated it (opens it).
    pub backend: Arc<dyn LampArrayBackend>,
    /// What the backend reported.
    pub info: LampArrayInfo,
}

impl std::fmt::Debug for LampArrayTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LampArrayTarget")
            .field("id", &self.info.id)
            .field("lamps", &self.info.lamps.len())
            .finish_non_exhaustive()
    }
}

impl SessionDevice {
    /// A LampArray of `backend`: key [`lightloom_hal::lamparray::device_key`],
    /// no HID collections.
    pub fn lamparray(backend: Arc<dyn LampArrayBackend>, info: LampArrayInfo) -> Self {
        Self {
            key: lightloom_hal::lamparray::device_key(&info),
            vid: info.vid,
            pid: info.pid,
            endpoints: Vec::new(),
            smbus: None,
            lamparray: Some(LampArrayTarget { backend, info }),
            network: None,
            serial: None,
        }
    }
}

/// The grid size and one cell per lamp (in lamp order) for a LampArray.
pub fn geometry(info: &LampArrayInfo) -> ([u32; 2], Vec<[u32; 2]>) {
    let [w, h, _] = info.bounding_box_um;
    let longer = w.max(h).max(1);
    let cell = f64::from(longer) / f64::from(GRID_CELLS);
    let side = |v: u32| ((f64::from(v) / cell).ceil() as u32).clamp(1, GRID_CELLS);
    let size = [side(w), side(h)];
    let at = |v: i32, n: u32| ((f64::from(v.max(0)) / cell).floor() as u32).min(n - 1);
    let leds = info
        .lamps
        .iter()
        .map(|l| [at(l.position_um[0], size[0]), at(l.position_um[1], size[1])])
        .collect();
    (size, leds)
}

/// The Lightloom device type of a LampArray kind.
pub fn device_type(kind: LampArrayKind) -> DeviceType {
    match kind {
        LampArrayKind::Keyboard => DeviceType::Keyboard,
        LampArrayKind::Mouse => DeviceType::Mouse,
        LampArrayKind::Headset => DeviceType::Headphones,
        LampArrayKind::Microphone => DeviceType::Microphone,
        LampArrayKind::Speaker => DeviceType::Speakers,
        LampArrayKind::Chassis => DeviceType::Case,
        _ => DeviceType::Other,
    }
}

/// The catalogue entry standing in for the plugin of a LampArray.
pub fn builtin_entry(info: &LampArrayInfo) -> CatalogEntry {
    let mut meta = PluginMeta::minimal(BUILTIN_PATH, BUILTIN_NAME, info.vid, vec![info.pid]);
    let (size, leds) = geometry(info);
    meta.publisher = Some("Lightloom".into());
    meta.device_type = device_type(info.kind);
    meta.size = size;
    meta.led_names = (0..leds.len()).map(|i| format!("Lamp {i}")).collect();
    meta.led_positions = leds;
    CatalogEntry {
        meta,
        root: std::path::PathBuf::new(),
    }
}

fn failure(class: &str, message: impl Into<String>) -> JsFailure {
    JsFailure {
        class: class.into(),
        message: message.into(),
    }
}

/// Apply one [`Control`] between two frames (no plugin hooks to call).
fn apply_control(
    shared: &SharedDevice,
    key: &str,
    control: Control,
    shutdown_mode: &mut ShutdownMode,
    report: &mut SessionReport,
) {
    let mut dev = lock_device(shared);
    match control {
        Control::ShutdownMode(m) => *shutdown_mode = m,
        // A LampArray has no device memory Lightloom could write.
        Control::Save(call) => {
            let _ = call.reply.send(SaveOutcome::failed(
                key,
                call.action,
                "a Windows Dynamic Lighting device cannot store lighting",
            ));
        }
        Control::Parameters(_) => report
            .notes
            .push("the built-in LampArray driver has no parameters".into()),
        Control::Brightness(b) => dev.set_brightness(b),
        Control::Alert(color) => dev.set_alert(color),
        Control::BatteryWarningLevel(level) => dev.set_battery_warning_level(level),
        Control::ChildPlacement { uid, .. } => report.notes.push(format!("no endpoint {uid} to place")),
        Control::Placement(p) => dev.set_placement(p),
    }
}

/// The session of a LampArray device (see the module docs).
pub(crate) fn run(
    device: SessionDevice,
    plugin: CatalogEntry,
    frames: Arc<FrameStore>,
    opts: SessionOptions,
    stop: &Arc<AtomicBool>,
    live: &LiveStats,
    controls: &Receiver<Control>,
) -> SessionReport {
    let started = Instant::now();
    let mut report = SessionReport::new(&device, &plugin);
    let Some(target) = device.lamparray.clone() else {
        report.faulted = Some("not a LampArray device".into());
        return report;
    };
    let info = &target.info;
    let meta = &plugin.meta;
    report.endpoint_path = Some(info.id.clone());

    let mut ctx = DeviceContext::new(Arc::new(NoHid), Vec::new(), opts.write_mode);
    ctx.set_name(info.name.clone());
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
    ctx.set_device_info([String::new(), info.name.clone(), info.id.clone()]);
    if let Err(e) = ctx.set_geometry(meta.size, meta.led_positions.clone()) {
        report.faulted = Some(format!("LampArray layout rejected: {e}"));
        return report;
    }
    let shared = ctx.shared();

    let mut handle = match target.backend.open(&info.id) {
        Ok(h) => h,
        Err(e) => {
            report.faulted = Some(format!("cannot open the LampArray {}: {e}", info.id));
            return report;
        }
    };
    report.initialized = true;
    live.set_state(SessionState::Running);
    live.publish_endpoints((lock_device(&shared).endpoint_views(&device.key), Vec::new()));

    let min_interval = Duration::from_micros(u64::from(info.min_update_interval_us));
    let leds = meta.led_positions.clone();
    let mut colors: Vec<[u8; 4]> = vec![[0, 0, 0, 255]; leds.len()];
    let mut sent: Option<(Vec<[u8; 4]>, Instant)> = None;
    let mut last_write: Option<Instant> = None;
    let mut was_available: Option<bool> = None;
    let mut write_latency = LatencyHistogram::new();
    let mut render = LatencyHistogram::new();
    let mut last_seq = 0;
    let mut write_errors = 0u64;
    let mut shutdown_mode = opts.shutdown_mode;
    while !stop.load(Ordering::Acquire) {
        let Some(seq) = frames.wait_for_newer(last_seq, Duration::from_millis(100)) else {
            continue;
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        last_seq = seq;
        let mut controlled = false;
        while let Ok(control) = controls.try_recv() {
            apply_control(&shared, &device.key, control, &mut shutdown_mode, &mut report);
            controlled = true;
        }
        if controlled || report.frames.is_multiple_of(30) {
            live.publish_endpoints((lock_device(&shared).endpoint_views(&device.key), Vec::new()));
        }

        // Windows gives the device to one app at a time.
        let available = handle.available();
        if was_available != Some(available) {
            was_available = Some(available);
            live.paused.store(!available, Ordering::Relaxed);
            lock_device(&shared).log(if available {
                "Windows gives Lightloom control of this Dynamic Lighting device"
            } else {
                "paused: Windows gives this Dynamic Lighting device to another app \
                 (Settings > Personalization > Dynamic Lighting)"
            });
            // Back in control: the next write is a full frame.
            sent = None;
        }
        if !available {
            continue;
        }
        if last_write.is_some_and(|t| t.elapsed() < min_interval) {
            // Too early for the device: the next frame carries the latest colours.
            continue;
        }

        let t = Instant::now();
        let (_, frame, captured) = frames.latest_stamped();
        {
            let mut dev = lock_device(&shared);
            dev.set_frame_at(frame, captured);
            if let Some(direct) = &opts.direct {
                dev.apply_direct(direct, &device.key);
            }
            for (c, p) in colors.iter_mut().zip(&leds) {
                let [r, g, b] = dev.color(p[0] as i32, p[1] as i32);
                *c = [r, g, b, 255];
            }
        }
        report.frames += 1;
        live.frames.fetch_add(1, Ordering::Relaxed);
        if sent
            .as_ref()
            .is_some_and(|(c, at)| *c == colors && at.elapsed() < KEEPALIVE)
        {
            render.record(t.elapsed());
            continue;
        }
        let w = Instant::now();
        let result = match opts.write_mode {
            WriteMode::DryRun => Ok(()),
            WriteMode::Live => handle.set_colors(&colors),
        };
        last_write = Some(Instant::now());
        match result {
            Ok(()) => {
                write_latency.record(w.elapsed());
                sent = Some((colors.clone(), Instant::now()));
                live.hid_disconnected.store(false, Ordering::Relaxed);
                if report.first_write_ms.is_none() {
                    let d = started.elapsed();
                    report.first_write_ms = Some(d.as_secs_f64() * 1000.0);
                    live.first_write_us
                        .store((d.as_micros() as u64).max(1), Ordering::Relaxed);
                }
            }
            Err(e) => {
                write_errors += 1;
                live.io_errors.store(write_errors, Ordering::Relaxed);
                if matches!(e, HalError::Disconnected) {
                    live.hid_disconnects.fetch_add(1, Ordering::Relaxed);
                    live.hid_disconnected.store(true, Ordering::Relaxed);
                }
                if write_errors <= 5 {
                    report.fail("Render", failure("IoError", format!("SetColorsForIndices: {e}")));
                }
            }
        }
        render.record(t.elapsed());
    }

    live.set_state(SessionState::Stopping);
    while let Ok(control) = controls.try_recv() {
        apply_control(&shared, &device.key, control, &mut shutdown_mode, &mut report);
    }
    report.shutdown_mode = shutdown_mode;
    if !opts.skip_shutdown && opts.write_mode == WriteMode::Live {
        // `Off`: black before handing the device back. `Keep` cannot be honoured:
        // once released, Windows gives the device to the next controller.
        if shutdown_mode == ShutdownMode::Off && handle.available() {
            if let Err(e) = handle.set_colors(&vec![[0, 0, 0, 255]; leds.len()]) {
                report.fail("Shutdown", failure("IoError", e.to_string()));
            }
        }
        match handle.release() {
            Ok(()) => report.shutdown_called = true,
            Err(e) => report.fail("Shutdown", failure("IoError", e.to_string())),
        }
    } else if !opts.skip_shutdown {
        report.shutdown_called = true;
    }
    drop(handle);
    report.io_errors = write_errors;
    report.render_errors = 0;
    report.render = render.summary();
    report.write = write_latency.summary();
    let dev = lock_device(&shared);
    let (tail, lines) = dev.log_tail();
    report.log_tail = tail;
    report.log_lines = lines;
    report.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::lamparray::LampInfo;

    fn info(bbox: [u32; 3], lamps: &[[i32; 2]]) -> LampArrayInfo {
        LampArrayInfo {
            id: "x".into(),
            name: "Fake keyboard".into(),
            vid: 1,
            pid: 2,
            container_id: None,
            kind: LampArrayKind::Keyboard,
            bounding_box_um: bbox,
            min_update_interval_us: 0,
            lamps: lamps
                .iter()
                .enumerate()
                .map(|(i, p)| LampInfo {
                    index: i as u32,
                    position_um: [p[0], p[1], 0],
                    is_rgb: true,
                })
                .collect(),
        }
    }

    #[test]
    fn geometry_follows_the_bounding_box() {
        // 320 mm x 100 mm keyboard: 10 mm cells, 32 x 10.
        let i = info(
            [320_000, 100_000, 30_000],
            &[[0, 0], [319_999, 99_999], [160_000, 50_000], [-5, 999_999]],
        );
        let (size, leds) = geometry(&i);
        assert_eq!(size, [32, 10]);
        assert_eq!(leds, vec![[0, 0], [31, 9], [16, 5], [0, 9]]);
        // A degenerate box still gives a usable grid.
        let (size, leds) = geometry(&info([0, 0, 0], &[[0, 0]]));
        assert_eq!((size, leds), ([1, 1], vec![[0, 0]]));
    }

    #[test]
    fn builtin_entry_describes_the_device() {
        let e = builtin_entry(&info([20_000, 40_000, 1], &[[1_000, 39_000]]));
        assert_eq!(e.meta.name, BUILTIN_NAME);
        assert_eq!(e.meta.device_type, DeviceType::Keyboard);
        assert_eq!(e.meta.size, [16, 32]);
        assert_eq!(e.meta.led_positions, vec![[0, 31]]);
        assert_eq!(e.meta.led_names, vec!["Lamp 0".to_string()]);
        assert_eq!(e.meta.source_path.to_string_lossy(), BUILTIN_PATH);
    }
}
