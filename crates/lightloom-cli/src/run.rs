// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `lightloom light | rainbow | bench`: the headless M1 pipeline.
//!
//! Null renderer → [`FrameStore`] → one plugin session per supported device
//! (the same catalog → session → device-host path the daemon will use).
//! Nothing is written to hardware while conflicting RGB software runs (it is
//! reported, never terminated), and every session ends with the plugin's
//! `Shutdown(false)` — on Ctrl-C too — unless `--keep` is given.

use anyhow::{bail, Context};
use clap::Args;
use lightloom_engine::{FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::MockHid;
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry};
use lightloom_plugin_host::device::{Brightness, IoKind, LogSink, WriteMode};
use lightloom_plugin_host::session::{
    self, RestoreCheckMode, RestoreProbe, SessionDevice, SessionOptions, SessionReport,
};
use lightloom_render::{NullRenderer, Pattern, RendererClient};
use std::path::PathBuf;
#[cfg(not(windows))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[cfg(not(windows))]
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// RGB software that holds the devices Lightloom drives. Reported, never terminated.
pub const KNOWN_CONFLICTS: &[&str] = &["SignalRgb.exe", "OpenRGB.exe"];

/// Options shared by `light`, `rainbow` and `bench`.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// Only drive devices whose key starts with this (`vid:pid` or `vid:pid:serial`, hex; repeatable).
    #[arg(long = "device", value_name = "UID")]
    pub devices: Vec<String>,
    /// Plugin directory (repeatable; later ones win on VID:PID collisions).
    /// Default: $LIGHTLOOM_PLUGINS, else ./plugins.
    #[arg(long = "plugins", value_name = "DIR")]
    pub plugin_dirs: Vec<PathBuf>,
    /// Read devices but never write; print the packets that would be sent.
    #[arg(long)]
    pub dry_run: bool,
    /// Stop after this many seconds (default: until Ctrl-C; `bench` uses --seconds).
    #[arg(long, value_name = "SECS", value_parser = parse_secs)]
    pub duration: Option<f64>,
    /// Exit without calling the plugin's Shutdown(): the device keeps the last colour.
    #[arg(long)]
    pub keep: bool,
    /// Run even when conflicting RGB software (SignalRGB, OpenRGB, the plugin's list) is running.
    #[arg(long)]
    pub ignore_conflicts: bool,
    /// Compare feature reports before Initialize and after Shutdown. Alone: every
    /// feature report the device's report descriptor declares. `=ID:LEN`: that
    /// report only (repeatable, e.g. --restore-check=0x52:185).
    #[arg(
        long,
        value_name = "ID:LEN",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "auto",
        value_parser = parse_restore
    )]
    pub restore_check: Vec<RestoreArg>,
    /// Plugin parameter override, NAME=VALUE (VALUE parsed as JSON, else taken as a string; repeatable).
    #[arg(long = "param", value_name = "NAME=VALUE", value_parser = parse_param)]
    pub params: Vec<(String, serde_json::Value)>,
    /// Global brightness 0-100.
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub brightness: u8,
    /// Print live counters every N seconds (0 = never).
    #[arg(long, value_name = "SECS", default_value_t = 10.0, value_parser = parse_stats_secs)]
    pub stats_every: f64,
    /// Write the full run report as JSON.
    #[arg(long, value_name = "FILE")]
    pub json: Option<PathBuf>,
    /// Print every plugin log line.
    #[arg(short, long)]
    pub verbose: bool,
}

/// What to draw and how long to run.
#[derive(Clone, Debug)]
pub enum Mode {
    /// One colour until stopped.
    Light(Rgb),
    /// Scrolling hue sweep.
    Rainbow {
        /// Frame rate.
        fps: f64,
    },
    /// Rainbow for a fixed time, then latency histograms.
    Bench {
        /// Frame rate.
        fps: f64,
        /// Run time.
        seconds: f64,
        /// Drive a scripted in-memory device with this VID:PID instead of hardware.
        mock: Option<(u16, u16)>,
    },
}

impl Mode {
    fn name(&self) -> &'static str {
        match self {
            Mode::Light(_) => "light",
            Mode::Rainbow { .. } => "rainbow",
            Mode::Bench { .. } => "bench",
        }
    }

    fn pattern(&self) -> Pattern {
        match self {
            Mode::Light(c) => Pattern::Solid(*c),
            _ => Pattern::Rainbow,
        }
    }

    fn fps(&self) -> f64 {
        match self {
            Mode::Light(_) => 30.0,
            Mode::Rainbow { fps } | Mode::Bench { fps, .. } => *fps,
        }
    }
}

/// `#RRGGBB`, `RRGGBB` or a basic colour name.
pub fn parse_color(s: &str) -> Result<Rgb, String> {
    const NAMES: &[(&str, Rgb)] = &[
        ("black", Rgb::new(0, 0, 0)),
        ("white", Rgb::new(255, 255, 255)),
        ("red", Rgb::new(255, 0, 0)),
        ("green", Rgb::new(0, 255, 0)),
        ("blue", Rgb::new(0, 0, 255)),
        ("yellow", Rgb::new(255, 255, 0)),
        ("cyan", Rgb::new(0, 255, 255)),
        ("magenta", Rgb::new(255, 0, 255)),
        ("orange", Rgb::new(255, 128, 0)),
        ("purple", Rgb::new(128, 0, 255)),
    ];
    let t = s.trim();
    if let Some((_, c)) = NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(t)) {
        return Ok(*c);
    }
    Rgb::from_hex(t).ok_or_else(|| format!("'{s}' is neither #RRGGBB nor a known colour name"))
}

/// Longest `--duration` / `--seconds` accepted (one year).
pub const MAX_RUN_SECS: f64 = 31_536_000.0;

/// A run time in seconds: finite, above 0, at most [`MAX_RUN_SECS`].
pub fn parse_secs(s: &str) -> Result<f64, String> {
    let v: f64 = s.trim().parse().map_err(|e| format!("'{s}': {e}"))?;
    if v.is_finite() && v > 0.0 && v <= MAX_RUN_SECS {
        Ok(v)
    } else {
        Err(format!(
            "'{s}' must be a number of seconds above 0 and at most {MAX_RUN_SECS}"
        ))
    }
}

/// A statistics interval in seconds: 0 (never) up to one day.
pub fn parse_stats_secs(s: &str) -> Result<f64, String> {
    let v: f64 = s.trim().parse().map_err(|e| format!("'{s}': {e}"))?;
    if v.is_finite() && (0.0..=86_400.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("'{s}' must be between 0 and 86400 seconds"))
    }
}

/// A frame rate the frame clock supports (1-240 fps).
pub fn parse_fps(s: &str) -> Result<f64, String> {
    let v: f64 = s.trim().parse().map_err(|e| format!("'{s}': {e}"))?;
    let (min, max) = (lightloom_render::clock::MIN_FPS, lightloom_render::clock::MAX_FPS);
    if v.is_finite() && (min..=max).contains(&v) {
        Ok(v)
    } else {
        Err(format!("'{s}' must be between {min} and {max} fps"))
    }
}

fn parse_u(s: &str) -> Result<u64, String> {
    let t = s.trim();
    let r = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16),
        None => t.parse(),
    };
    r.map_err(|e| format!("'{s}': {e}"))
}

/// One `--restore-check` occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreArg {
    /// Every declared feature report.
    Auto,
    /// One report.
    Report(RestoreProbe),
}

fn parse_restore(s: &str) -> Result<RestoreArg, String> {
    if s.eq_ignore_ascii_case("auto") {
        return Ok(RestoreArg::Auto);
    }
    parse_probe(s).map(RestoreArg::Report)
}

/// The session's mode for the given `--restore-check` occurrences.
pub fn restore_mode(args: &[RestoreArg]) -> RestoreCheckMode {
    if args.contains(&RestoreArg::Auto) {
        return RestoreCheckMode::DeclaredFeatureReports;
    }
    let reports: Vec<RestoreProbe> = args
        .iter()
        .filter_map(|a| match a {
            RestoreArg::Report(p) => Some(*p),
            RestoreArg::Auto => None,
        })
        .collect();
    if reports.is_empty() {
        RestoreCheckMode::Off
    } else {
        RestoreCheckMode::Reports(reports)
    }
}

fn parse_probe(s: &str) -> Result<RestoreProbe, String> {
    let (id, len) = s.split_once(':').ok_or("expected ID:LEN, e.g. 0x52:185")?;
    let report_id = u8::try_from(parse_u(id)?).map_err(|_| format!("report id '{id}' > 255"))?;
    let len = usize::try_from(parse_u(len)?).map_err(|e| e.to_string())?;
    if !(1..=65536).contains(&len) {
        return Err(format!("length {len} outside 1..=65536"));
    }
    Ok(RestoreProbe { report_id, len })
}

fn parse_param(s: &str) -> Result<(String, serde_json::Value), String> {
    let (name, value) = s.split_once('=').ok_or("expected NAME=VALUE")?;
    if name.trim().is_empty() {
        return Err("empty parameter name".into());
    }
    let v = serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
    Ok((name.trim().to_string(), v))
}

/// `VID:PID` in hex.
pub fn parse_vid_pid(s: &str) -> Result<(u16, u16), String> {
    let (v, p) = s.split_once(':').ok_or("expected VID:PID (hex)")?;
    let hex = |x: &str| {
        u16::from_str_radix(x.trim().trim_start_matches("0x"), 16).map_err(|e| format!("'{x}': {e}"))
    };
    Ok((hex(v)?, hex(p)?))
}

fn default_plugin_dirs() -> Vec<PathBuf> {
    if let Some(dirs) = std::env::var_os("LIGHTLOOM_PLUGINS") {
        return std::env::split_paths(&dirs).collect();
    }
    vec![PathBuf::from("plugins")]
}

/// Process-wide numbers of one run.
#[derive(Debug, serde::Serialize)]
pub struct RunReport {
    /// `light`, `rainbow` or `bench`.
    pub mode: &'static str,
    /// Frames per second the renderer targeted.
    pub fps: f64,
    /// Writes suppressed.
    pub dry_run: bool,
    /// Wall time from start to the end of the last session.
    pub elapsed_s: f64,
    /// Command start to the first write of any session.
    pub first_write_after_start_ms: Option<f64>,
    /// Peak working set sampled once per second (Windows).
    pub max_rss_bytes: Option<u64>,
    /// `--keep`: Shutdown() was skipped on purpose.
    pub keep: bool,
    /// One report per session that returned.
    pub sessions: Vec<SessionReport>,
    /// Sessions that panicked or did not stop in time (`key: reason`).
    pub failed_sessions: Vec<String>,
}

impl RunReport {
    /// Every session clean (no lifecycle failure, fault or HID error).
    pub fn is_clean(&self) -> bool {
        !self.sessions.is_empty()
            && self.failed_sessions.is_empty()
            && self.sessions.iter().all(SessionReport::is_clean)
    }
}

/// How long stopped sessions get to run Shutdown() before the run gives up on them.
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// Stop every session and collect the reports of those that end within
/// [`STOP_TIMEOUT`]. Sessions that panicked or are stuck (in device I/O)
/// come back as `key: reason`; stuck threads are left behind and end with
/// the process.
fn stop_sessions(handles: Vec<session::SessionHandle>) -> (Vec<SessionReport>, Vec<String>) {
    for h in &handles {
        h.stop();
    }
    let deadline = Instant::now() + STOP_TIMEOUT;
    while handles.iter().any(|h| !h.is_finished()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut reports = Vec::new();
    let mut failed = Vec::new();
    for h in handles {
        let key = h.key().to_string();
        if !h.is_finished() {
            failed.push(format!(
                "{key}: did not stop within {} s (stuck in device I/O?)",
                STOP_TIMEOUT.as_secs()
            ));
            continue;
        }
        match h.join() {
            Ok(r) => reports.push(r),
            Err(panic) => failed.push(format!("{key}: panicked: {panic}")),
        }
    }
    (reports, failed)
}

/// A device paired with the plugin that drives it.
struct Target {
    device: SessionDevice,
    plugin: CatalogEntry,
}

fn matches_filter(key: &str, filters: &[String]) -> bool {
    filters.is_empty()
        || filters.iter().any(|f| {
            key.to_ascii_lowercase()
                .starts_with(&f.trim().to_ascii_lowercase())
        })
}

/// A scripted device that answers every feature-report id with 1024 zero bytes
/// (report id first), so any HID plugin can run its Initialize against it.
fn permissive_mock(vid: u16, pid: u16) -> anyhow::Result<(Arc<MockHid>, SessionDevice)> {
    let info = MockHid::endpoint_info(
        vid,
        pid,
        "MOCK",
        0,
        0x0001,
        0x0000,
        0,
        ReportLengths {
            input: 64,
            output: 64,
            feature: 1024,
        },
    );
    let mock = MockHid::new().add_endpoint(info.clone());
    for id in 0..=255u8 {
        let mut report = vec![0u8; 1024];
        report[0] = id;
        mock.set_feature_report(&info.path, report)?;
    }
    let device = SessionDevice {
        key: format!("{vid:04x}:{pid:04x}:MOCK"),
        vid,
        pid,
        endpoints: vec![info],
    };
    Ok((Arc::new(mock), device))
}

#[cfg(windows)]
fn hardware_targets(
    catalog: &Catalog,
    filters: &[String],
) -> anyhow::Result<(Arc<dyn HidBackend>, Vec<Target>, usize)> {
    // Enumerate and probe only the devices some plugin claims.
    let wanted: std::collections::BTreeSet<(u16, u16)> =
        catalog.entries().iter().flat_map(|e| e.meta.keys()).collect();
    let (backend, groups): (Arc<dyn HidBackend>, _) = if crate::devices::use_hidapi() {
        let b = lightloom_hal_windows::WindowsHidBackend::new().context("initialising hidapi")?;
        let groups = b
            .enumerate_grouped_for(&wanted)
            .context("enumerating HID devices")?;
        (Arc::new(b), groups)
    } else {
        let b = lightloom_hal_windows::NativeHidBackend::new();
        let groups = b
            .enumerate_grouped_for(&wanted)
            .context("enumerating HID devices")?;
        (Arc::new(b), groups)
    };
    let total = groups.len();
    let mut targets = Vec::new();
    for g in groups {
        let key = g.key.to_string();
        if !matches_filter(&key, filters) {
            continue;
        }
        if let Some(plugin) = catalog.lookup(g.key.vid, g.key.pid) {
            targets.push(Target {
                device: SessionDevice {
                    key,
                    vid: g.key.vid,
                    pid: g.key.pid,
                    endpoints: g.endpoints.into_iter().map(|e| e.info).collect(),
                },
                plugin: plugin.clone(),
            });
        }
    }
    Ok((backend, targets, total))
}

#[cfg(not(windows))]
fn hardware_targets(_: &Catalog, _: &[String]) -> anyhow::Result<(Arc<dyn HidBackend>, Vec<Target>, usize)> {
    bail!("driving hardware is only implemented on Windows so far (Linux backend: milestone M7); try `lightloom bench --mock VID:PID`")
}

#[cfg(windows)]
fn running_conflicts(targets: &[Target]) -> anyhow::Result<Vec<String>> {
    let running = lightloom_hal_windows::system::running_process_names().map_err(anyhow::Error::msg)?;
    let mut wanted: Vec<String> = KNOWN_CONFLICTS.iter().map(|s| s.to_string()).collect();
    for t in targets {
        wanted.extend(t.plugin.meta.conflicting_processes.iter().cloned());
    }
    let mut found: Vec<String> = running
        .into_iter()
        .filter(|r| wanted.iter().any(|w| w.eq_ignore_ascii_case(r)))
        .collect();
    found.sort_by_key(|s| s.to_ascii_lowercase());
    found.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(found)
}

#[cfg(not(windows))]
fn running_conflicts(_: &[Target]) -> anyhow::Result<Vec<String>> {
    Ok(Vec::new())
}

/// Stop requests from the console: Ctrl-C and Ctrl-Break everywhere; on
/// Windows also window close, log-off and shutdown, which are held (up to
/// ~4.5 s) until [`Interrupt::finished`] so plugins can restore devices.
struct Interrupt {
    #[cfg(windows)]
    console: Option<lightloom_hal_windows::console::ConsoleStop>,
    #[cfg(not(windows))]
    flag: Arc<AtomicBool>,
}

impl Interrupt {
    #[cfg(windows)]
    fn install() -> Self {
        let console = lightloom_hal_windows::console::ConsoleStop::install()
            .map_err(|e| eprintln!("lightloom: console handler not installed ({e}); stop with --duration"))
            .ok();
        Self { console }
    }

    #[cfg(not(windows))]
    fn install() -> Self {
        static FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
        let flag = FLAG.get_or_init(|| {
            let flag = Arc::new(AtomicBool::new(false));
            let handler_flag = Arc::clone(&flag);
            if let Err(e) = ctrlc::set_handler(move || handler_flag.store(true, Ordering::Release)) {
                eprintln!("lightloom: Ctrl-C handler not installed ({e}); stop with --duration");
            }
            flag
        });
        flag.store(false, Ordering::Release);
        Self {
            flag: Arc::clone(flag),
        }
    }

    fn requested(&self) -> bool {
        #[cfg(windows)]
        {
            self.console.is_some_and(|c| c.requested())
        }
        #[cfg(not(windows))]
        {
            self.flag.load(Ordering::Acquire)
        }
    }

    /// Every session has shut down: a held close/log-off/shutdown may proceed.
    fn finished(&self) {
        #[cfg(windows)]
        if let Some(c) = self.console {
            c.finished();
        }
    }
}

fn rss_bytes() -> Option<u64> {
    #[cfg(windows)]
    {
        lightloom_hal_windows::system::working_set_bytes()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Exit status of a finished run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every session clean.
    Clean,
    /// A session failed, faulted or saw HID errors.
    Failed,
}

/// Run `mode` with `args`. Errors are setup problems (no plugin, no device, conflicts).
pub fn run(mode: Mode, args: &RunArgs) -> anyhow::Result<Outcome> {
    let started = Instant::now();
    let dirs = if args.plugin_dirs.is_empty() {
        default_plugin_dirs()
    } else {
        args.plugin_dirs.clone()
    };
    let catalog = Catalog::scan(&dirs, Duration::from_secs(10));
    if catalog.entries().is_empty() {
        bail!(
            "no device plugin found in {} (use --plugins DIR or LIGHTLOOM_PLUGINS)",
            dirs.iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let mock = matches!(mode, Mode::Bench { mock: Some(_), .. });
    let (backend, targets) = match mode {
        Mode::Bench {
            mock: Some((vid, pid)),
            ..
        } => {
            let plugin = catalog
                .lookup(vid, pid)
                .with_context(|| format!("no plugin claims {vid:04x}:{pid:04x}"))?
                .clone();
            let (mock, device) = permissive_mock(vid, pid)?;
            let backend: Arc<dyn HidBackend> = mock;
            (backend, vec![Target { device, plugin }])
        }
        _ => {
            let (backend, targets, total) = hardware_targets(&catalog, &args.devices)?;
            if targets.is_empty() {
                bail!(
                    "no supported device found: {total} device(s) match the {} loaded plugin(s){}",
                    catalog.entries().len(),
                    if args.devices.is_empty() {
                        ""
                    } else {
                        " (after --device filters)"
                    }
                );
            }
            (backend, targets)
        }
    };

    if !mock {
        let conflicts = running_conflicts(&targets)?;
        if !conflicts.is_empty() && !args.ignore_conflicts {
            bail!(
                "conflicting RGB software is running: {}. Close it first (Lightloom never terminates other \
                 applications), or pass --ignore-conflicts",
                conflicts.join(", ")
            );
        }
    }

    for t in &targets {
        eprintln!(
            "lightloom {}: {} -> {} ({}){}",
            mode.name(),
            t.device.key,
            t.plugin.meta.name,
            t.plugin.meta.source_path.display(),
            if args.dry_run { " [dry-run]" } else { "" }
        );
    }

    let interrupt = Interrupt::install();

    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let mut renderer = NullRenderer::new(mode.pattern(), mode.fps());
    renderer.start(Arc::clone(&store))?;

    let verbose = args.verbose;
    let mut handles = Vec::new();
    for t in targets {
        let label = t.device.key.clone();
        let sink: LogSink = Arc::new(move |line: &str| {
            if verbose {
                eprintln!("  [{label}] {line}");
            }
        });
        let opts = SessionOptions {
            parameters: args.params.iter().cloned().collect(),
            brightness: Brightness {
                global: args.brightness,
                layout: 100,
            },
            write_mode: if args.dry_run {
                WriteMode::DryRun
            } else {
                WriteMode::Live
            },
            skip_shutdown: args.keep,
            restore_check: restore_mode(&args.restore_check),
            log_sink: Some(sink),
            ..SessionOptions::default()
        };
        match session::spawn(t.device, t.plugin, Arc::clone(&backend), Arc::clone(&store), opts) {
            Ok(h) => handles.push((h, Instant::now())),
            Err(e) => {
                // Do not leave the sessions already started running detached.
                let (_, failed) = stop_sessions(handles.into_iter().map(|(h, _)| h).collect());
                renderer.stop();
                interrupt.finished();
                return Err(anyhow::Error::from(e).context(format!(
                    "starting a device session ({} already started were stopped{})",
                    failed.len(),
                    if failed.is_empty() { "" } else { ", some uncleanly" }
                )));
            }
        }
    }

    let limit = match mode {
        Mode::Bench { seconds, .. } => Some(seconds),
        _ => args.duration,
    }
    .filter(|s| s.is_finite() && *s > 0.0)
    .map(Duration::from_secs_f64);
    let stats_every = (args.stats_every.is_finite() && args.stats_every > 0.0)
        .then(|| Duration::from_secs_f64(args.stats_every));
    let mut max_rss = rss_bytes();
    let mut first_write: Option<f64> = None;
    let mut last_stats = Instant::now();
    let mut last_rss = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let first = handles
            .iter()
            .filter_map(|(h, spawned)| {
                h.live()
                    .first_write_ms
                    .map(|ms| spawned.duration_since(started).as_secs_f64() * 1000.0 + ms)
            })
            .reduce(f64::min);
        if let (None, Some(ms)) = (first_write, first) {
            first_write = Some(ms);
            eprintln!(
                "lightloom {}: first {} after {ms:.1} ms (command start)",
                mode.name(),
                if args.dry_run { "dry-run write" } else { "write" }
            );
        }
        if last_rss.elapsed() >= Duration::from_secs(1) {
            last_rss = Instant::now();
            max_rss = max_rss.max(rss_bytes());
        }
        if let Some(every) = stats_every {
            if last_stats.elapsed() >= every {
                last_stats = Instant::now();
                for (h, _) in &handles {
                    let l = h.live();
                    eprintln!(
                        "  {:>7.1}s {}: {:?} frames={} render_errors={} hid_errors={} rss={}",
                        started.elapsed().as_secs_f64(),
                        h.key(),
                        l.state,
                        l.frames,
                        l.render_errors,
                        l.hid_errors,
                        rss_bytes()
                            .map(|b| format!("{:.1} MB", b as f64 / 1e6))
                            .unwrap_or_else(|| "n/a".into())
                    );
                }
            }
        }
        let interrupted = interrupt.requested();
        let expired = limit.is_some_and(|l| started.elapsed() >= l);
        let all_done = handles.iter().all(|(h, _)| h.is_finished());
        if interrupted || expired || all_done {
            if interrupted {
                eprintln!("lightloom {}: interrupted, shutting down", mode.name());
            }
            break;
        }
    }
    let (sessions, failed_sessions) = stop_sessions(handles.into_iter().map(|(h, _)| h).collect());
    renderer.stop();
    interrupt.finished();
    max_rss = max_rss.max(rss_bytes());
    for f in &failed_sessions {
        eprintln!("lightloom: session {f}");
    }

    let report = RunReport {
        mode: mode.name(),
        fps: mode.fps(),
        dry_run: args.dry_run,
        elapsed_s: started.elapsed().as_secs_f64(),
        first_write_after_start_ms: first_write,
        max_rss_bytes: max_rss,
        keep: args.keep,
        sessions,
        failed_sessions,
    };
    print_report(&report);
    if let Some(path) = &args.json {
        std::fs::write(path, serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(if report.is_clean() {
        Outcome::Clean
    } else {
        Outcome::Failed
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// `[a..=b]` ranges of the offsets where `a` and `b` differ.
fn diff_ranges(a: &[u8], b: &[u8]) -> String {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let n = a.len().max(b.len());
    for i in 0..=n {
        let differs = i < n && a.get(i) != b.get(i);
        match (differs, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push(if s == i - 1 {
                    format!("0x{s:02X}")
                } else {
                    format!("0x{s:02X}..0x{:02X}", i - 1)
                });
                start = None;
            }
            _ => {}
        }
    }
    if out.is_empty() {
        "none".into()
    } else {
        out.join(", ")
    }
}

fn print_report(r: &RunReport) {
    println!(
        "{} run: {:.1} s{}{}",
        r.mode,
        r.elapsed_s,
        r.max_rss_bytes
            .map(|b| format!(", peak RSS {:.1} MB", b as f64 / 1e6))
            .unwrap_or_default(),
        r.first_write_after_start_ms
            .map(|ms| format!(", first write {ms:.1} ms after start"))
            .unwrap_or_default()
    );
    for s in &r.sessions {
        println!("- {} ({})", s.device, s.name);
        if let (Some(ep), Some(path)) = (&s.endpoint, &s.endpoint_path) {
            println!(
                "  endpoint: interface {} usage_page 0x{:04X} usage 0x{:04X} collection {} ({} of {} validated)  {}",
                ep.interface, ep.usage_page, ep.usage, ep.collection, s.validated, s.endpoints, path
            );
        }
        println!(
            "  frames {}  render p50 {:.2} ms p99 {:.2} ms max {:.2} ms  render errors {}",
            s.frames, s.render.p50_ms, s.render.p99_ms, s.render.max_ms, s.render_errors
        );
        println!(
            "  HID: {} writes, {} feature sends, {} feature reads, {} errors; write p50 {:.2} ms p99 {:.2} ms{}",
            s.io.writes,
            s.io.feature_sends,
            s.io.feature_gets,
            s.io.errors(),
            s.write.p50_ms,
            s.write.p99_ms,
            s.io.last_error.as_ref().map(|e| format!("; last error: {e}")).unwrap_or_default()
        );
        if let Some(ms) = s.first_write_ms {
            println!("  first write {ms:.1} ms after session start");
        }
        if r.dry_run {
            let read = s.io_log.iter().find(|x| x.kind == IoKind::GetFeature);
            let writes: Vec<_> = s.io_log.iter().filter(|x| x.dry_run).collect();
            if let Some(read) = read {
                println!("  read  {} bytes: {}", read.bytes.len(), hex(&read.bytes));
            }
            for (i, w) in writes.iter().take(3).enumerate() {
                println!(
                    "  would send #{} ({:?}, {} bytes): {}",
                    i + 1,
                    w.kind,
                    w.bytes.len(),
                    hex(&w.bytes)
                );
                if let Some(read) = read {
                    if read.bytes.first() == w.bytes.first() {
                        println!(
                            "    differs from the read report at: {}",
                            diff_ranges(&read.bytes, &w.bytes)
                        );
                    }
                }
            }
            if writes.len() > 3 {
                println!("  ... {} more recorded writes", writes.len() - 3);
            }
        }
        for note in &s.restore_notes {
            println!("  restore check: {note}");
        }
        for c in &s.restore_checks {
            if c.after.is_empty() {
                println!(
                    "  restore check 0x{:02X}: the report could not be read back",
                    c.report_id
                );
            } else if c.identical {
                println!(
                    "  restore check 0x{:02X}: identical before and after",
                    c.report_id
                );
            } else {
                println!(
                    "  restore check 0x{:02X}: differs at {}",
                    c.report_id,
                    diff_ranges(&c.before, &c.after)
                );
            }
        }
        if !s.shutdown_called {
            if r.keep {
                println!("  Shutdown() skipped (--keep): the device keeps the last colour");
            } else if s.initialized && !s.failures.iter().any(|f| f.phase == "Shutdown") {
                println!("  the plugin exports no Shutdown()");
            }
        }
        if let Some(f) = &s.faulted {
            println!("  FAULT: {f}");
        }
        for f in &s.failures {
            println!("  {} failed: {}", f.phase, f.error);
        }
        if !s.is_clean() && !s.log_tail.is_empty() {
            println!(
                "  plugin log (last {} of {} lines):",
                s.log_tail.len(),
                s.log_lines
            );
            for line in &s.log_tail {
                println!("    {line}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse() {
        assert_eq!(parse_color("#ff0000"), Ok(Rgb::new(255, 0, 0)));
        assert_eq!(parse_color("00FF00"), Ok(Rgb::new(0, 255, 0)));
        assert_eq!(parse_color("Blue"), Ok(Rgb::new(0, 0, 255)));
        assert!(parse_color("nope").is_err());
    }

    #[test]
    fn probes_and_params_parse() {
        assert_eq!(
            parse_probe("0x52:185"),
            Ok(RestoreProbe {
                report_id: 0x52,
                len: 185
            })
        );
        assert!(parse_probe("0x152:185").is_err());
        assert!(parse_probe("82").is_err());
        assert_eq!(parse_restore("auto"), Ok(RestoreArg::Auto));
        assert_eq!(restore_mode(&[]), RestoreCheckMode::Off);
        assert_eq!(
            restore_mode(&[
                RestoreArg::Auto,
                RestoreArg::Report(RestoreProbe { report_id: 1, len: 2 })
            ]),
            RestoreCheckMode::DeclaredFeatureReports
        );
        assert_eq!(
            parse_param("LightingMode=Forced"),
            Ok(("LightingMode".into(), serde_json::json!("Forced")))
        );
        assert_eq!(parse_param("speed=3"), Ok(("speed".into(), serde_json::json!(3))));
        assert!(parse_param("=1").is_err());
        assert_eq!(parse_vid_pid("1462:7c91"), Ok((0x1462, 0x7C91)));
    }

    #[test]
    fn diff_ranges_are_compact() {
        assert_eq!(diff_ranges(&[1, 2, 3, 4], &[1, 9, 9, 4]), "0x01..0x02");
        assert_eq!(diff_ranges(&[1, 2], &[1, 2]), "none");
        assert_eq!(diff_ranges(&[1, 2, 3], &[0, 2, 0]), "0x00, 0x02");
    }

    #[test]
    fn filters_match_key_prefixes() {
        assert!(matches_filter("1462:7c91:A1", &[]));
        assert!(matches_filter("1462:7c91:A1", &["1462:7C91".into()]));
        assert!(!matches_filter("1462:7c91:A1", &["1532".into()]));
    }

    #[test]
    fn bench_mock_runs_the_first_party_plugin_end_to_end() {
        let args = RunArgs {
            devices: Vec::new(),
            plugin_dirs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")],
            dry_run: false,
            duration: None,
            keep: false,
            ignore_conflicts: false,
            restore_check: vec![RestoreArg::Report(RestoreProbe {
                report_id: 0x52,
                len: 185,
            })],
            params: Vec::new(),
            brightness: 100,
            stats_every: 0.0,
            json: None,
            verbose: false,
        };
        let mode = Mode::Bench {
            fps: 60.0,
            seconds: 1.0,
            mock: Some((0x1462, 0x7C91)),
        };
        assert_eq!(run(mode, &args).unwrap(), Outcome::Clean);
    }
}
