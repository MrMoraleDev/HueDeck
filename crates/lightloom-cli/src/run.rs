// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom light | rainbow | bench`: the headless M1 pipeline.
//!
//! Null renderer → [`FrameStore`] → one plugin session per supported device
//! (the same catalog → session → device-host path the daemon will use).
//! Nothing is written to hardware while conflicting RGB software runs (it is
//! reported, never terminated), and every session ends with the plugin's
//! `Shutdown(false)` — on Ctrl-C too — unless `--keep` is given.
//!
//! With `--smbus` the SMBus devices (GPU RGB over NvAPI, RAM RGB through the
//! Lightloom I/O broker) join the run:
//! the SMBus plugins scan the buses once before the sessions start, never
//! while SignalRGB, OpenRGB or a vendor lighting service runs and never
//! alongside lightloomd or another `--smbus` run (`--ignore-conflicts` does not
//! apply). Those programs are looked for again every second while the SMBus
//! devices run; one that starts ends the run (every `Shutdown()` runs).

use anyhow::{bail, Context};
use clap::Args;
use lightloom_engine::{ComponentInstance, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::{MockHid, MockSmbus, TransactionRecorder};
use lightloom_hal::trace::{RecordingHid, TraceOptions, TraceRestoreCheck};
use lightloom_hal::{HidBackend, ReportLengths};
use lightloom_plugin_host::catalog::{Catalog, CatalogEntry, ScanOptions};
use lightloom_plugin_host::device::{Brightness, IoKind, LogSink, WriteMode};
use lightloom_plugin_host::discover::{ScanBudget, ScanOutcome};
use lightloom_plugin_host::manager::{Change, DeviceManager, OptionsFn, Present};
use lightloom_plugin_host::session::{
    RestoreCheckMode, RestoreProbe, SessionDevice, SessionOptions, SessionReport,
};
use lightloom_plugin_host::smbus::BusHandle;
use lightloom_render::{CefRenderer, CefRendererConfig, NullRenderer, Pattern, RendererClient};
use lightloom_runtime::hardware::Hardware;
use lightloom_runtime::hotplug::{Hotplug, HOTPLUG_SETTLE};
use lightloom_runtime::smbus::SmbusHardware;
use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(not(windows))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[cfg(not(windows))]
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Options shared by `light`, `rainbow` and `bench`.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// Only drive devices whose key starts with this (`vid:pid` or `vid:pid:serial`, hex; repeatable).
    #[arg(long = "device", value_name = "UID")]
    pub devices: Vec<String>,
    #[command(flatten)]
    pub catalog: CatalogArgs,
    /// Read devices but never write; print the packets that would be sent.
    #[arg(long)]
    pub dry_run: bool,
    /// Place devices, subdevices and component instances on the canvas from a
    /// layout file (schemas/layout.schema.json: placements keyed by endpoint uid,
    /// layout brightness). Endpoints it does not place keep their defaults.
    #[arg(long, value_name = "FILE")]
    pub layout: Option<PathBuf>,
    /// Record every HID operation and what the devices answered into FILE
    /// (JSON), for `lightloom compat replay`. Record with `light` (a solid
    /// colour) to get a trace that replays exactly.
    #[arg(long, value_name = "FILE")]
    pub record: Option<PathBuf>,
    /// Stop after this many seconds (default: until Ctrl-C; `bench` uses --seconds).
    #[arg(long, value_name = "SECS", value_parser = parse_secs)]
    pub duration: Option<f64>,
    /// Exit without calling the plugin's Shutdown(): the device keeps the last colour.
    #[arg(long)]
    pub keep: bool,
    /// Run even when conflicting RGB software (SignalRGB, OpenRGB, the plugin's list) is running.
    /// Never applies to --smbus.
    #[arg(long)]
    pub ignore_conflicts: bool,
    /// Also drive SMBus devices (NVIDIA GPU RGB controllers through NvAPI, RAM through
    /// the Lightloom I/O broker): the SMBus plugins scan the buses first. Refused while
    /// SignalRGB or OpenRGB runs.
    #[arg(long)]
    pub smbus: bool,
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

/// Where plugins come from (shared by `light`, `rainbow`, `bench` and `plugins`).
#[derive(Debug, Clone, Default, Args)]
pub struct CatalogArgs {
    /// Plugin directory (repeatable; later ones win on VID:PID collisions).
    /// Default: $LIGHTLOOM_PLUGINS, else ./plugins.
    #[arg(long = "plugins", value_name = "DIR")]
    pub plugin_dirs: Vec<PathBuf>,
    /// Also use the plugins of a local SignalRGB installation, read in place (never
    /// copied). They are scanned first, so Lightloom's own plugin directories win.
    /// Device runs also take SignalRGB's component setup of each channel (read-only).
    #[arg(long)]
    pub signalrgb: bool,
    /// Load every plugin file again instead of using the plugin index cache.
    #[arg(long)]
    pub no_plugin_cache: bool,
}

impl CatalogArgs {
    /// Directories in scan order: SignalRGB's (when asked), then Lightloom's.
    pub fn dirs(&self) -> anyhow::Result<Vec<PathBuf>> {
        let mut dirs = Vec::new();
        if self.signalrgb {
            let found = lightloom_plugin_host::catalog::signalrgb_plugin_dirs();
            if found.is_empty() {
                bail!("--signalrgb: no SignalRGB plugin directory found on this machine");
            }
            dirs.extend(found);
        }
        if self.plugin_dirs.is_empty() {
            dirs.extend(default_plugin_dirs());
        } else {
            dirs.extend(self.plugin_dirs.iter().cloned());
        }
        Ok(dirs)
    }

    /// Scan the directories (with the index cache unless disabled).
    pub fn scan(&self) -> anyhow::Result<(Catalog, Vec<PathBuf>)> {
        let dirs = self.dirs()?;
        let opts = ScanOptions {
            cache: if self.no_plugin_cache {
                None
            } else {
                lightloom_plugin_host::catalog::default_cache_path()
            },
            ..ScanOptions::default()
        };
        let catalog = Catalog::scan_with(&dirs, &opts);
        if catalog.entries().is_empty() {
            bail!(
                "no device plugin found in {} (use --plugins DIR, LIGHTLOOM_PLUGINS or --signalrgb)",
                dirs.iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok((catalog, dirs))
    }
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
    /// An effect (lightscript) rendered by `lightloom-renderer` (CEF).
    Effect(EffectArgs),
    /// Rainbow for a fixed time, then latency histograms.
    Bench {
        /// Frame rate.
        fps: f64,
        /// Run time.
        seconds: f64,
        /// Drive a scripted in-memory device with this VID:PID instead of hardware.
        mock: Option<(u16, u16)>,
        /// Drive the SMBus plugin with this name (or directory name) on a scripted
        /// in-memory bus instead of hardware.
        mock_smbus: Option<String>,
    },
}

/// `lightloom effect`: which effect and how to render it.
#[derive(Clone, Debug, Args)]
pub struct EffectArgs {
    /// Effect HTML file (a SignalRGB-style lightscript).
    pub effect: PathBuf,
    /// Effect control value, NAME=VALUE (VALUE parsed as JSON, else taken as a string; repeatable).
    #[arg(long = "prop", value_name = "NAME=VALUE", value_parser = parse_param)]
    pub props: Vec<(String, serde_json::Value)>,
    /// Frames per second the effect paints at (1-240).
    #[arg(long, default_value_t = 30.0, value_parser = parse_fps)]
    pub fps: f64,
    /// The `lightloom-renderer` executable (default: $LIGHTLOOM_RENDERER, else next to lightloom).
    #[arg(long, value_name = "PATH")]
    pub renderer: Option<PathBuf>,
    /// Composite on the GPU. The default, software compositing, paints the
    /// 320x200 view at a steadier cadence with less memory (docs/hardware/effects-m3.md)
    /// and works in VMs and remote desktop sessions; GPU compositing can help
    /// effects with heavy canvas filters.
    #[arg(long)]
    pub gpu_compositing: bool,
}

impl Mode {
    fn name(&self) -> &'static str {
        match self {
            Mode::Light(_) => "light",
            Mode::Rainbow { .. } => "rainbow",
            Mode::Effect(_) => "effect",
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
            Mode::Effect(e) => e.fps,
        }
    }

    /// The renderer client that draws this mode.
    fn renderer(&self) -> anyhow::Result<Box<dyn RendererClient>> {
        Ok(match self {
            Mode::Effect(e) => {
                let effect = std::fs::canonicalize(&e.effect)
                    .with_context(|| format!("effect {}", e.effect.display()))?;
                let work_dir = std::env::temp_dir().join(format!("lightloom-effect-{}", std::process::id()));
                Box::new(CefRenderer::new(CefRendererConfig {
                    renderer: renderer_exe(e.renderer.as_deref())?,
                    effect,
                    fps: e.fps.round().clamp(1.0, 240.0) as u32,
                    properties: e.props.iter().cloned().collect(),
                    software_compositing: !e.gpu_compositing,
                    work_dir,
                }))
            }
            _ => Box::new(NullRenderer::new(self.pattern(), self.fps())),
        })
    }
}

/// The `lightloom-renderer` executable: `explicit`, $LIGHTLOOM_RENDERER, or next to this program.
fn renderer_exe(explicit: Option<&std::path::Path>) -> anyhow::Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("LIGHTLOOM_RENDERER") {
        return Ok(PathBuf::from(p));
    }
    let name = if cfg!(windows) {
        "lightloom-renderer.exe"
    } else {
        "lightloom-renderer"
    };
    let beside = std::env::current_exe()?.with_file_name(name);
    if beside.exists() {
        return Ok(beside);
    }
    bail!(
        "{name} was not found next to lightloom: build it with `cargo build --release -p lightloom-renderer \
         --features cef` and pass --renderer PATH (or set LIGHTLOOM_RENDERER)"
    )
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

// Used by the Windows device path (and tests); Linux drives no hardware yet.
#[cfg_attr(not(windows), allow(dead_code))]
/// A scripted device that answers every feature-report id with 1024 zero bytes
/// (report id first), so any HID plugin can run its Initialize against it.
pub(crate) fn permissive_mock(vid: u16, pid: u16) -> anyhow::Result<(Arc<MockHid>, SessionDevice)> {
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
        smbus: None,
        key: format!("{vid:04x}:{pid:04x}:MOCK"),
        vid,
        pid,
        endpoints: vec![info],
    };
    Ok((Arc::new(mock), device))
}

// Used by `bench --mock-smbus` (and tests).
/// A scripted bus for the SMBus plugin `meta`: the first `BrandGPUList()`
/// card's NVIDIA bus with a device answering at every `SmbusAddresses()`
/// address (unset registers read 0xFF); else an AMD PIIX4 bus with one DRAM
/// RGB controller at the first `SmbusAddresses()` address: the ENE-style
/// indexed register file OpenRGB's DRAM detectors identify (0xA0..0xAF read
/// 0..15, "Micron" at 0x1025, a version string at 0x1000).
pub(crate) fn permissive_mock_smbus(meta: &lightloom_plugin_host::PluginMeta) -> BusHandle {
    let rec = TransactionRecorder::new();
    let mut bus = match meta.gpu_ids().first() {
        Some(gpu) => MockSmbus::new("MOCK NVIDIA GPU", lightloom_hal::SmbusBusKind::Nvidia, rec)
            .with_pci(lightloom_hal::PciIds {
                vendor: gpu.vendor,
                device: gpu.device,
                sub_vendor: gpu.sub_vendor,
                sub_device: gpu.sub_device,
            })
            .with_instance("pci01"),
        None => MockSmbus::new("MOCK AMD PIIX4 Bus - 0x0b00", lightloom_hal::SmbusBusKind::Amd, rec)
            .with_pci(lightloom_hal::PciIds {
                vendor: 0x1022,
                device: 0x790B,
                sub_vendor: 0x1022,
                sub_device: 0x790B,
            })
            .with_instance("0b00"),
    };
    if meta.gpu_ids().is_empty() {
        if let Some(&a) = meta.smbus_addresses.first() {
            let ident = (0..16u8).map(|i| (0xA0 + i, i));
            let micron = b"Micron\0"
                .iter()
                .enumerate()
                .map(|(i, b)| (0x1025 + i as u16, *b));
            let version = b"Lightloom mock\0"
                .iter()
                .enumerate()
                .map(|(i, b)| (0x1000 + i as u16, *b));
            bus.add_indexed_device(a, ident, micron.chain(version));
        }
    } else {
        for a in &meta.smbus_addresses {
            bus.add_device(*a, []);
        }
    }
    BusHandle::new(Box::new(bus))
}

/// The SMBus plugin `name` (its `Name()` or its directory's name).
fn smbus_plugin<'c>(catalog: &'c Catalog, name: &str) -> Option<&'c CatalogEntry> {
    catalog.smbus_plugins().find(|e| {
        e.meta.name.eq_ignore_ascii_case(name)
            || e.meta
                .source_path
                .parent()
                .and_then(|d| d.file_name())
                .is_some_and(|d| d.to_string_lossy().eq_ignore_ascii_case(name))
    })
}

fn print_scan(outcomes: &[ScanOutcome]) {
    for o in outcomes {
        let found = o
            .addrs
            .iter()
            .map(|a| format!("0x{a:02x}"))
            .collect::<Vec<_>>()
            .join(", ");
        match &o.error {
            Some(e) => eprintln!("  SMBus scan [{}] {}: {e}", o.bus, o.plugin),
            None => eprintln!(
                "  SMBus scan [{}] {}: {} ({:.0} ms)",
                o.bus,
                o.plugin,
                if found.is_empty() { "nothing".into() } else { found },
                o.took.as_secs_f64() * 1000.0
            ),
        }
    }
}

/// The options of the session for `device`: the run's flags, SignalRGB's
/// channel setup with `--signalrgb`, and a log sink labelled with the device.
fn session_options(
    args: &RunArgs,
    device: &SessionDevice,
    system_info: Option<Arc<lightloom_hal::SystemInfo>>,
    layout: Option<&lightloom_engine::Layout>,
) -> SessionOptions {
    let label = device.key.clone();
    let channel_components = if args.catalog.signalrgb {
        let (channels, notes) = lightloom_runtime::signalrgb::channels(&device.key);
        for note in notes {
            eprintln!("  [{label}] SignalRGB setup: {note}");
        }
        if !channels.is_empty() {
            eprintln!(
                "  [{label}] SignalRGB component setup: {}",
                channels
                    .iter()
                    .map(|(name, list)| format!(
                        "{name} ({} component{})",
                        list.len(),
                        if list.len() == 1 { "" } else { "s" }
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        channels
    } else {
        BTreeMap::new()
    };
    let verbose = args.verbose;
    let sink: LogSink = Arc::new(move |line: &str| {
        if verbose {
            eprintln!("  [{label}] {line}");
        }
    });
    SessionOptions {
        parameters: args.params.iter().cloned().collect(),
        brightness: Brightness {
            global: args.brightness,
            layout: layout.map_or(100, |l| l.brightness.min(100)),
        },
        placement: layout.and_then(|l| l.endpoints.get(&device.key).cloned()),
        placements: layout.map(|l| l.endpoints.clone()).unwrap_or_default(),
        write_mode: if args.dry_run {
            WriteMode::DryRun
        } else {
            WriteMode::Live
        },
        skip_shutdown: args.keep,
        restore_check: restore_mode(&args.restore_check),
        log_sink: Some(sink),
        channel_components,
        system_info,
        ..SessionOptions::default()
    }
}

fn print_change(mode: &Mode, change: &Change) {
    let name = mode.name();
    match change {
        Change::Started { key, plugin } => eprintln!("lightloom {name}: {key} connected -> {plugin}"),
        Change::Removed { key } => eprintln!("lightloom {name}: {key} removed, session stopped"),
        Change::Restarted { key } => eprintln!("lightloom {name}: {key} reconnected, session restarted"),
        Change::Failed { key, error } => eprintln!("lightloom {name}: {key}: cannot start a session: {error}"),
    }
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

impl Drop for Interrupt {
    /// Every session has shut down and the outputs are written (on every return path).
    fn drop(&mut self) {
        self.finished();
    }
}

/// The trace form of a restore-check mode.
fn trace_restore_check(mode: &RestoreCheckMode) -> TraceRestoreCheck {
    match mode {
        RestoreCheckMode::Off => TraceRestoreCheck::Off,
        RestoreCheckMode::DeclaredFeatureReports => TraceRestoreCheck::Declared,
        RestoreCheckMode::Reports(list) => {
            TraceRestoreCheck::Reports(list.iter().map(|p| (p.report_id, p.len)).collect())
        }
    }
}

/// `lightloom plugins --smbus`: the SMBus buses, the plugins offered each, and
/// what their `Scan(bus)` found (this runs the scans: hardware access).
fn list_smbus(catalog: &Catalog, json: bool) -> anyhow::Result<serde_json::Value> {
    // Not even NvAPI is loaded while SignalRGB or OpenRGB runs.
    let blockers = lightloom_runtime::conflicts::smbus_blockers()?;
    if !blockers.is_empty() {
        let text = format!("{} {}", blockers.join(", "), lightloom_runtime::smbus::BLOCKED);
        if !json {
            println!("SMBus: {text}");
        }
        return Ok(serde_json::json!({ "error": text }));
    }
    let hw = SmbusHardware::open();
    let buses: Vec<serde_json::Value> = hw
        .buses()
        .iter()
        .map(|b| {
            let d = b.descriptor();
            serde_json::json!({
                "name": d.name,
                "key": d.key(),
                "kind": lightloom_plugin_host::bus::kind_name(d.kind),
                "plugins": catalog.smbus_candidates(d).iter().map(|e| &e.meta.name).collect::<Vec<_>>(),
            })
        })
        .collect();
    let scan = hw.targets(catalog, &[], &ScanBudget::default());
    if !json {
        for note in hw.notes() {
            println!("SMBus: {note}");
        }
        for b in &buses {
            println!(
                "SMBus bus {} [{}] ({}): plugins {}",
                b["name"].as_str().unwrap_or_default(),
                b["key"].as_str().unwrap_or_default(),
                b["kind"].as_str().unwrap_or_default(),
                b["plugins"]
            );
        }
        match &scan {
            Ok((found, outcomes)) => {
                print_scan(outcomes);
                for p in found {
                    println!("  {}  ->  {}", p.device.key, p.plugin.meta.name);
                }
            }
            Err(e) => println!("SMBus scan not run: {e:#}"),
        }
    }
    Ok(serde_json::json!({
        "notes": hw.notes(),
        "buses": buses,
        "scan": match &scan {
            Ok((found, outcomes)) => serde_json::json!({
                "outcomes": outcomes,
                "devices": found.iter().map(|p| serde_json::json!({
                    "device": p.device.key,
                    "plugin": p.plugin.meta.source_path,
                })).collect::<Vec<_>>(),
            }),
            Err(e) => serde_json::json!({ "error": format!("{e:#}") }),
        },
    }))
}

/// `lightloom plugins`: the plugins found and, for each connected HID device
/// (read-only enumeration), the plugin that would drive it; with `smbus`, the
/// SMBus buses and what the SMBus plugins' scans find on them.
pub fn list_plugins(args: &CatalogArgs, smbus: bool, json: bool) -> anyhow::Result<()> {
    let started = Instant::now();
    let (catalog, dirs) = args.scan()?;
    let scan_ms = started.elapsed().as_secs_f64() * 1000.0;
    let stats = catalog.stats();
    let devices = lightloom_runtime::hardware::connected_devices().unwrap_or_else(|e| {
        eprintln!("lightloom: device enumeration unavailable: {e}");
        Vec::new()
    });
    let matched: Vec<(String, Option<&CatalogEntry>)> = devices
        .iter()
        .map(|(key, vid, pid)| (key.clone(), catalog.lookup(*vid, *pid)))
        .collect();
    let smbus_view = if smbus {
        Some(list_smbus(&catalog, json)?)
    } else {
        None
    };
    if json {
        let value = serde_json::json!({
            "smbus": smbus_view,
            "directories": dirs,
            "files": stats.files,
            "cached": stats.cached,
            "loaded": stats.loaded,
            "scan_ms": scan_ms,
            "plugins": catalog.entries().iter().map(|e| serde_json::json!({
                "name": e.meta.name,
                "file": e.meta.source_path,
                "vendor_id": e.meta.vendor_id,
                "product_ids": e.meta.product_ids,
            })).collect::<Vec<_>>(),
            "devices": matched.iter().map(|(key, plugin)| serde_json::json!({
                "device": key,
                "plugin": plugin.map(|p| &p.meta.source_path),
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!(
        "{} plugins from {} directories ({} files: {} cached, {} loaded) in {scan_ms:.0} ms",
        catalog.entries().len(),
        dirs.len(),
        stats.files,
        stats.cached,
        stats.loaded
    );
    for (key, plugin) in &matched {
        match plugin {
            Some(p) => println!("  {key}  ->  {}  ({})", p.meta.name, p.meta.source_path.display()),
            None => println!("  {key}  ->  no plugin"),
        }
    }
    Ok(())
}

/// `lightloom effects`: every `*.html` effect in `dirs` (default `./effects`) with its controls.
pub fn list_effects(dirs: &[PathBuf], json: bool) -> anyhow::Result<()> {
    let dirs = if dirs.is_empty() {
        vec![PathBuf::from("effects")]
    } else {
        dirs.to_vec()
    };
    let mut effects = Vec::new();
    for dir in &dirs {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("html")))
            .collect();
        files.sort();
        for path in files {
            let html = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            let mut meta = lightloom_effects::parse_effect_meta(&String::from_utf8_lossy(&html));
            meta.id = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            effects.push((path, meta));
        }
    }
    if json {
        let list: Vec<_> = effects
            .iter()
            .map(|(path, meta)| serde_json::json!({ "path": path, "meta": meta }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(());
    }
    for (path, meta) in &effects {
        println!("{}  ({})", meta.title, path.display());
        if !meta.description.is_empty() {
            println!("    {}", meta.description);
        }
        for c in &meta.properties {
            let values = if c.values.is_empty() {
                String::new()
            } else {
                format!(" [{}]", c.values.join(", "))
            };
            println!(
                "    --prop {}=…  {:?}, default {}{values}",
                c.property,
                c.control_type,
                c.default.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
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
    if args.record.is_some() && args.dry_run {
        bail!("--record needs the devices' real answers: drop --dry-run");
    }
    let started = Instant::now();
    let (catalog, _) = args.catalog.scan()?;
    // Subscribe before enumerating, so a device plugged in meanwhile is not missed.
    let mocked = matches!(
        &mode,
        Mode::Bench { mock: Some(_), .. }
            | Mode::Bench {
                mock_smbus: Some(_),
                ..
            }
    );
    let hotplug = if mocked {
        Hotplug::inactive()
    } else {
        Hotplug::start()
    };
    let scan_budget = ScanBudget {
        mode: if args.dry_run {
            WriteMode::DryRun
        } else {
            WriteMode::Live
        },
        ..ScanBudget::default()
    };

    // SMBus devices are found once, before any session starts; hotplug only
    // follows HID devices, so the SMBus ones are handed to every reconcile.
    let mut smbus_present: Vec<Present> = Vec::new();
    let mut _smbus_hardware: Option<SmbusHardware> = None;
    // Held until the run ends: no other Lightloom process drives SMBus meanwhile.
    let mut _smbus_claim: Option<lightloom_runtime::conflicts::SmbusClaim> = None;
    let (backend, hardware, initial) = match &mode {
        Mode::Bench {
            mock: Some((vid, pid)),
            ..
        } => {
            let (vid, pid) = (*vid, *pid);
            let plugin = catalog
                .lookup(vid, pid)
                .with_context(|| format!("no plugin claims {vid:04x}:{pid:04x}"))?
                .clone();
            let (mock, device) = permissive_mock(vid, pid)?;
            let backend: Arc<dyn HidBackend> = mock;
            (backend, None, vec![Present { device, plugin }])
        }
        Mode::Bench {
            mock_smbus: Some(name),
            ..
        } => {
            let plugin =
                smbus_plugin(&catalog, name).with_context(|| format!("no SMBus plugin named {name}"))?;
            let hw = SmbusHardware::from_buses(vec![permissive_mock_smbus(&plugin.meta)]);
            let (found, outcomes) = hw.scan(&catalog, &args.devices, &scan_budget);
            print_scan(&outcomes);
            let found: Vec<Present> = found
                .into_iter()
                .filter(|p| p.plugin.meta.source_path == plugin.meta.source_path)
                .collect();
            if found.is_empty() {
                bail!("{name}: Scan(bus) found no device on the scripted bus");
            }
            _smbus_hardware = Some(hw);
            smbus_present = found.clone();
            let backend: Arc<dyn HidBackend> = Arc::new(MockHid::new());
            (backend, None, found)
        }
        _ => {
            let hardware = Hardware::open()?;
            let (mut targets, total) = hardware.targets(&catalog, &args.devices)?;
            if args.smbus {
                // Checked before NvAPI is even loaded; never bypassed (Scan() itself writes).
                let blockers = lightloom_runtime::conflicts::smbus_blockers()?;
                if !blockers.is_empty() {
                    bail!("{} {}", blockers.join(", "), lightloom_runtime::smbus::BLOCKED);
                }
                // The daemon may drive the same buses: never scan alongside it either.
                let daemon = lightloom_runtime::conflicts::running_conflicts(&[], &["lightloomd.exe"])?;
                if daemon.iter().any(|n| n.eq_ignore_ascii_case("lightloomd.exe")) {
                    bail!(
                        "lightloomd is running: stop it before using --smbus (both would drive the same buses)"
                    );
                }
                // Nor alongside another `lightloom --smbus` run (or a daemon started meanwhile).
                _smbus_claim = Some(lightloom_runtime::conflicts::claim_smbus()?);
                let hw = SmbusHardware::open();
                for note in hw.notes() {
                    eprintln!("lightloom {}: SMBus: {note}", mode.name());
                }
                // Never bypassed: Scan() itself writes.
                let (found, outcomes) = hw.targets(&catalog, &args.devices, &scan_budget)?;
                print_scan(&outcomes);
                smbus_present = found.clone();
                targets.extend(found);
                _smbus_hardware = Some(hw);
            }
            if targets.is_empty() {
                bail!(
                    "no supported device found: {total} device(s) match the {} loaded plugin(s){}{}",
                    catalog.entries().len(),
                    if args.devices.is_empty() {
                        ""
                    } else {
                        " (after --device filters)"
                    },
                    if args.smbus {
                        ", and no SMBus device was found"
                    } else {
                        ""
                    }
                );
            }
            (hardware.backend(), Some(hardware), targets)
        }
    };

    let recorder = args.record.as_ref().map(|_| {
        let r = Arc::new(RecordingHid::new(
            Arc::clone(&backend),
            format!(
                "lightloom {} {}",
                mode.name(),
                std::env::args().skip(2).collect::<Vec<_>>().join(" ")
            ),
        ));
        r.set_conditions(
            match mode {
                Mode::Light(c) => Some(c.to_hex()),
                _ => None,
            },
            args.params.iter().cloned().collect(),
        );
        r.set_options(TraceOptions {
            brightness: args.brightness,
            skip_shutdown: args.keep,
            restore_check: trace_restore_check(&restore_mode(&args.restore_check)),
        });
        for t in &initial {
            r.note_device(&t.device.key, t.device.vid, t.device.pid, &t.device.endpoints);
        }
        r
    });
    let backend: Arc<dyn HidBackend> = match &recorder {
        Some(r) => r.clone(),
        None => backend,
    };

    if hardware.is_some() {
        let conflicts = lightloom_runtime::conflicts::running_conflicts(&initial, &["lightloomd.exe"])?;
        if !conflicts.is_empty() && !args.ignore_conflicts {
            bail!(
                "conflicting RGB software is running: {}. Close it first (Lightloom never terminates other \
                 applications), or pass --ignore-conflicts",
                conflicts.join(", ")
            );
        }
    }

    for t in &initial {
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
    let mut renderer = mode.renderer()?;
    renderer.start(Arc::clone(&store))?;

    let system_info = lightloom_runtime::system::machine_info().map(Arc::new);
    let layout = match &args.layout {
        Some(path) => {
            let text =
                std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            Some(
                serde_json::from_str::<lightloom_engine::Layout>(&text)
                    .with_context(|| format!("{} is not a layout", path.display()))?,
            )
        }
        None => None,
    };
    if let Some(r) = &recorder {
        r.set_system_info(system_info.as_deref().cloned());
    }
    let option_args = args.clone();
    let option_recorder = recorder.clone();
    let options: OptionsFn = Box::new(move |device: &SessionDevice, _: &CatalogEntry| {
        let opts = session_options(&option_args, device, system_info.clone(), layout.as_ref());
        if let Some(r) = &option_recorder {
            r.set_channels(
                &device.key,
                opts.channel_components
                    .iter()
                    .map(|(name, list)| {
                        let array = list.iter().map(ComponentInstance::to_json).collect();
                        (name.clone(), serde_json::Value::Array(array))
                    })
                    .collect(),
            );
        }
        opts
    });
    let mut manager = DeviceManager::new(backend, Arc::clone(&store), options);
    for change in manager.reconcile(initial) {
        if let Change::Failed { key, error } = change {
            // Do not leave the sessions already started running detached.
            let (_, failed) = manager.finish();
            renderer.stop();
            bail!(
                "starting the device session for {key}: {error} (the sessions already started were stopped{})",
                if failed.is_empty() { "" } else { ", some uncleanly" }
            );
        }
    }
    // Devices plugged in, out or back in while the run goes on are followed.
    let mut settle: Option<Instant> = None;
    // While SMBus devices run, the programs that forbid SMBus are looked for
    // again every SMBUS_RECHECK; one that shows up ends the run.
    let mut last_smbus_check = Instant::now();
    let mut smbus_stopped: Option<String> = None;

    let limit = match mode {
        Mode::Bench { seconds, .. } => Some(seconds),
        _ => args.duration,
    }
    .filter(|s| s.is_finite() && *s > 0.0)
    .map(Duration::from_secs_f64);
    let stats_every = (args.stats_every.is_finite() && args.stats_every > 0.0)
        .then(|| Duration::from_secs_f64(args.stats_every));
    let mut max_rss = lightloom_runtime::system::rss_bytes();
    let mut first_write: Option<f64> = None;
    let mut last_stats = Instant::now();
    let mut last_rss = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let first = manager
            .sessions()
            .filter_map(|h| {
                h.live()
                    .first_write_ms
                    .map(|ms| h.started().duration_since(started).as_secs_f64() * 1000.0 + ms)
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
            max_rss = max_rss.max(lightloom_runtime::system::rss_bytes());
        }
        if let Some(every) = stats_every {
            if last_stats.elapsed() >= every {
                last_stats = Instant::now();
                for h in manager.sessions() {
                    let l = h.live();
                    eprintln!(
                        "  {:>7.1}s {}: {:?} frames={} render_errors={} hid_errors={} rss={}",
                        started.elapsed().as_secs_f64(),
                        h.key(),
                        l.state,
                        l.frames,
                        l.render_errors,
                        l.hid_errors,
                        lightloom_runtime::system::rss_bytes()
                            .map(|b| format!("{:.1} MB", b as f64 / 1e6))
                            .unwrap_or_else(|| "n/a".into())
                    );
                }
            }
        }
        if hotplug.changed() {
            // One plug produces a burst of events: wait until it is quiet.
            settle = Some(Instant::now());
        } else if settle.is_none() && manager.has_disconnected() {
            settle = Some(Instant::now());
        }
        if let (Some(since), Some(hw)) = (settle, &hardware) {
            if since.elapsed() >= HOTPLUG_SETTLE {
                settle = None;
                match hw.targets(&catalog, &args.devices) {
                    Ok((mut present, _)) => {
                        present.extend(smbus_present.iter().cloned());
                        if let Some(r) = &recorder {
                            for t in &present {
                                r.note_device(&t.device.key, t.device.vid, t.device.pid, &t.device.endpoints);
                            }
                        }
                        for change in manager.reconcile(present) {
                            print_change(&mode, &change);
                        }
                    }
                    Err(e) => eprintln!("lightloom {}: enumerating again failed: {e:#}", mode.name()),
                }
            }
        }
        if hardware.is_some()
            && !smbus_present.is_empty()
            && last_smbus_check.elapsed() >= lightloom_runtime::conflicts::SMBUS_RECHECK
        {
            last_smbus_check = Instant::now();
            match lightloom_runtime::conflicts::smbus_blockers() {
                Ok(blockers) if !blockers.is_empty() => {
                    eprintln!(
                        "lightloom {}: {} started: Lightloom never drives SMBus/GPU I2C alongside it, stopping",
                        mode.name(),
                        blockers.join(", ")
                    );
                    smbus_stopped = Some(blockers.join(", "));
                    break;
                }
                Ok(_) => {}
                Err(e) => eprintln!(
                    "lightloom {}: cannot check for conflicting software: {e:#}",
                    mode.name()
                ),
            }
        }
        let interrupted = interrupt.requested();
        let expired = limit.is_some_and(|l| started.elapsed() >= l);
        // Every session ended on its own (when devices may come back, a run
        // whose devices were all unplugged waits for them instead).
        let all_done = manager.all_finished() && (!hotplug.active() || manager.sessions().next().is_some());
        if interrupted || expired || all_done {
            if interrupted {
                eprintln!("lightloom {}: interrupted, shutting down", mode.name());
            }
            break;
        }
    }
    let (sessions, failed_sessions) = manager.finish();
    renderer.stop();
    if let Some(line) = renderer.status_line() {
        eprintln!("lightloom {}: {line}", mode.name());
    }
    max_rss = max_rss.max(lightloom_runtime::system::rss_bytes());
    // A held console close/log-off/shutdown is released only when `interrupt` drops,
    // after the trace and the JSON report are written.
    if let (Some(r), Some(path)) = (&recorder, &args.record) {
        for s in &sessions {
            r.add_session(&s.device, s.frames);
        }
        let trace = r.trace();
        std::fs::write(path, serde_json::to_string_pretty(&trace)?)
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!(
            "lightloom {}: recorded {} HID operations to {}",
            mode.name(),
            trace.events.len(),
            path.display()
        );
    }
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
    if let Some(blockers) = &smbus_stopped {
        eprintln!("lightloom {}: stopped because {blockers} started", mode.name());
    }
    Ok(if report.is_clean() && smbus_stopped.is_none() {
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
        println!(
            "- {} ({}){}",
            s.device,
            s.name,
            if s.device_removed {
                " [device removed during the run]"
            } else {
                ""
            }
        );
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
        if let Some(b) = &s.smbus {
            println!(
                "  SMBus: {} operations, {} writes, {} errors, {} refused{}",
                b.ops,
                b.writes,
                b.errors,
                b.denied,
                b.last_error
                    .as_ref()
                    .map(|e| format!("; last: {e}"))
                    .unwrap_or_default()
            );
            if r.dry_run {
                for w in b.dry_run_log.iter().take(6) {
                    println!("  would write: {}", hex(w));
                }
                if b.dry_run_log.len() > 6 {
                    println!("  ... {} more recorded writes", b.dry_run_log.len() - 6);
                }
            }
        } else {
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
        }
        if let Some(ms) = s.first_write_ms {
            println!("  first write {ms:.1} ms after session start");
        }
        if s.canvas_to_wire.count > 0 {
            println!(
                "  canvas to wire p50 {:.2} ms p99 {:.2} ms max {:.2} ms ({} frames)",
                s.canvas_to_wire.p50_ms,
                s.canvas_to_wire.p99_ms,
                s.canvas_to_wire.max_ms,
                s.canvas_to_wire.count
            );
        }
        if !s.subdevices.is_empty() {
            println!(
                "  subdevices: {}",
                s.subdevices
                    .iter()
                    .map(|d| format!("{} ({}x{})", d.name, d.size[0], d.size[1]))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        for c in &s.channels {
            println!(
                "  channel {}: {} LEDs (limit {}){}",
                c.name,
                c.led_count,
                c.led_limit,
                if c.components.is_empty() {
                    String::new()
                } else {
                    format!(
                        ": {}",
                        c.components
                            .iter()
                            .map(|x| format!("{} [{}]", x.name, x.led_count))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            );
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
    fn bench_mock_runs_the_first_party_plugin_end_to_end() {
        let args = RunArgs {
            devices: Vec::new(),
            catalog: CatalogArgs {
                plugin_dirs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")],
                signalrgb: false,
                no_plugin_cache: true,
            },
            dry_run: false,
            layout: None,
            record: None,
            duration: None,
            keep: false,
            ignore_conflicts: false,
            smbus: false,
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
            mock_smbus: None,
        };
        assert_eq!(run(mode, &args).unwrap(), Outcome::Clean);
    }

    fn mock_args(json: Option<PathBuf>) -> RunArgs {
        RunArgs {
            devices: Vec::new(),
            catalog: CatalogArgs {
                plugin_dirs: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")],
                signalrgb: false,
                no_plugin_cache: true,
            },
            dry_run: false,
            layout: None,
            record: None,
            duration: None,
            keep: false,
            ignore_conflicts: false,
            smbus: false,
            restore_check: Vec::new(),
            params: Vec::new(),
            brightness: 100,
            stats_every: 0.0,
            json,
            verbose: false,
        }
    }

    #[test]
    fn bench_mock_smbus_runs_msi_gpu_end_to_end() {
        let out = std::env::temp_dir().join(format!("lightloom-bench-smbus-{}.json", std::process::id()));
        let mode = Mode::Bench {
            fps: 60.0,
            seconds: 1.0,
            mock: None,
            mock_smbus: Some("msi-gpu".into()),
        };
        assert_eq!(run(mode, &mock_args(Some(out.clone()))).unwrap(), Outcome::Clean);
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let s = &report["sessions"][0];
        assert_eq!(s["name"], "MSI GPU");
        assert_eq!(s["device"], "smbus:10de:2504:1462:3903@pci01:68");
        assert!(s["smbus"]["writes"].as_u64().unwrap() >= 6, "{s}");
        let _ = std::fs::remove_file(&out);
        let unknown = Mode::Bench {
            fps: 60.0,
            seconds: 1.0,
            mock: None,
            mock_smbus: Some("nope".into()),
        };
        assert!(run(unknown, &mock_args(None)).is_err());
    }

    #[test]
    fn bench_mock_smbus_runs_crucial_end_to_end() {
        let out = std::env::temp_dir().join(format!("lightloom-bench-ram-{}.json", std::process::id()));
        let mode = Mode::Bench {
            fps: 60.0,
            seconds: 1.0,
            mock: None,
            mock_smbus: Some("crucial-ballistix".into()),
        };
        assert_eq!(run(mode, &mock_args(Some(out.clone()))).unwrap(), Outcome::Clean);
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let sessions = report["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1, "{report}");
        let s = &sessions[0];
        assert_eq!(s["name"], "Crucial Ballistix DRAM");
        assert_eq!(s["device"], "smbus:1022:790b:1022:790b@0b00:20");
        // Initialize alone is 68 register writes (two transfers each).
        assert!(s["smbus"]["writes"].as_u64().unwrap() >= 136 + 6, "{s}");
        assert_eq!(s["smbus"]["errors"], 0, "{s}");
        let _ = std::fs::remove_file(&out);
    }
}
